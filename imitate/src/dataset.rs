//! The decisions of the real players, from native replays: for every step where the acting
//! player had two or more candidate answers, the candidates' features (`features::extract`, from
//! what that player saw) and which candidates are equivalent to the recorded answer.
//!
//! Equivalent (`labels`): the same answer once card ids are replaced by what the cards are (two
//! copies of a card in the same place count as one choice); the same game afterwards
//! (`Game::oracle_state` and who must play), compared after the other players' following answers
//! when the answer does not show yet (both players choose their opening Mana at once); or the
//! same answers in another order: the player gives this answer a few steps later, and played
//! first it leads to the same game once the same recorded answers are used.

use crate::features::{self, Config, Feats};
use altered_ai::legal;
use altered_ai::play::{self, Agent, Observation};
use altered_ai::search::{self, Ai, Params};
use altered_replay::native::{self, NativeReplay, NativeStep};
use altered_rules::Game;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

/// The recorded game of a native replay: the game before each step (prepared for its answer),
/// the step's decision, the state after it. Shorter than the replay when a step fails.
pub struct Trace {
    pub n: NativeReplay,
    pub before: Vec<Game>,
    pub decs: Vec<Value>,
    pub after: Vec<String>,
}

pub fn trace(n: NativeReplay) -> Result<Trace, String> {
    let mut g = native::start(&n).map_err(|e| format!("{:?}", e))?;
    let (mut before, mut decs, mut after) = (vec![], vec![], vec![]);
    for s in &n.steps {
        let mut ready = g.clone();
        prepare(&mut ready, s);
        if native::apply(&mut g, s).is_err() {
            break;
        }
        decs.push(ready.try_decision(s.p).ok().flatten().unwrap_or(Value::Null));
        before.push(ready);
        after.push(state_sig(&g));
    }
    Ok(Trace { n, before, decs, after })
}

/// How many answers to look ahead for the same answer given later
const WINDOW: usize = 40;

fn choice_label(d: &Value, a: &Value) -> Option<String> {
    if a["type"] != "choice" {
        return None;
    }
    let c = d["choices"].as_array()?.iter().find(|c| c["id"] == a["id"])?;
    Some(format!("{}|{}", c["label"], c["sourceCardId"]))
}

impl Trace {
    /// Plays the recorded answer of step i on `g` (prepared as step i; a choice by its label, its
    /// id may have changed)
    fn feed(&self, g: &Game, i: usize) -> Option<Game> {
        let r = &self.n.steps[i];
        let mut y = g.clone();
        prepare(&mut y, r);
        let now = y.try_decision(r.p).ok().flatten()?;
        let mut a = r.a.clone();
        if let Some(l) = choice_label(&self.decs[i], &r.a) {
            if let Some(c) = now["choices"]
                .as_array()
                .and_then(|cs| cs.iter().find(|c| choice_label(&now, &json!({"type": "choice", "id": c["id"]})).as_deref() == Some(l.as_str())))
            {
                a = json!({"type": "choice", "id": c["id"]});
            }
        }
        altered_replay::play(&mut y, r.p, &a).ok()?;
        Some(y)
    }

    /// The step after k where the same player gives answer `a` (of step k) again
    pub fn given_later(&self, k: usize, a: &Value) -> Option<usize> {
        let s = &self.n.steps[k];
        let abs = features::abstract_answer(&self.before[k], a);
        let label = choice_label(&self.decs[k], a);
        (k + 1..(k + WINDOW).min(self.after.len())).find(|&j| {
            let r = &self.n.steps[j];
            r.p == s.p
                && (legal::canonical(&r.a) == legal::canonical(a)
                    || match &label {
                        Some(l) => choice_label(&self.decs[j], &r.a).as_ref() == Some(l),
                        None => features::abstract_answer(&self.before[j], &r.a) == abs,
                    })
        })
    }

    /// Answer `a` at step k is the recorded answer of a later step j, and played first it leads
    /// to the recorded game: after it, at each decision, the first unused recorded answer of the
    /// window asked with the same prompt that the game accepts (then any that it accepts); same
    /// when, once every step k..m is used (m >= j), the game equals the recorded game after m
    pub fn reordered(&self, k: usize, a: &Value) -> bool {
        let Some(j) = self.given_later(k, a) else { return false };
        let end = (k + WINDOW).min(self.after.len());
        let run = || -> Option<bool> {
            let mut x = self.before[k].clone();
            altered_replay::play(&mut x, self.n.steps[k].p, a).ok()?;
            let mut used = vec![false; end - k];
            used[j - k] = true;
            loop {
                let m = k + used.iter().take_while(|u| **u).count();
                if m > j && state_sig(&x) == self.after[m - 1] {
                    return Some(true);
                }
                let mut next = None;
                'pass: for same_prompt in [true, false] {
                    for i in (k..end).filter(|i| !used[i - k]) {
                        if same_prompt
                            && x.clone().try_decision(self.n.steps[i].p).ok().flatten().map(|d| d["prompt"].clone()) != Some(self.decs[i]["prompt"].clone())
                        {
                            continue;
                        }
                        if let Some(y) = self.feed(&x, i) {
                            next = Some((i, y));
                            break 'pass;
                        }
                    }
                }
                let (i, y) = next?;
                used[i - k] = true;
                x = y;
            }
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).ok().flatten().unwrap_or(false)
    }

    /// Which candidates of step k are equivalent to the recorded answer
    pub fn labels(&self, k: usize, cands: &[Value]) -> Vec<bool> {
        let s = &self.n.steps[k];
        let g = &self.before[k];
        let real_abs = features::abstract_answer(g, &s.a);
        let games: Vec<Option<Game>> = cands
            .iter()
            .map(|c| {
                let mut x = g.clone();
                altered_replay::play(&mut x, s.p, c).ok().map(|_| x)
            })
            .collect();
        // Every answer leads to the recorded state: the answer does not show yet; compare after
        // the other players' following answers
        let hidden = games.iter().all(|x| x.as_ref().is_some_and(|x| state_sig(x) == self.after[k]));
        let same_state = |x: &Game| -> bool {
            if !hidden {
                return state_sig(x) == self.after[k];
            }
            let mut x = x.clone();
            let mut i = k + 1;
            while i < self.after.len() && i <= k + 6 && self.n.steps[i].p != s.p {
                match self.feed(&x, i) {
                    Some(y) => x = y,
                    None => return false,
                }
                i += 1;
            }
            state_sig(&x) == self.after[i - 1]
        };
        cands
            .iter()
            .zip(&games)
            .map(|(c, x)| features::abstract_answer(g, c) == real_abs || x.as_ref().is_some_and(same_state) || (x.is_some() && self.reordered(k, c)))
            .collect()
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
    /// Answers the player took back right after (`actCancel...`); the new answer is the decision
    pub cancelled: usize,
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
    let steps = n.steps.len();
    let tr = match trace(n) {
        Ok(t) => t,
        Err(_) => {
            skipped.replay_error += 1;
            return (vec![], vec![], skipped);
        }
    };
    if tr.after.len() < steps {
        skipped.replay_error += 1;
    }
    let n = &tr.n;
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
    for k in 0..tr.after.len() {
        let s = &n.steps[k];
        if s.a["type"] == "act" {
            skipped.act += 1;
            continue;
        }
        let ready = &tr.before[k];
        let d = &tr.decs[k];
        if d.is_null() {
            skipped.no_decision += 1;
            continue;
        }
        let cands = legal::candidates(d);
        let undone =
            n.steps.get(k + 1).is_some_and(|r| r.p == s.p && r.a["type"] == "act" && r.a["action"].as_str().is_some_and(|a| a.starts_with("actCancel")));
        if undone {
            skipped.cancelled += 1;
        } else if cands.len() < 2 {
            skipped.single += 1;
        } else if baseline.is_some() {
            let seat = (s.p - 2001) as usize;
            let mut b = ready.clone();
            let bot = search::bot_step(&mut b, s.p as i64) && state_sig(&b) == tr.after[k];
            let mut rng = ChaCha8Rng::seed_from_u64(n.table as u64 * 1009 + k as u64);
            let obs = Observation::new(ready, s.p as i64, &decks);
            let a = ais[seat].decide(&obs, d, &mut rng);
            // The deck AI's answer among the candidates, so that it is labelled as they are
            let mut with = cands.clone();
            with.push(a);
            let ai = *tr.labels(k, &with).last().unwrap();
            base.push(Baseline { table: n.table, step: k, bot, ai });
        } else {
            let lab = tr.labels(k, &cands);
            let mut rng = ChaCha8Rng::seed_from_u64((n.table as u64 * 7919 + k as u64) ^ cfg.seed);
            let obs = Observation::new(ready, s.p as i64, &decks);
            let seat = (s.p - 2001) as usize;
            let pick = if cfg.search_pick { Some(features::search_pick(&mut ais[seat], &obs, d, &cands, &mut rng)) } else { None };
            let feats: Vec<Feats> = features::extract(&obs, d, &cands, &mut rng, cfg, pick.as_ref());
            let mut it = interner.lock().unwrap();
            let feats = feats.into_iter().map(|f| f.into_iter().map(|(name, v)| (it.id(&name), v)).collect()).collect();
            drop(it);
            out.push(Decision {
                table: n.table,
                step: k,
                seat: s.p,
                player: n.names.get(seat).cloned().unwrap_or_default(),
                deck: tables.deck_of(n.table, s.p),
                group: features::group(d),
                prompt: d["prompt"].as_str().unwrap_or("").to_string(),
                day: ready.oracle_state()["day"].as_i64().unwrap_or(0),
                labels: lab,
                feats,
            });
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
