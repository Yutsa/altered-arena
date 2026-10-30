//! The AI: each candidate answer is played on copies of the game redealt from the player's point
//! of view, the rest of the player's action is finished by the fast policy (Bot.php), and the
//! position is evaluated (`eval`). Candidates are averaged over the copies; the best one is
//! played.

use crate::eval::{self, Weights};
use crate::legal;
use crate::play::{Agent, Observation};
use altered_rules::game::PId;
use altered_rules::{bot, framework, v1, Game};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Per-deck parameters (`ai/params/DXX.json`)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Params {
    pub weights: Weights,
    /// Copies of the game per decision (determinisations)
    pub copies: usize,
    /// Most candidates simulated (the others are cut by the priors)
    pub max_candidates: usize,
    /// Most fast-policy steps to finish an action
    pub finish_steps: usize,
    /// Bonus (in evaluation points) for ending the turn, against playing everything at once
    pub end_turn_bias: f64,
    /// Card priors: class name -> bonus when the card is played (plays) / put in Mana (mana)
    pub play_prior: std::collections::BTreeMap<String, f64>,
    pub mana_prior: std::collections::BTreeMap<String, f64>,
    /// Morning Mana: put a card in Mana while below this many Mana Orbs
    pub mana_target: f64,
    /// Mana choice without a prior: this times the card's hand cost (expensive cards first)
    pub mana_cost: f64,
    /// Turn decisions: the best candidates of the 1-ply evaluation (and the end of the turn)
    /// are played on to the end of the day by the fast policy (0: no rollout)
    pub rollouts: usize,
    pub rollout_steps: usize,
    /// Night cleanup: weight of the discarded cards' keep value (0: none)
    pub cleanup_keep: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            weights: Weights::default(),
            copies: 1,
            max_candidates: 16,
            finish_steps: 12,
            end_turn_bias: 0.0,
            play_prior: Default::default(),
            mana_prior: Default::default(),
            mana_target: 10.0,
            mana_cost: 0.1,
            rollouts: 3,
            rollout_steps: 80,
            cleanup_keep: 0.0,
        }
    }
}

pub struct Ai {
    pub label: String,
    pub params: Params,
    /// Engine answers spent in simulations
    pub sim_steps: usize,
}

impl Ai {
    pub fn new(label: &str, params: Params) -> Ai {
        Ai { label: label.into(), params, sim_steps: 0 }
    }
}

/// One decision of the fast policy (Bot.php, then its fallback and the arena's last resort, as
/// `arena::run_bots`). False: no answer was accepted.
pub fn bot_step(g: &mut Game, p: PId) -> bool {
    if g.begin_request(None).is_err() {
        return false;
    }
    let first = bot::decide(g, p).ok().flatten();
    let start = g.clone();
    let try_call = |g: &mut Game, c: &(String, Vec<Value>)| -> bool {
        g.seed_rng();
        let ok = g.begin_request(Some(p)).and_then(|_| v1::call_action(g, &c.0, &c.1)).is_ok();
        if !ok {
            *g = start.clone();
        }
        ok
    };
    if let Some(c) = &first {
        if try_call(g, c) {
            return true;
        }
    }
    let mut more = vec![];
    let mut probe = start.clone();
    if let Ok(Some(f)) = bot::fallback(&mut probe, p) {
        more.push(f);
    }
    more.extend(bot::last_resort(&mut probe, p).unwrap_or_default());
    more.iter().any(|c| try_call(g, c))
}

/// Plays the fast policy for `me` until the opponent must decide, `me` starts a new turn action
/// (Afternoon `chooseAssignment`), the game ends, or `max` steps. Returns the steps played.
pub fn finish_action(g: &mut Game, me: PId, max: usize) -> usize {
    let mut n = 0;
    while n < max && !g.is_over() {
        let w = g.waiting_for();
        if w.first().map(|p| *p as PId) != Some(me) {
            break;
        }
        if framework::state(g).map(|s| s.name) == Some("chooseAssignment") && n > 0 {
            break;
        }
        if !bot_step(g, me) {
            break;
        }
        n += 1;
    }
    n
}

/// Plays the fast policy for both players to the end of the current day (Night), the end of the
/// game, or `max` steps. Returns the steps played.
pub fn rollout_day(g: &mut Game, max: usize) -> usize {
    let day = altered_rules::core::globals::get_int(g, "day").unwrap_or(0);
    let mut n = 0;
    while n < max && !g.is_over() {
        let phase = altered_rules::core::globals::get_int(g, "phase").unwrap_or(0);
        let d = altered_rules::core::globals::get_int(g, "day").unwrap_or(0);
        if d != day || phase >= 4 {
            break;
        }
        let Some(&p) = g.waiting_for().first() else { break };
        if !bot_step(g, p as PId) {
            break;
        }
        n += 1;
    }
    n
}

fn is_mana_choice(d: &Value) -> bool {
    d["kind"] == "select" && d["prompt"].as_str().unwrap_or("").to_lowercase().contains("mana")
}

/// The key of a card in the priors: its engine class, or `Unique:<name>` for a Unique (whose
/// class is the generic `Card`)
pub fn card_key(g: &Game, id: &Value) -> Option<String> {
    let id = id.as_i64()?;
    let r = v1::card_ref(g, id).ok()?;
    if r == "Card" {
        let name = g.cards.get(&id)?.props.get("name")?.as_str()?.to_string();
        return Some(format!("Unique:{}", name));
    }
    Some(r)
}

fn card_ref(g: &Game, id: &Value) -> Option<String> {
    card_key(g, id)
}

impl Ai {
    /// Cheap score of a candidate before simulation (priors of the deck)
    fn prior(&self, g: &Game, d: &Value, a: &Value) -> f64 {
        let p = &self.params;
        match (d["kind"].as_str(), a["type"].as_str()) {
            (Some("turn"), Some("play")) => card_ref(g, &a["cardId"]).and_then(|r| p.play_prior.get(&r).copied()).unwrap_or(0.0),
            (Some("turn"), Some("endTurn")) => p.end_turn_bias,
            (Some("select"), Some("select")) if d["prompt"].as_str().unwrap_or("").to_lowercase().contains("mana") => {
                a["cardIds"].as_array().into_iter().flatten().map(|id| card_ref(g, id).and_then(|r| p.mana_prior.get(&r).copied()).unwrap_or(0.0)).sum()
            }
            (Some("cleanup"), Some("cleanup")) if p.cleanup_keep != 0.0 => {
                let ids = a["reserve"].as_array().into_iter().flatten().chain(a["landmarks"].as_array().into_iter().flatten());
                -p.cleanup_keep * ids.map(|id| self.keep_value(g, id)).sum::<f64>()
            }
            _ => 0.0,
        }
    }

    /// What a card in Reserve or Landmarks is worth keeping: its play prior, statistics and cost
    fn keep_value(&self, g: &Game, id: &Value) -> f64 {
        let Some(c) = id.as_i64().and_then(|i| g.cards.get(&i)) else { return 0.0 };
        let num = |k: &str| c.props.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let prior = card_ref(g, id).and_then(|r| self.params.play_prior.get(&r).copied()).unwrap_or(0.0);
        prior + 0.2 * (num("forest") + num("mountain") + num("ocean")) + 0.15 * num("costHand")
    }

    pub fn choose(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let mut cands = legal::candidates(d);
        if cands.is_empty() {
            return json!({"type": "pass"});
        }
        if cands.len() == 1 {
            return cands.remove(0);
        }
        let me = obs.me;
        if is_mana_choice(d) {
            return self.choose_mana(obs, d);
        }
        let copies: Vec<Game> = (0..self.params.copies.max(1)).map(|_| obs.determinize(rng)).collect();
        // Priors: order the candidates, keep the best ones
        let mut scored: Vec<(f64, Value)> = cands.into_iter().map(|a| (self.prior(&copies[0], d, &a), a)).collect();
        if scored.len() > self.params.max_candidates {
            scored.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(self.params.max_candidates);
        }
        let trace = std::env::var("AI_TRACE").is_ok();
        if trace {
            eprintln!("--- {} {} day/phase {:?}: {}", me, d["kind"], obs.view().get("phase"), d["prompt"]);
        }
        let mut evaluated: Vec<(f64, Value)> = vec![];
        for (prior, a) in scored {
            let mut total = 0.0;
            let mut ok = 0;
            for base in &copies {
                let mut s = base.clone();
                if s.answer(me as u32, &legal::with_id(d, &a)).is_err() {
                    continue;
                }
                self.sim_steps += 1 + finish_action(&mut s, me, self.params.finish_steps);
                let view = s.view(Some(me as u32), "").unwrap_or(Value::Null);
                total += eval::evaluate(&view, &self.params.weights);
                ok += 1;
            }
            if ok == 0 {
                continue;
            }
            let score = total / ok as f64 + prior;
            if trace {
                let what = a.get("cardId").and_then(|c| card_ref(&copies[0], c)).unwrap_or_default();
                eprintln!("   {:8.2} {} {}", score, a, what);
            }
            evaluated.push((score, a));
        }
        if evaluated.is_empty() {
            return legal::candidates(d).remove(0);
        }
        // Turn decisions: the best candidates and the end of the turn, played on to the end of the day
        if d["kind"] == "turn" && self.params.rollouts > 0 && evaluated.len() > 1 {
            let mut order: Vec<usize> = (0..evaluated.len()).collect();
            order.sort_by(|&x, &y| evaluated[y].0.partial_cmp(&evaluated[x].0).unwrap_or(std::cmp::Ordering::Equal));
            let mut chosen: Vec<usize> = order.iter().copied().take(self.params.rollouts).collect();
            if let Some(e) = evaluated.iter().position(|(_, a)| a["type"] == "endTurn" || a["type"] == "pass") {
                if !chosen.contains(&e) {
                    chosen.push(e);
                }
            }
            let mut best: Option<(f64, usize)> = None;
            for &k in &chosen {
                let a = &evaluated[k].1;
                let mut total = 0.0;
                let mut ok = 0;
                for base in &copies {
                    let mut s = base.clone();
                    if s.answer(me as u32, &legal::with_id(d, a)).is_err() {
                        continue;
                    }
                    self.sim_steps += 1 + rollout_day(&mut s, self.params.rollout_steps);
                    let view = s.view(Some(me as u32), "").unwrap_or(Value::Null);
                    total += eval::evaluate(&view, &self.params.weights);
                    ok += 1;
                }
                if ok == 0 {
                    continue;
                }
                // The 1-ply score breaks ties (and counts a little)
                let score = total / ok as f64 + 0.1 * evaluated[k].0;
                if trace {
                    eprintln!("   rollout {:8.2} {}", score, a);
                }
                if best.is_none_or(|(b, _)| score > b) {
                    best = Some((score, k));
                }
            }
            if let Some((_, k)) = best {
                return evaluated.swap_remove(k).1;
            }
        }
        let mut best = 0;
        for (k, (sc, _)) in evaluated.iter().enumerate() {
            if *sc > evaluated[best].0 {
                best = k;
            }
        }
        evaluated.swap_remove(best).1
    }

    /// Mana: below the target, the cards with the best Mana prior (else the most expensive ones)
    fn choose_mana(&mut self, obs: &Observation, d: &Value) -> Value {
        let view = obs.view();
        let me = obs.me;
        let mana = view["players"].as_array().into_iter().flatten().find(|p| p["id"].as_i64() == Some(me)).map(|p| p["manaTotal"].as_f64().unwrap_or(0.0)).unwrap_or(0.0);
        let ids: Vec<Value> = d["cardIds"].as_array().cloned().unwrap_or_default();
        let k = (d["max"].as_u64().unwrap_or(1) as usize).min(ids.len());
        if d["canPass"] == true && (mana >= self.params.mana_target || k == 0) {
            return json!({"type": "pass"});
        }
        let card = |id: &Value| view["cards"].as_array().and_then(|cs| cs.iter().find(|c| c["id"] == *id)).cloned().unwrap_or(Value::Null);
        let mut scored: Vec<(f64, Value)> = ids
            .iter()
            .map(|id| {
                let c = card(id);
                let r = obs.card_key(id).unwrap_or_default();
                let s = self.params.mana_prior.get(&r).copied().unwrap_or(0.0) + self.params.mana_cost * c["costHand"].as_f64().unwrap_or(0.0);
                (s, id.clone())
            })
            .collect();
        scored.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        let min = (d["min"].as_u64().unwrap_or(0) as usize).min(k);
        let n = k.max(min);
        json!({"type": "select", "cardIds": scored.into_iter().take(n).map(|(_, id)| id).collect::<Vec<_>>()})
    }
}

impl Agent for Ai {
    fn name(&self) -> String {
        self.label.clone()
    }
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        self.choose(obs, d, rng)
    }
}
