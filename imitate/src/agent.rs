//! A player's AI: the shared weights plus that player's offsets, on that player's deck. It plugs
//! into `play::Agent`, so it plays full games: the best-scoring candidate, or a draw from the
//! softmax with a temperature.

use crate::features::{self, Config};
use altered_ai::legal;
use altered_ai::play::{Agent, Observation};
use altered_ai::search::{Ai, Params};
use rand::{Rng, SeedableRng};
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
    /// More ensemble members: each its own redeal of the hidden cards (`seed`) and its own
    /// weights; the members' scores are averaged
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Member {
    pub seed: u64,
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
    /// The offsets of the other ensemble members, in the order of `Shared::more`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_offsets: Vec<BTreeMap<String, f64>>,
}

fn one() -> usize {
    1
}

/// One ensemble member at play time: its redeal seed, its weights (shared + the player's
/// offsets) and its own deck AI (whose pick is a feature)
pub struct Scorer {
    pub seed: u64,
    pub weights: HashMap<String, f64>,
    pub search: Option<Ai>,
}

pub struct Imitator {
    pub label: String,
    pub members: Vec<Scorer>,
    pub cfg: Config,
    /// Some(t): draw from the softmax at temperature t instead of the best candidate
    pub temperature: Option<f64>,
    recent: Vec<String>,
}

impl Imitator {
    /// `deck`: the deck AI's parameters (`ai/params/DXX.json`), used when the shared model reads
    /// the deck AI's pick
    pub fn new(label: &str, shared: &Shared, player: Option<&PlayerFile>, deck: Option<Params>) -> Imitator {
        let cfg = Config { finish_steps: shared.finish_steps, search_pick: shared.search_pick, copies: shared.copies, ..Config::default() };
        let combine = |w: &BTreeMap<String, f64>, o: Option<&BTreeMap<String, f64>>| -> HashMap<String, f64> {
            let mut weights: HashMap<String, f64> = w.iter().map(|(k, v)| (k.clone(), *v)).collect();
            for (k, v) in o.into_iter().flatten() {
                *weights.entry(k.clone()).or_default() += v;
            }
            weights
        };
        let search = || shared.search_pick.then(|| Ai::new("deck", features::without_priors(deck.clone().unwrap_or_default())));
        let mut members = vec![Scorer { seed: 0, weights: combine(&shared.weights, player.map(|p| &p.offsets)), search: search() }];
        for (k, m) in shared.more.iter().enumerate() {
            members.push(Scorer { seed: m.seed, weights: combine(&m.weights, player.and_then(|p| p.more_offsets.get(k))), search: search() });
        }
        Imitator { label: label.into(), members, cfg, temperature: None, recent: vec![] }
    }

    /// The average score of each candidate over the members; member m redeals the hidden cards
    /// with the random stream `base ^ seed_m` (as the extraction of its dataset did)
    pub fn scores(&mut self, obs: &Observation, d: &Value, cands: &[Value], base: u64) -> Vec<f64> {
        let mut total = vec![0.0; cands.len()];
        let n = self.members.len() as f64;
        for m in &mut self.members {
            let mut rng = ChaCha8Rng::seed_from_u64(base ^ m.seed);
            let pick = m.search.as_mut().map(|ai| features::search_pick(ai, obs, d, cands, &mut rng));
            let feats = features::extract(obs, d, cands, &mut rng, &self.cfg, pick.as_ref());
            for (t, f) in total.iter_mut().zip(&feats) {
                let refused = if f.iter().any(|(n, _)| n == "refused") { 1e6 } else { 0.0 };
                *t += (f.iter().map(|(name, v)| m.weights.get(name).copied().unwrap_or(0.0) * *v as f64).sum::<f64>() - refused) / n;
            }
        }
        total
    }

    /// The answer, the hidden cards redealt from `base` (see `scores`)
    pub fn decide_seeded(&mut self, obs: &Observation, d: &Value, base: u64, rng: &mut ChaCha8Rng) -> Value {
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
        let scores = self.scores(obs, d, &cands, base);
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

impl Agent for Imitator {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let base: u64 = rng.random();
        self.decide_seeded(obs, d, base, rng)
    }
}
