//! Leave-one-game-out cross-validation: for each game, the model is trained on every other game
//! (shared weights and offsets alike) and predicts both players' decisions in the held-out game.
//! Each game of a player is held out once, so every decision gets a held-out prediction.

use crate::dataset::{Baseline, Decision};
use crate::model::{self, Hyper, Pred};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The features kept: none whose name starts with a prefix of `drop` (`*x`: ends with x,
/// `~x`: contains x)
pub fn keep(names: &[String], drop: &[String]) -> Vec<bool> {
    let hit = |n: &str, p: &str| match p.chars().next() {
        None => false,
        Some('*') => n.ends_with(&p[1..]),
        Some('~') => n.contains(&p[1..]),
        _ => n.starts_with(p),
    };
    names.iter().map(|n| !drop.iter().any(|p| hit(n, p))).collect()
}

pub fn players(decs: &[Decision]) -> Vec<String> {
    let mut p: Vec<String> = decs.iter().map(|d| d.player.clone()).collect();
    p.sort();
    p.dedup();
    p
}

/// Runs `f` on every item with `threads` threads (256 MB stacks)
pub fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, || loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= items.len() {
                        break;
                    }
                    let r = f(&items[i]);
                    out.lock().unwrap()[i] = Some(r);
                })
                .unwrap();
        }
    });
    out.into_inner().unwrap().into_iter().map(|x| x.unwrap()).collect()
}

/// Held-out predictions of every decision (in the order of `decs`)
pub fn logo(decs: &[Decision], nfeat: usize, keep: &[bool], h: &Hyper, threads: usize) -> Vec<Pred> {
    let players = players(decs);
    let mut tables: Vec<i64> = decs.iter().map(|d| d.table).collect();
    tables.sort();
    tables.dedup();
    let folds = par_map(&tables, threads, |&t| {
        let train: Vec<&Decision> = decs.iter().filter(|d| d.table != t).collect();
        let test_idx: Vec<usize> = (0..decs.len()).filter(|&i| decs[i].table == t).collect();
        let test: Vec<&Decision> = test_idx.iter().map(|&i| &decs[i]).collect();
        let mtrain = model::matrix(&train, nfeat, keep, &players, None);
        let theta = model::train(&mtrain, h);
        let mtest = model::matrix(&test, nfeat, keep, &players, Some(&mtrain.scale));
        let preds: Vec<(usize, Pred)> = (0..test.len()).map(|k| (test_idx[k], model::predict(&mtest, &theta, k))).collect();
        preds
    });
    let mut out: Vec<Option<Pred>> = vec![None; decs.len()];
    for f in folds {
        for (i, p) in f {
            out[i] = Some(p);
        }
    }
    out.into_iter().map(|p| p.unwrap()).collect()
}

/// Held-out predictions of an ensemble: one model per dataset (the same decisions, each with its
/// own redeal of the hidden cards), candidate scores averaged
pub fn logo_ensemble(sets: &[(Vec<String>, Vec<Decision>)], drop: &[String], h: &Hyper, threads: usize) -> Vec<Pred> {
    let decs = &sets[0].1;
    let players = players(decs);
    let mut tables: Vec<i64> = decs.iter().map(|d| d.table).collect();
    tables.sort();
    tables.dedup();
    let folds = par_map(&tables, threads, |&t| {
        let test_idx: Vec<usize> = (0..decs.len()).filter(|&i| decs[i].table == t).collect();
        let mut sum: Vec<Vec<f64>> = vec![vec![]; test_idx.len()];
        let mut first: Option<model::Matrix> = None;
        for (names, ds) in sets {
            let keep = keep(names, drop);
            let train: Vec<&Decision> = ds.iter().filter(|d| d.table != t).collect();
            let test: Vec<&Decision> = test_idx.iter().map(|&i| &ds[i]).collect();
            let mtrain = model::matrix(&train, names.len(), &keep, &players, None);
            let theta = model::train(&mtrain, h);
            let mtest = model::matrix(&test, names.len(), &keep, &players, Some(&mtrain.scale));
            let mut s = vec![];
            for k in 0..test.len() {
                mtest.scores(&theta, k, &mut s);
                if sum[k].is_empty() {
                    sum[k] = vec![0.0; s.len()];
                }
                for (x, y) in sum[k].iter_mut().zip(&s) {
                    *x += y / sets.len() as f64;
                }
            }
            first.get_or_insert(mtest);
        }
        let m = first.unwrap();
        (0..test_idx.len()).map(|k| (test_idx[k], model::pred_of(&m, k, &sum[k]))).collect::<Vec<_>>()
    });
    let mut out: Vec<Option<Pred>> = vec![None; decs.len()];
    for f in folds {
        for (i, p) in f {
            out[i] = Some(p);
        }
    }
    out.into_iter().map(|p| p.unwrap()).collect()
}

/// Predictions of the model trained on every decision (training-set agreement)
pub fn train_fit(decs: &[Decision], nfeat: usize, keep: &[bool], h: &Hyper) -> (Vec<Pred>, Vec<f64>, model::Matrix) {
    let players = players(decs);
    let all: Vec<&Decision> = decs.iter().collect();
    let m = model::matrix(&all, nfeat, keep, &players, None);
    let theta = model::train(&m, h);
    let preds = (0..m.ndec()).map(|k| model::predict(&m, &theta, k)).collect();
    (preds, theta, m)
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Rate {
    pub n: usize,
    pub top1: usize,
    pub top3: usize,
    pub ll: f64,
    pub ll_n: usize,
    pub bot: usize,
    pub ai: usize,
    pub base_n: usize,
    pub train: usize,
}

impl Rate {
    fn add(&mut self, p: &Pred, b: Option<&Baseline>, train: Option<&Pred>) {
        self.n += 1;
        self.top1 += p.top1 as usize;
        self.top3 += p.top3 as usize;
        if let Some(l) = p.logp {
            self.ll += l;
            self.ll_n += 1;
        }
        if let Some(b) = b {
            self.base_n += 1;
            self.bot += b.bot as usize;
            self.ai += b.ai as usize;
        }
        if let Some(t) = train {
            self.train += t.top1 as usize;
        }
    }
    pub fn pct(&self, x: usize) -> f64 {
        100.0 * x as f64 / self.n.max(1) as f64
    }
    pub fn base_pct(&self, x: usize) -> f64 {
        100.0 * x as f64 / self.base_n.max(1) as f64
    }
    pub fn mean_ll(&self) -> f64 {
        self.ll / self.ll_n.max(1) as f64
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    /// Mean of the per-player held-out top-1 (each player counts the same)
    pub player_mean: f64,
    /// Held-out top-1 over all decisions
    pub decision_mean: f64,
    pub top3: f64,
    pub mean_ll: f64,
    pub players: BTreeMap<String, Rate>,
    pub groups: BTreeMap<String, Rate>,
    pub days: BTreeMap<String, Rate>,
    pub player_groups: BTreeMap<String, BTreeMap<String, Rate>>,
    pub games: BTreeMap<String, usize>,
}

pub fn summary(decs: &[Decision], preds: &[Pred], base: &HashMap<(i64, usize), Baseline>, train: Option<&[Pred]>) -> Summary {
    let mut s = Summary::default();
    let mut all = Rate::default();
    let mut games: BTreeMap<String, Vec<i64>> = Default::default();
    for (i, (d, p)) in decs.iter().zip(preds).enumerate() {
        let b = base.get(&(d.table, d.step));
        let t = train.map(|t| &t[i]);
        all.add(p, b, t);
        s.players.entry(d.player.clone()).or_default().add(p, b, t);
        s.groups.entry(d.group.clone()).or_default().add(p, b, t);
        s.days.entry(if d.day >= 5 { "day5+".to_string() } else { format!("day{}", d.day) }).or_default().add(p, b, t);
        s.player_groups.entry(d.player.clone()).or_default().entry(d.group.clone()).or_default().add(p, b, t);
        games.entry(d.player.clone()).or_default().push(d.table);
    }
    for (p, mut g) in games {
        g.sort();
        g.dedup();
        s.games.insert(p, g.len());
    }
    s.decision_mean = all.pct(all.top1);
    s.top3 = all.pct(all.top3);
    s.mean_ll = all.mean_ll();
    s.player_mean = s.players.values().map(|r| r.pct(r.top1)).sum::<f64>() / s.players.len().max(1) as f64;
    s.groups.insert("ALL".into(), all);
    s
}
