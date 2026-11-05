//! Haven, Bravos Bastion taps by an imitation AI, as `ai/examples/haven_check.rs` counts them for
//! the deck AIs: taps followed by a Character played from Reserve (used) or by anything else
//! (wasted), and Characters played from Reserve while a Haven was ready and not tapped (missed).
//!   cargo run --release -p altered-imitate --example haven_check -- PLAYER_FILE_STEM OPP_DECK N
use altered_ai::play::{play_game, run_parallel, Agent, Observation};
use altered_ai::search::{Ai, Params};
use altered_imitate::agent::{Imitator, PlayerFile, Shared};
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const OUT: &str = "private/tournament-617607/imitation";
const HAVEN: &str = "AX_Rare_HavenBravosBastion";

struct Counted {
    agent: Box<dyn Agent>,
    pending: bool,
    counts: Arc<[AtomicUsize; 3]>,
}

impl Agent for Counted {
    fn name(&self) -> String {
        self.agent.name()
    }
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let a = self.agent.decide(obs, d, rng);
        if d["kind"] == "turn" {
            let view = obs.view();
            let card = |id: &Value| view["cards"].as_array().and_then(|cs| cs.iter().find(|c| c["id"] == *id)).cloned().unwrap_or(Value::Null);
            let c = card(&a["cardId"]);
            let haven = a["type"] == "tap" && c["ref"] == HAVEN;
            let reserve_char =
                a["type"] == "play" && c["zone"] == "reserve" && a["cardId"].as_i64().and_then(|id| obs.card_prop(id, "type")) == Some(json!("character"));
            let haven_ready = d["tap"].as_array().into_iter().flatten().any(|id| card(id)["ref"] == HAVEN);
            if reserve_char && !self.pending && haven_ready {
                self.counts[2].fetch_add(1, Ordering::SeqCst);
            }
            if self.pending && !haven {
                self.counts[if reserve_char { 0 } else { 1 }].fetch_add(1, Ordering::SeqCst);
                self.pending = false;
            }
            if haven {
                self.pending = true;
            }
        }
        a
    }
}

fn main() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let a: Vec<String> = std::env::args().collect();
    let n: u64 = a[3].parse().unwrap();
    let decks: Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let shared: Shared = serde_json::from_str(&std::fs::read_to_string(format!("{}/shared.json", OUT)).unwrap()).unwrap();
    let player: PlayerFile = serde_json::from_str(&std::fs::read_to_string(format!("{}/players/{}.json", OUT, a[1])).unwrap()).unwrap();
    let params = |d: &str| -> Option<Params> { serde_json::from_str(&std::fs::read_to_string(format!("ai/params/{}.json", d)).ok()?).ok() };
    let counts: Arc<[AtomicUsize; 3]> = Arc::new(Default::default());
    let seeds: Vec<u64> = (1..=n).collect();
    run_parallel(
        &seeds,
        8,
        |&seed| {
            let pair = [decks[&player.deck].clone(), decks[&a[2]].clone()];
            let me =
                Counted { agent: Box::new(Imitator::new(&player.label, &shared, Some(&player), params(&player.deck))), pending: false, counts: counts.clone() };
            let mut agents: [Box<dyn Agent>; 2] = [Box::new(me), Box::new(Ai::new(&a[2], params(&a[2]).unwrap_or_default()))];
            if let Some(e) = play_game(&pair, &mut agents, seed).error {
                eprintln!("seed {}: {}", seed, e);
            }
        },
        |&seed, e| eprintln!("seed {} panicked: {}", seed, e),
    );
    let c: Vec<usize> = counts.iter().map(|x| x.load(Ordering::SeqCst)).collect();
    println!("{} vs {}: Haven taps used {}, wasted {}, Reserve Characters played without the ready Haven {}", player.label, a[2], c[0], c[1], c[2]);
}
