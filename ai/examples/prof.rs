//! Where the AI's time goes: cargo run --release -p altered-ai --example prof -- D01 D02 SEED
use altered_ai::legal;
use altered_ai::play::new_game;
use altered_ai::search::{bot_step, rollout_day};
use std::time::Instant;
fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let a: Vec<String> = std::env::args().collect();
    let decks: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let pair = [decks[&a[1]].clone(), decks[&a[2]].clone()];
    let mut g = new_game(&pair, a[3].parse().unwrap()).unwrap();
    // Play 40 bot steps to reach mid-game
    for _ in 0..40 {
        let p = g.waiting_for()[0];
        bot_step(&mut g, p as i64);
    }
    let p = g.waiting_for()[0];
    let d = g.decision(p).unwrap();
    let c = legal::candidates(&d);
    let n = 200;
    let t = Instant::now();
    for _ in 0..n { std::hint::black_box(g.clone()); }
    println!("clone {:.1} us", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    let t = Instant::now();
    for _ in 0..n { std::hint::black_box(g.decision(p)); }
    println!("decision {:.2} ms", t.elapsed().as_secs_f64() * 1e3 / n as f64);
    let t = Instant::now();
    for _ in 0..n { let mut h = g.clone(); std::hint::black_box(h.view(Some(p), "")); }
    println!("view {:.2} ms", t.elapsed().as_secs_f64() * 1e3 / n as f64);
    let t = Instant::now();
    for _ in 0..n { let mut h = g.clone(); std::hint::black_box(h.answer(p, &legal::with_id(&d, &c[0]))).ok(); }
    println!("answer {:.2} ms ({})", t.elapsed().as_secs_f64() * 1e3 / n as f64, c[0]);
    let t = Instant::now();
    for _ in 0..n { let mut h = g.clone(); std::hint::black_box(bot_step(&mut h, p as i64)); }
    println!("bot step {:.2} ms", t.elapsed().as_secs_f64() * 1e3 / n as f64);
    let t = Instant::now();
    let mut steps = 0;
    for _ in 0..20 { let mut h = g.clone(); steps += rollout_day(&mut h, 80); }
    println!("rollout {:.2} ms ({} steps each)", t.elapsed().as_secs_f64() * 1e3 / 20.0, steps / 20);
}
