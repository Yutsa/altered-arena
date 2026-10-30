//! The imitation model: a linear score of each candidate over its features, a softmax over the
//! candidates of a decision. Two levels: shared weights `w` (all players) and, for each player,
//! offsets `u_p` on top of them. Trained by maximum likelihood of the recorded answers, with L2
//! regularization `l_shared / 2 |w|^2 + l_player / 2 sum_p |u_p|^2`: a player with few
//! decisions keeps offsets close to zero, i.e. plays like the shared model.

use crate::dataset::Decision;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hyper {
    pub l_shared: f64,
    pub l_player: f64,
    pub iters: usize,
    /// Weight of a decision in the loss, as a power of 1 / candidates (0: every decision counts 1)
    pub cand_power: f64,
    /// Per feature: l_player is multiplied by this (empty: 1 everywhere)
    #[serde(skip)]
    pub player_mult: Vec<f64>,
}

impl Default for Hyper {
    fn default() -> Self {
        Hyper { l_shared: 3.0, l_player: 30.0, iters: 300, cand_power: 0.0, player_mult: vec![] }
    }
}

/// Decisions as sparse rows, features scaled
pub struct Matrix {
    pub nfeat: usize,
    pub players: Vec<String>,
    /// Per decision: player index, first candidate, labels
    pub dec_player: Vec<usize>,
    pub dec_start: Vec<usize>,
    /// Per candidate: first nonzero; label
    pub cand_start: Vec<usize>,
    pub label: Vec<bool>,
    pub idx: Vec<u32>,
    pub val: Vec<f32>,
    /// The scale each raw feature value was divided by
    pub scale: Vec<f64>,
}

impl Matrix {
    pub fn ndec(&self) -> usize {
        self.dec_player.len()
    }

    /// Scores of the candidates of decision `d` under parameters `theta`
    pub fn scores(&self, theta: &[f64], d: usize, out: &mut Vec<f64>) {
        out.clear();
        let f = self.nfeat;
        let p = self.dec_player[d];
        let off = f * (1 + p);
        for c in self.dec_start[d]..self.dec_start[d + 1] {
            let mut s = 0.0;
            for k in self.cand_start[c]..self.cand_start[c + 1] {
                let j = self.idx[k] as usize;
                s += self.val[k] as f64 * (theta[j] + theta[off + j]);
            }
            out.push(s);
        }
    }
}

/// Builds the matrix of `decs` (`keep`: the features used; `scale`: given, or computed from `decs`)
pub fn matrix(decs: &[&Decision], nfeat: usize, keep: &[bool], players: &[String], scale: Option<&[f64]>) -> Matrix {
    let pindex: HashMap<&str, usize> = players.iter().enumerate().map(|(i, p)| (p.as_str(), i)).collect();
    let scale: Vec<f64> = match scale {
        Some(s) => s.to_vec(),
        None => {
            // Typical size of a feature where it is not zero
            let mut sq = vec![0.0f64; nfeat];
            let mut n = vec![0usize; nfeat];
            for d in decs {
                for row in &d.feats {
                    for &(j, v) in row {
                        sq[j as usize] += (v as f64) * (v as f64);
                        n[j as usize] += 1;
                    }
                }
            }
            (0..nfeat).map(|j| if n[j] > 0 { (sq[j] / n[j] as f64).sqrt().max(1e-6) } else { 1.0 }).collect()
        }
    };
    let mut m = Matrix { nfeat, players: players.to_vec(), dec_player: vec![], dec_start: vec![0], cand_start: vec![0], label: vec![], idx: vec![], val: vec![], scale: scale.clone() };
    for d in decs {
        m.dec_player.push(pindex[d.player.as_str()]);
        for (c, row) in d.feats.iter().enumerate() {
            // Repeated names add up
            let mut acc: Vec<(u32, f32)> = row.iter().filter(|(j, _)| keep[*j as usize]).map(|&(j, v)| (j, (v as f64 / scale[j as usize]) as f32)).collect();
            acc.sort_by_key(|x| x.0);
            let mut merged: Vec<(u32, f32)> = vec![];
            for (j, v) in acc {
                match merged.last_mut() {
                    Some(l) if l.0 == j => l.1 += v,
                    _ => merged.push((j, v)),
                }
            }
            for (j, v) in merged {
                if v != 0.0 {
                    m.idx.push(j);
                    m.val.push(v);
                }
            }
            m.cand_start.push(m.idx.len());
            m.label.push(d.labels[c]);
        }
        m.dec_start.push(m.cand_start.len() - 1);
    }
    m
}

fn logsumexp(x: &[f64]) -> f64 {
    let mx = x.iter().cloned().fold(f64::MIN, f64::max);
    mx + x.iter().map(|v| (v - mx).exp()).sum::<f64>().ln()
}

/// Loss and gradient (decisions without a labelled candidate are left out)
pub fn loss_grad(m: &Matrix, theta: &[f64], h: &Hyper, grad: &mut [f64]) -> f64 {
    let f = m.nfeat;
    grad.iter_mut().for_each(|g| *g = 0.0);
    let mut loss = 0.0;
    let mut s = vec![];
    let mut gs = vec![];
    for d in 0..m.ndec() {
        let (a, b) = (m.dec_start[d], m.dec_start[d + 1]);
        if !(a..b).any(|c| m.label[c]) {
            continue;
        }
        let wd = if h.cand_power != 0.0 { ((b - a) as f64).powf(-h.cand_power) } else { 1.0 };
        m.scores(theta, d, &mut s);
        let lz = logsumexp(&s);
        let lab: Vec<f64> = (a..b).filter(|&c| m.label[c]).map(|c| s[c - a]).collect();
        let ll = logsumexp(&lab);
        loss += wd * (lz - ll);
        gs.clear();
        for c in a..b {
            let p = (s[c - a] - lz).exp();
            let q = if m.label[c] { (s[c - a] - ll).exp() } else { 0.0 };
            gs.push(wd * (p - q));
        }
        let off = f * (1 + m.dec_player[d]);
        for c in a..b {
            let g = gs[c - a];
            if g == 0.0 {
                continue;
            }
            for k in m.cand_start[c]..m.cand_start[c + 1] {
                let j = m.idx[k] as usize;
                let x = g * m.val[k] as f64;
                grad[j] += x;
                grad[off + j] += x;
            }
        }
    }
    for j in 0..f {
        loss += 0.5 * h.l_shared * theta[j] * theta[j];
        grad[j] += h.l_shared * theta[j];
    }
    for j in f..theta.len() {
        let l = h.l_player * h.player_mult.get(j % f).copied().unwrap_or(1.0);
        loss += 0.5 * l * theta[j] * theta[j];
        grad[j] += l * theta[j];
    }
    loss
}

/// L-BFGS from zero
pub fn train(m: &Matrix, h: &Hyper) -> Vec<f64> {
    let n = m.nfeat * (1 + m.players.len());
    let mut x = vec![0.0; n];
    let mut g = vec![0.0; n];
    let mut fx = loss_grad(m, &x, h, &mut g);
    let hist = 10;
    let mut ss: Vec<Vec<f64>> = vec![];
    let mut ys: Vec<Vec<f64>> = vec![];
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut gn = vec![0.0; n];
    for _ in 0..h.iters {
        // Two-loop recursion
        let mut q = g.clone();
        let k = ss.len();
        let mut alpha = vec![0.0; k];
        for i in (0..k).rev() {
            let rho = 1.0 / dot(&ys[i], &ss[i]);
            alpha[i] = rho * dot(&ss[i], &q);
            for (qj, yj) in q.iter_mut().zip(&ys[i]) {
                *qj -= alpha[i] * yj;
            }
        }
        let gamma = if k > 0 { dot(&ss[k - 1], &ys[k - 1]) / dot(&ys[k - 1], &ys[k - 1]) } else { 1.0 / dot(&g, &g).sqrt().max(1e-12) };
        q.iter_mut().for_each(|v| *v *= gamma);
        for i in 0..k {
            let rho = 1.0 / dot(&ys[i], &ss[i]);
            let beta = rho * dot(&ys[i], &q);
            for (qj, sj) in q.iter_mut().zip(&ss[i]) {
                *qj += (alpha[i] - beta) * sj;
            }
        }
        // Direction -q, backtracking line search (Armijo)
        let slope = -dot(&g, &q);
        if slope >= 0.0 {
            ss.clear();
            ys.clear();
            continue;
        }
        let mut t = 1.0;
        let mut xn = vec![0.0; n];
        let mut fnew;
        loop {
            for j in 0..n {
                xn[j] = x[j] - t * q[j];
            }
            fnew = loss_grad(m, &xn, h, &mut gn);
            if fnew <= fx + 1e-4 * t * slope || t < 1e-10 {
                break;
            }
            t *= 0.5;
        }
        let s: Vec<f64> = (0..n).map(|j| xn[j] - x[j]).collect();
        let y: Vec<f64> = (0..n).map(|j| gn[j] - g[j]).collect();
        let improved = fx - fnew;
        x.copy_from_slice(&xn);
        g.copy_from_slice(&gn);
        let done = improved.abs() < 1e-7 * fx.abs().max(1.0);
        fx = fnew;
        if dot(&s, &y) > 1e-12 {
            ss.push(s);
            ys.push(y);
            if ss.len() > hist {
                ss.remove(0);
                ys.remove(0);
            }
        }
        if done {
            break;
        }
    }
    x
}

/// One decision's prediction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pred {
    pub top1: bool,
    pub top3: bool,
    /// log P(recorded answer), None when it is not a candidate
    pub logp: Option<f64>,
    /// The model's best candidate
    pub best: usize,
}

pub fn predict(m: &Matrix, theta: &[f64], d: usize) -> Pred {
    let mut s = vec![];
    m.scores(theta, d, &mut s);
    let a = m.dec_start[d];
    let mut order: Vec<usize> = (0..s.len()).collect();
    order.sort_by(|&x, &y| s[y].partial_cmp(&s[x]).unwrap_or(std::cmp::Ordering::Equal).then(x.cmp(&y)));
    let lab = |c: usize| m.label[a + c];
    let lz = logsumexp(&s);
    let ls: Vec<f64> = (0..s.len()).filter(|&c| lab(c)).map(|c| s[c]).collect();
    Pred { top1: lab(order[0]), top3: order.iter().take(3).any(|&c| lab(c)), logp: if ls.is_empty() { None } else { Some(logsumexp(&ls) - lz) }, best: order[0] }
}
