//! Haven, Bravos Bastion taps by the AI: how many are followed by a Character played from
//! Reserve (used) and how many by anything else (wasted). Also the Characters played from
//! Reserve while a Haven was ready and not tapped (missed).
//!   cargo run --release -p altered-ai --example haven_check -- D04 D01 N
use altered_ai::play::{play_game, Agent, Observation};
use altered_ai::search::{Ai, Params};
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct Counted {
    ai: Ai,
    pending: bool,
    used: Arc<AtomicUsize>,
    wasted: Arc<AtomicUsize>,
    missed: Arc<AtomicUsize>,
}

impl Agent for Counted {
    fn name(&self) -> String {
        self.ai.name()
    }
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let a = self.ai.decide(obs, d, rng);
        if d["kind"] == "turn" {
            let view = obs.view();
            let card = view["cards"].as_array().and_then(|cs| cs.iter().find(|c| c["id"] == a["cardId"])).cloned().unwrap_or(Value::Null);
            let haven = a["type"] == "tap" && card["ref"] == "AX_Rare_HavenBravosBastion";
            let reserve_char = a["type"] == "play"
                && card["zone"] == "reserve"
                && a["cardId"].as_i64().and_then(|id| obs.card_prop(id, "type")) == Some(serde_json::json!("character"));
            let haven_ready = d["tap"].as_array().into_iter().flatten().any(|id| {
                view["cards"].as_array().and_then(|cs| cs.iter().find(|c| c["id"] == *id)).map(|c| c["ref"] == "AX_Rare_HavenBravosBastion").unwrap_or(false)
            });
            if reserve_char && !self.pending && haven_ready {
                self.missed.fetch_add(1, Ordering::SeqCst);
                if std::env::var("HAVEN_LOG").is_ok() {
                    eprintln!("missed: {} played from Reserve with Haven ready", card["ref"]);
                }
            }
            if self.pending && !haven {
                if reserve_char { &self.used } else { &self.wasted }.fetch_add(1, Ordering::SeqCst);
                if std::env::var("HAVEN_LOG").is_ok() && !reserve_char {
                    eprintln!("wasted: then {} ({})", a, card["ref"]);
                }
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
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let a: Vec<String> = std::env::args().collect();
    let decks: Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let n: u64 = a[3].parse().unwrap();
    let params =
        |d: &str| std::fs::read_to_string(format!("ai/params/{}.json", d)).ok().map(|s| serde_json::from_str::<Params>(&s).unwrap()).unwrap_or_default();
    let used = Arc::new(AtomicUsize::new(0));
    let wasted = Arc::new(AtomicUsize::new(0));
    let missed = Arc::new(AtomicUsize::new(0));
    let seeds: Vec<u64> = (1..=n).collect();
    altered_ai::play::run_parallel(
        &seeds,
        8,
        |&seed| {
            let pair = [decks[&a[1]].clone(), decks[&a[2]].clone()];
            let counted = |d: &str| Counted { ai: Ai::new(d, params(d)), pending: false, used: used.clone(), wasted: wasted.clone(), missed: missed.clone() };
            let mut agents: [Box<dyn Agent>; 2] = [Box::new(counted(&a[1])), Box::new(counted(&a[2]))];
            let r = play_game(&pair, &mut agents, seed);
            if let Some(e) = r.error {
                eprintln!("seed {}: {}", seed, e);
            }
        },
        |&seed, e| eprintln!("seed {} panicked: {}", seed, e),
    );
    println!(
        "{} vs {}: Haven taps used {}, wasted {}, Reserve Characters played without the ready Haven {}",
        a[1],
        a[2],
        used.load(Ordering::SeqCst),
        wasted.load(Ordering::SeqCst),
        missed.load(Ordering::SeqCst)
    );
}
