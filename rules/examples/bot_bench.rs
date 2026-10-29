//! Bot-vs-bot benchmark: whole games on starter decks, one thread.
//!
//!   cargo run --release -p altered-rules --example bot_bench [-- GAMES]
//!
//! Prints the time per game (creation, then the bots play to the end), the size of a saved game
//! (JSON) at the end, and a checksum of the final states (the same before and after a change of
//! performance).

use altered_rules::arena::NewPlayer;
use altered_rules::managers::cards::STARTER_DECKS;
use altered_rules::Game;
use std::time::Instant;

fn main() {
    let games: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(40);
    std::thread::Builder::new().stack_size(std::env::var("STACK_KB").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(256 << 10) << 10).spawn(move || run(games)).unwrap().join().unwrap();
}

fn run(games: usize) {
    let n = STARTER_DECKS.len();
    let mut create = 0.0;
    let mut play = 0.0;
    let mut saved = 0usize;
    let mut check = 0u64;
    for i in 0..games {
        let decks = [STARTER_DECKS[(i * 7) % n].0, STARTER_DECKS[(i * 11 + 3) % n].0];
        let players: Vec<NewPlayer> = decks.iter().map(|d| NewPlayer { name: "Bot".into(), bot: true, deck: Some(d.to_string()), api_deck: None }).collect();
        let dice: Vec<i64> = (0..100).map(|k| (k * 7 + i as i64) % 6 + 1).collect();
        let t = Instant::now();
        let mut g = Game::create(1000 + i as i64, &players, &dice).unwrap();
        create += t.elapsed().as_secs_f64();
        let t = Instant::now();
        for _ in 0..50 {
            if g.is_over() {
                break;
            }
            g.run_bots().unwrap();
        }
        play += t.elapsed().as_secs_f64();
        let json = serde_json::to_vec(&g).unwrap();
        saved += json.len();
        if let Ok(dir) = std::env::var("BOT_BENCH_DUMP") {
            std::fs::write(format!("{}/game{}.json", dir, i), &json).unwrap();
        }
        let state = serde_json::to_string(&g.oracle_state()).unwrap();
        for b in state.bytes() {
            check = check.wrapping_mul(31).wrapping_add(b as u64);
        }
    }
    println!(
        "{} games: create {:.2} ms/game, play {:.2} ms/game, saved {:.0} KB/game, checksum {:x}",
        games,
        create * 1000.0 / games as f64,
        play * 1000.0 / games as f64,
        saved as f64 / 1024.0 / games as f64,
        check
    );
}
