//! What a search-based AI would pay with the current state: a whole bot game, copying a mid-game
//! state, and playing a copy to the end (a playout).
//!
//!   cargo run --release -p goldens --example bench_search -- [GAMES]

use altered_rules::arena::{self, NewPlayer};
use altered_rules::{bot, framework, v1, Game};
use std::time::Instant;

/// One bot decision, as arena::run_bots takes it (the rollback copy included). False: nothing to do.
fn step(g: &mut Game) -> bool {
    g.begin_request(None).unwrap();
    let Some(&b) = g.bots.clone().iter().find(|&&b| framework::is_player_active(g, b)) else { return false };
    let Some((action, args)) = bot::decide(g, b).unwrap() else { return false };
    let backup = g.clone();
    g.seed_rng();
    let ok = g.begin_request(Some(b)).and_then(|_| v1::call_action(g, &action, &args));
    match ok {
        Ok(()) => arena::store_notifications(g),
        Err(_) => {
            *g = backup;
            // Let the arena's fallbacks deal with a refused choice
            g.run_bots().unwrap();
        }
    }
    true
}

fn play_out(g: &mut Game) -> usize {
    let mut n = 0;
    while !g.is_over() && n < 2000 && step(g) {
        n += 1;
    }
    n
}

fn main() {
    let n: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let players = [
        NewPlayer { name: "A".into(), bot: false, deck: Some("ax_1_sierra".into()), api_deck: None, reunion_deck: None },
        NewPlayer { name: "B".into(), bot: true, deck: None, api_deck: None, reunion_deck: None },
    ];
    let (mut game_s, mut decisions, mut clone_s, mut clones, mut playout_s, mut playouts, mut bytes) = (0.0, 0, 0.0, 0, 0.0, 0, 0);
    for seed in 1..=n {
        // Game::create would run two bots to the end: player 2001 is human at creation, then a bot
        // stepped one decision at a time
        let mut h = Game::create(seed, &players, &[]).unwrap();
        h.bots = vec![2001, 2002];
        let t = Instant::now();
        let mut k = 0;
        let mut mid = None;
        while !h.is_over() && k < 2000 && step(&mut h) {
            k += 1;
            if k == 60 {
                mid = Some(h.clone());
            }
        }
        game_s += t.elapsed().as_secs_f64();
        decisions += k;
        if let Some(m) = mid {
            bytes += serde_json::to_vec(&m).unwrap().len();
            let t = Instant::now();
            for _ in 0..50 {
                std::hint::black_box(m.clone());
            }
            clone_s += t.elapsed().as_secs_f64();
            clones += 50;
            for _ in 0..3 {
                let mut c = m.clone();
                let t = Instant::now();
                play_out(&mut c);
                playout_s += t.elapsed().as_secs_f64();
                playouts += 1;
            }
        }
    }
    println!(
        "games: {} ({} decisions each), {:.1} ms per game, {:.2} ms per decision",
        n,
        decisions / n as usize,
        game_s * 1e3 / n as f64,
        game_s * 1e3 / decisions as f64
    );
    println!("mid-game state: {} KB serialised, copy {:.1} µs", bytes / 1024 / n as usize, clone_s * 1e6 / clones as f64);
    println!("playout from decision 60: {:.1} ms", playout_s * 1e3 / playouts as f64);
}
