//! A player's AI: the shared weights plus that player's offsets, on that player's deck. It plugs
//! into `play::Agent`, so it plays full games: the best-scoring candidate, or a draw from the
//! softmax with a temperature.

use crate::features::{self, Config};
use altered_ai::legal;
use altered_ai::play::{Agent, Observation};
use altered_ai::search::{Ai, Params};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

/// The shared weights (`shared.json`), in raw feature units
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shared {
    pub version: String,
    pub finish_steps: usize,
    /// The deck AI's pick is a feature: the agent runs the deck AI (without priors) too
    #[serde(default)]
    pub search_pick: bool,
    #[serde(default = "one")]
    pub copies: usize,
    pub weights: BTreeMap<String, f64>,
}

/// One player's AI (`players/<player>.json`)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerFile {
    /// The `version` of the shared weights these offsets go with
    pub shared: String,
    pub player: String,
    pub deck: String,
    /// The deck's hero, and "player — hero" as the AI's name
    #[serde(default)]
    pub hero: String,
    #[serde(default)]
    pub label: String,
    pub offsets: BTreeMap<String, f64>,
}

fn one() -> usize {
    1
}

pub struct Imitator {
    pub label: String,
    pub weights: HashMap<String, f64>,
    pub cfg: Config,
    /// Some(t): draw from the softmax at temperature t instead of the best candidate
    pub temperature: Option<f64>,
    /// The deck AI whose pick is a feature
    pub search: Option<Ai>,
    recent: Vec<String>,
}

impl Imitator {
    /// `deck`: the deck AI's parameters (`ai/params/DXX.json`), used when the shared model reads
    /// the deck AI's pick
    pub fn new(label: &str, shared: &Shared, player: Option<&PlayerFile>, deck: Option<Params>) -> Imitator {
        let mut weights: HashMap<String, f64> = shared.weights.iter().map(|(k, v)| (k.clone(), *v)).collect();
        if let Some(p) = player {
            for (k, v) in &p.offsets {
                *weights.entry(k.clone()).or_default() += v;
            }
        }
        let cfg = Config { finish_steps: shared.finish_steps, search_pick: shared.search_pick, copies: shared.copies, ..Config::default() };
        let search = shared.search_pick.then(|| Ai::new("deck", features::without_priors(deck.unwrap_or_default())));
        Imitator { label: label.into(), weights, cfg, temperature: None, search, recent: vec![] }
    }

    pub fn score(&self, f: &features::Feats) -> f64 {
        f.iter().map(|(n, v)| self.weights.get(n).copied().unwrap_or(0.0) * *v as f64).sum()
    }
}

impl Agent for Imitator {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let mut cands = legal::candidates(d);
        if cands.is_empty() {
            return json!({"type": "pass"});
        }
        if cands.len() == 1 {
            return cands.remove(0);
        }
        // The same effect-order question a third time among the last 8 (both players handing it
        // back to each other): the first answer, as `search::Ai` does
        let sig = format!("{}|{}|{}", d["kind"], d["prompt"], d["choices"]);
        let repeats = self.recent.iter().filter(|s| **s == sig).count();
        self.recent.push(sig);
        if self.recent.len() > 8 {
            self.recent.remove(0);
        }
        if repeats >= 2 && d["kind"] == "choice" {
            return cands.remove(0);
        }
        let pick = match self.search.as_mut() {
            Some(ai) => Some(features::search_pick(ai, obs, d, &cands, rng)),
            None => None,
        };
        let feats = features::extract(obs, d, &cands, rng, &self.cfg, pick.as_ref());
        let scores: Vec<f64> = feats.iter().map(|f| self.score(f) - if f.iter().any(|(n, _)| n == "refused") { 1e6 } else { 0.0 }).collect();
        let best = match self.temperature {
            Some(t) if t > 0.0 => {
                let mx = scores.iter().cloned().fold(f64::MIN, f64::max);
                let w: Vec<f64> = scores.iter().map(|s| ((s - mx) / t).exp()).collect();
                let mut r = rng.random_range(0.0..w.iter().sum::<f64>());
                let mut k = 0;
                while k + 1 < w.len() && r >= w[k] {
                    r -= w[k];
                    k += 1;
                }
                k
            }
            _ => (0..scores.len()).fold(0, |b, k| if scores[k] > scores[b] { k } else { b }),
        };
        cands.swap_remove(best)
    }
}
