//! The decisions of the real players, from native replays: for every step where the acting
//! player had two or more candidate answers, the candidates' features (`features::extract`, from
//! what that player saw) and which candidates are equivalent to the recorded answer.
//!
//! Equivalent: the same game afterwards (`Game::oracle_state` and who must play), or the same
//! answer once card ids are replaced by what the cards are (two copies of a card in the same
//! place count as one choice).

use crate::features::{self, Config, Feats};
use altered_ai::play::{self, Agent, Observation};
use altered_ai::search::{self, Ai, Params};
use altered_ai::legal;
use altered_replay::native::{self, NativeReplay, NativeStep};
use altered_rules::Game;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// One decision of the dataset
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    pub table: i64,
    pub step: usize,
    pub seat: u32,
    pub player: String,
    pub deck: String,
    pub group: String,
    pub prompt: String,
    pub day: i64,
    /// Candidates equivalent to the recorded answer (all false: the answer is not a candidate)
    pub labels: Vec<bool>,
    /// Features of each candidate: (feature index, value)
    pub feats: Vec<Vec<(u32, f32)>>,
}

/// What the simple bot and the deck AI answer at a decision (baselines)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub table: i64,
    pub step: usize,
    pub bot: bool,
    pub ai: bool,
}

#[derive(Default)]
pub struct Interner {
    pub names: Vec<String>,
    index: HashMap<String, u32>,
}

impl Interner {
    pub fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.names.len() as u32;
        self.names.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }

    pub fn from_names(names: Vec<String>) -> Interner {
        let index = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
        Interner { names, index }
    }
}

pub fn read_replay(path: &str) -> Result<NativeReplay, String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut de = serde_json::Deserializer::from_slice(&b);
    de.disable_recursion_limit();
    NativeReplay::deserialize(&mut de).map_err(|e| e.to_string())
}

/// The game ready for a step's answer (what `native::apply` sets before playing it)
pub fn prepare(g: &mut Game, s: &NativeStep) {
    g.fw_set(3, s.m);
    for &(p, pref, v) in &s.f {
        altered_replay::setup::set_pref(g, p, pref, v);
    }
    if let Some(id) = s.i["cards"].as_i64() {
        g.next_card_id = id;
    }
    if let Some(id) = s.i["meeples"].as_i64() {
        g.next_meeple_id = id;
    }
    for (id, state) in &s.o {
        if let Some(c) = g.cards.get_mut(id) {
            c.state = *state;
        }
    }
}

fn state_sig(g: &Game) -> String {
    format!("{:?}{}", g.waiting_for(), g.oracle_state())
}

/// The state after an answer (None: refused)
fn after(g: &Game, p: u32, a: &Value) -> Option<String> {
    let mut c = g.clone();
    altered_replay::play(&mut c, p, a).ok()?;
    Some(state_sig(&c))
}

/// Which candidates are equivalent to `real`
pub fn labels(g: &Game, p: u32, cands: &[Value], real: &Value) -> Vec<bool> {
    let real_sig = after(g, p, real);
    let real_abs = features::abstract_answer(g, real);
    cands
        .iter()
        .map(|c| {
            if features::abstract_answer(g, c) == real_abs {
                return true;
            }
            real_sig.is_some() && after(g, p, c) == real_sig
        })
        .collect()
}

/// Whether answer `a` is equivalent to `real`
fn same(g: &Game, p: u32, a: &Value, real: &Value) -> bool {
    features::abstract_answer(g, a) == features::abstract_answer(g, real) || {
        let r = after(g, p, real);
        r.is_some() && after(g, p, a) == r
    }
}

pub struct Tables {
    /// table -> seat ("2001") -> deck id
    pub seats: Value,
}

impl Tables {
    pub fn deck_of(&self, table: i64, seat: u32) -> String {
        self.seats[table.to_string()][seat.to_string()].as_str().unwrap_or("").to_string()
    }
}

/// Counts of the steps skipped while building the dataset
#[derive(Default, Debug, Clone, Serialize)]
pub struct Skipped {
    pub act: usize,
    pub no_decision: usize,
    pub single: usize,
    pub replay_error: usize,
}

/// The decisions of one native replay. With `baseline`, the simple bot's and the deck AI's
/// answers instead of the features (`params_dir`: the deck AIs' parameters).
pub fn decisions(path: &str, tables: &Tables, cfg: &Config, interner: &Mutex<Interner>, baseline: Option<&str>) -> (Vec<Decision>, Vec<Baseline>, Skipped) {
    let mut skipped = Skipped::default();
    let n = match read_replay(path) {
        Ok(n) => n,
        Err(_) => {
            skipped.replay_error += 1;
            return (vec![], vec![], skipped);
        }
    };
    let decks: [Value; 2] = [n.decks[0].clone(), n.decks[1].clone()];
    let mut g = match native::start(&n) {
        Ok(g) => g,
        Err(_) => {
            skipped.replay_error += 1;
            return (vec![], vec![], skipped);
        }
    };
    let mut ais: Vec<Ai> = (0..2)
        .map(|seat| {
            let deck = tables.deck_of(n.table, play::SEATS[seat] as u32);
            let params = std::fs::read_to_string(format!("{}/{}.json", baseline.unwrap_or("ai/params"), deck))
                .ok()
                .and_then(|s| serde_json::from_str::<Params>(&s).ok())
                .unwrap_or_default();
            // The baseline is the deck AI as it is; as a feature, without its priors
            Ai::new("deck", if baseline.is_some() { params } else { features::without_priors(params) })
        })
        .collect();
    let mut out = vec![];
    let mut base = vec![];
    for (k, s) in n.steps.iter().enumerate() {
        if s.a["type"] == "act" {
            skipped.act += 1;
        } else {
            let mut ready = g.clone();
            prepare(&mut ready, s);
            match ready.try_decision(s.p) {
                Ok(Some(d)) => {
                    let cands = legal::candidates(&d);
                    if cands.len() < 2 {
                        skipped.single += 1;
                    } else if let Some(_dir) = baseline {
                        let seat = (s.p - 2001) as usize;
                        let mut b = ready.clone();
                        let real = after(&ready, s.p, &s.a);
                        let bot = search::bot_step(&mut b, s.p as i64) && Some(state_sig(&b)) == real;
                        let mut rng = ChaCha8Rng::seed_from_u64(n.table as u64 * 1009 + k as u64);
                        let obs = Observation::new(&ready, s.p as i64, &decks);
                        let a = ais[seat].decide(&obs, &d, &mut rng);
                        let ai = same(&ready, s.p, &a, &s.a);
                        base.push(Baseline { table: n.table, step: k, bot, ai });
                    } else {
                        let lab = labels(&ready, s.p, &cands, &s.a);
                        let mut rng = ChaCha8Rng::seed_from_u64(n.table as u64 * 7919 + k as u64);
                        let obs = Observation::new(&ready, s.p as i64, &decks);
                        let seat = (s.p - 2001) as usize;
                        let pick = if cfg.search_pick { Some(features::search_pick(&mut ais[seat], &obs, &d, &cands, &mut rng)) } else { None };
                        let feats: Vec<Feats> = features::extract(&obs, &d, &cands, &mut rng, cfg, pick.as_ref());
                        let mut it = interner.lock().unwrap();
                        let feats = feats.into_iter().map(|f| f.into_iter().map(|(name, v)| (it.id(&name), v)).collect()).collect();
                        drop(it);
                        let seat = (s.p - 2001) as usize;
                        out.push(Decision {
                            table: n.table,
                            step: k,
                            seat: s.p,
                            player: n.names.get(seat).cloned().unwrap_or_default(),
                            deck: tables.deck_of(n.table, s.p),
                            group: features::group(&d),
                            prompt: d["prompt"].as_str().unwrap_or("").to_string(),
                            day: ready.oracle_state()["day"].as_i64().unwrap_or(0),
                            labels: lab,
                            feats,
                        });
                    }
                }
                _ => skipped.no_decision += 1,
            }
        }
        if native::apply(&mut g, s).is_err() {
            skipped.replay_error += 1;
            break;
        }
    }
    (out, base, skipped)
}

/// The dataset on disk: `names.json` (feature names) and `dataset.jsonl` (one decision a line)
pub fn write(dir: &str, names: &[String], decs: &[Decision]) {
    use std::io::Write;
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(format!("{}/names.json", dir), serde_json::to_string(names).unwrap()).unwrap();
    let mut f = std::io::BufWriter::new(std::fs::File::create(format!("{}/dataset.jsonl", dir)).unwrap());
    for d in decs {
        writeln!(f, "{}", serde_json::to_string(d).unwrap()).unwrap();
    }
}

pub fn read(dir: &str) -> (Vec<String>, Vec<Decision>) {
    let names: Vec<String> = serde_json::from_str(&std::fs::read_to_string(format!("{}/names.json", dir)).expect("names.json")).unwrap();
    let s = std::fs::read_to_string(format!("{}/dataset.jsonl", dir)).expect("dataset.jsonl");
    let decs = s.lines().filter(|l| !l.is_empty()).map(|l| serde_json::from_str(l).unwrap()).collect();
    (names, decs)
}

pub fn read_baselines(dir: &str) -> HashMap<(i64, usize), Baseline> {
    let Ok(s) = std::fs::read_to_string(format!("{}/baselines.jsonl", dir)) else { return HashMap::new() };
    s.lines().filter_map(|l| serde_json::from_str::<Baseline>(l).ok()).map(|b| ((b.table, b.step), b)).collect()
}
