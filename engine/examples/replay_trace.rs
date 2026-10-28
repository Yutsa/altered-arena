//! Replays a trace saved by tools/diff_engines.py (traces/game-N.json) in the Rust engine and
//! prints, for each answer, the engine log and the decisions that follow.
//! Usage: cargo run --release --example replay_trace traces/game-3.json [from_step]
use altered_engine::{Answer, Game};
use serde_json::Value;

fn main() {
    let path = std::env::args().nth(1).expect("trace file");
    let from: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let trace: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    println!("problem: {}\n", trace["problem"].as_str().unwrap_or(""));
    let mut g = Game::from_snapshot("trace".into(), &trace["snapshot"]).expect("snapshot");
    let mut seen = 0;
    let show = |g: &Game, seen: &mut usize, verbose: bool| {
        for l in &g.log[(*seen).min(g.log.len())..] {
            if verbose {
                println!("    | {}", l.text);
            }
        }
        *seen = g.log.len();
        if verbose {
            for p in &g.pending {
                println!("    ? {} asks: {} [{}]", g.players[p.seat].id, p.prompt, altered_engine::answer::kind_name(&p.kind));
            }
        }
    };
    show(&g, &mut seen, from == 0);
    for (i, a) in trace["answers"].as_array().unwrap().iter().enumerate() {
        let player = a["player"].as_u64().unwrap() as u32;
        let seat = g.seat_of(player).unwrap();
        let verbose = i + 1 >= from;
        if verbose {
            println!("[{}] {} -> {}   (php: {})", i + 1, player, a["answer"], a["php"].as_str().unwrap_or(""));
        }
        let answer: Answer = serde_json::from_value(a["answer"].clone()).unwrap();
        if let Err(e) = g.answer(seat, answer) {
            println!("    !! refused: {}", e);
        }
        show(&g, &mut seen, verbose);
    }
}
