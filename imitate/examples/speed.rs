//! Time per decision of the imitation AI and its parts, against the deck AI: speed [GAMES]
use altered_ai::play::{self, Agent, Observation};
use altered_ai::search::{Ai, Params};
use altered_imitate::agent::{Imitator, PlayerFile, Shared};
use altered_imitate::features;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const OUT: &str = "private/tournament-617607/imitation";

/// An agent that adds the time of its decisions to a counter (seconds, decisions)
struct Timed(Box<dyn Agent>, Arc<Mutex<(f64, usize)>>);
impl Agent for Timed {
    fn name(&self) -> String {
        self.0.name()
    }
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let t = Instant::now();
        let a = self.0.decide(obs, d, rng);
        let mut c = self.1.lock().unwrap();
        c.0 += t.elapsed().as_secs_f64();
        c.1 += 1;
        a
    }
}

fn main() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let games: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let decks: Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let shared: Shared = serde_json::from_str(&std::fs::read_to_string(format!("{}/shared.json", OUT)).unwrap()).unwrap();
    let mut files: Vec<PlayerFile> = std::fs::read_dir(format!("{}/players", OUT))
        .unwrap()
        .flatten()
        .filter_map(|f| serde_json::from_str(&std::fs::read_to_string(f.path()).ok()?).ok())
        .collect();
    files.sort_by(|a, b| a.deck.cmp(&b.deck));
    let params = |d: &str| -> Params { serde_json::from_str(&std::fs::read_to_string(format!("ai/params/{}.json", d)).unwrap()).unwrap() };
    let ids: Vec<String> = decks.as_object().unwrap().keys().cloned().collect();
    // Seat 0: imitation AI, deck AI, deck AI without priors, imitation with one member
    for kind in ["deck", "deck-nopriors", "imitation-1", "imitation-2"] {
        let me = Arc::new(Mutex::new((0.0, 0)));
        let opp = Arc::new(Mutex::new((0.0, 0)));
        let t = Instant::now();
        for g in 0..games {
            let pf = &files[(g as usize * 5) % files.len()];
            let o = &ids[(g as usize * 11 + 3) % ids.len()];
            let a: Box<dyn Agent> = match kind {
                "deck" => Box::new(Ai::new("deck", params(&pf.deck))),
                "deck-nopriors" => Box::new(Ai::new("deck", features::without_priors(params(&pf.deck)))),
                _ => {
                    let mut s = shared.clone();
                    if kind == "imitation-1" {
                        s.more.clear();
                    }
                    Box::new(Imitator::new("imit", &s, Some(pf), Some(params(&pf.deck))))
                }
            };
            let b: Box<dyn Agent> = Box::new(Ai::new("opp", params(o)));
            let mut agents: [Box<dyn Agent>; 2] = [Box::new(Timed(a, me.clone())), Box::new(Timed(b, opp.clone()))];
            let r = play::play_game(&[decks[&pf.deck].clone(), decks[o].clone()], &mut agents, 300_000 + g);
            if let Some(e) = r.error {
                eprintln!("error {}", e);
            }
        }
        let (m, o) = (*me.lock().unwrap(), *opp.lock().unwrap());
        println!(
            "{:14} {} games {:.1}s: seat {:.2}s/game {:.0}ms/decision ({} decisions) | opponent deck AI {:.2}s/game {:.0}ms/decision",
            kind,
            games,
            t.elapsed().as_secs_f64(),
            m.0 / games as f64,
            1000.0 * m.0 / m.1.max(1) as f64,
            m.1,
            o.0 / games as f64,
            1000.0 * o.0 / o.1.max(1) as f64
        );
    }
}
