//! Micro-benchmark of the costs of a saved game: load, clone, save, fresh state, views.
//!
//!   cargo run --release -p altered-rules --example micro -- GAME.json

use altered_rules::Game;
use std::time::Instant;

fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let path = std::env::args().nth(1).expect("a saved game (JSON of altered_rules::Game)");
    let bytes = std::fs::read(&path).unwrap();
    let t = Instant::now();
    let g: Game = serde_json::from_slice(&bytes).unwrap();
    println!("deserialize {:?}", t.elapsed());
    let t = Instant::now();
    for _ in 0..1000 {
        std::hint::black_box(g.clone());
    }
    println!("clone {:?}", t.elapsed() / 1000);
    let t = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(serde_json::to_vec(&g).unwrap());
    }
    println!("serialize {:?}", t.elapsed() / 100);
    let mut h = g.clone();
    let t = Instant::now();
    for _ in 0..100 {
        h.fresh_state(None).unwrap();
    }
    println!("fresh_state {:?}", t.elapsed() / 100);
    let t = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(h.view(Some(2001), "x").unwrap());
    }
    println!("view {:?}", t.elapsed() / 100);
    let t = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(h.view(None, "x").unwrap());
    }
    println!("spectator view {:?}", t.elapsed() / 100);
    let t = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(h.try_decision(2001).ok());
    }
    println!("decision {:?}", t.elapsed() / 100);
}
