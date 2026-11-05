//! Cards an AI plays that do nothing (`altered_ai::useful`), against a deck AI: count, plays in
//! all, and the cards played that way. `off`: with the filter turned off (the AIs before it).
//!   cargo run --release -p altered-imitate --example useless_check -- AI OPP_DECK N [off]
//! AI: `deck:DXX` or `imitation:STEM` (private/tournament-617607/imitation/players/STEM.json)
use altered_ai::play::{play_game, run_parallel, Agent, Observation};
use altered_ai::search::{Ai, Params};
use altered_ai::useful;
use altered_imitate::agent::{Imitator, PlayerFile, Shared};
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const OUT: &str = "private/tournament-617607/imitation";

#[derive(Default)]
struct Counts {
    plays: usize,
    useless: usize,
    cards: BTreeMap<String, usize>,
}

struct Counted {
    agent: Box<dyn Agent>,
    counts: Arc<Mutex<Counts>>,
}

impl Agent for Counted {
    fn name(&self) -> String {
        self.agent.name()
    }
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let a = self.agent.decide(obs, d, rng);
        if std::env::var("USELESS_ALL").is_ok() && d["kind"] == "turn" {
            for c in altered_ai::legal::candidates(d) {
                if useful::is_useless_play(obs, d, &c) {
                    let name = c["cardId"].as_i64().and_then(|id| obs.card_prop(id, "name")).unwrap_or_default();
                    let zone =
                        obs.view()["cards"].as_array().and_then(|cs| cs.iter().find(|x| x["id"] == c["cardId"]).map(|x| x["zone"].clone())).unwrap_or_default();
                    eprintln!("candidate that does nothing: {} from {}", name, zone);
                }
            }
        }
        if d["kind"] == "turn" && a["type"] == "play" {
            let useless = useful::is_useless_play(obs, d, &a);
            let mut c = self.counts.lock().unwrap();
            c.plays += 1;
            if useless {
                c.useless += 1;
                let name = a["cardId"].as_i64().and_then(|id| obs.card_prop(id, "name")).and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
                *c.cards.entry(name).or_default() += 1;
            }
        }
        a
    }
}

fn main() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).unwrap().join().unwrap();
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> T {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path, e))).unwrap()
}

fn run() {
    let a: Vec<String> = std::env::args().collect();
    let n: u64 = a[3].parse().unwrap();
    if a.get(4).map(String::as_str) == Some("off") {
        useful::DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let decks: Value = read("server/goldens/tournament_decks.json");
    let shared: Shared = read(&format!("{}/shared.json", OUT));
    let params = |d: &str| -> Params { read(&format!("ai/params/{}.json", d)) };
    let counts = Arc::new(Mutex::new(Counts::default()));
    let seeds: Vec<u64> = (1..=n).collect();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
    run_parallel(
        &seeds,
        threads,
        |&seed| {
            let (deck, agent): (String, Box<dyn Agent>) = match a[1].strip_prefix("imitation:") {
                Some(stem) => {
                    let p: PlayerFile = read(&format!("{}/players/{}.json", OUT, stem));
                    (p.deck.clone(), Box::new(Imitator::new(&p.label, &shared, Some(&p), Some(params(&p.deck)))))
                }
                None => {
                    let d = a[1].strip_prefix("deck:").unwrap().to_string();
                    let ai = Ai::new(&a[1], params(&d));
                    (d, Box::new(ai))
                }
            };
            let me = Counted { agent, counts: counts.clone() };
            let opp = Box::new(Ai::new(&a[2], params(&a[2])));
            // Seats alternate
            let (pair, mut agents): ([Value; 2], [Box<dyn Agent>; 2]) = if seed % 2 == 1 {
                ([decks[&deck].clone(), decks[&a[2]].clone()], [Box::new(me), opp])
            } else {
                ([decks[&a[2]].clone(), decks[&deck].clone()], [opp, Box::new(me)])
            };
            if let Some(e) = play_game(&pair, &mut agents, seed).error {
                eprintln!("seed {}: {}", seed, e);
            }
        },
        |&seed, e| eprintln!("seed {} panicked: {}", seed, e),
    );
    let c = counts.lock().unwrap();
    let mut cards: Vec<_> = c.cards.iter().collect();
    cards.sort_by_key(|(_, k)| std::cmp::Reverse(**k));
    let list: Vec<String> = cards.iter().map(|(n, k)| format!("{} {}", n, k)).collect();
    println!(
        "{} vs {} ({} games{}): {} plays that do nothing of {} plays; {}",
        a[1],
        a[2],
        n,
        if useful::DISABLED.load(std::sync::atomic::Ordering::Relaxed) { ", filter off" } else { "" },
        c.useless,
        c.plays,
        list.join(", ")
    );
}
