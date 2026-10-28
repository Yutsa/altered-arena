//! Bot against bot on every pair of starter decks: games must end, and every effect met is listed
//! when it is not implemented. Usage: cargo run --release --example selfplay [games]
use altered_engine::catalog::catalog;
use altered_engine::model::Phase;
use altered_engine::{Game, PlayerConfig};
use std::collections::BTreeMap;
use std::time::Instant;

fn main() {
    let games: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(300);
    let decks: Vec<String> = catalog().starters.iter().map(|(d, _)| d.clone()).collect();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let (mut ended, mut days, mut decisions, mut stuck, mut wins) = (0, 0, 0, 0, [0usize; 2]);
    let t = Instant::now();
    for i in 0..games {
        let d0 = decks[i % decks.len()].clone();
        let d1 = decks[(i * 7 + 3) % decks.len()].clone();
        let mut g = Game::new(
            format!("{}", i),
            [
                PlayerConfig { id: 1, name: "A".into(), bot: true, deck: Some(d0.clone()) },
                PlayerConfig { id: 2, name: "B".into(), bot: true, deck: Some(d1.clone()) },
            ],
            i as u64,
        );
        decisions += g.run_bots();
        if g.phase == Phase::Ended && g.winner.is_some() {
            ended += 1;
            days += g.day as usize;
            wins[g.winner.unwrap()] += 1;
        } else {
            stuck += 1;
            if stuck <= 5 {
                let p = g.pending.first().map(|p| p.prompt.clone()).unwrap_or_default();
                eprintln!("game {} ({} vs {}) not ended: day {} phase {:?}, pending: {} | last log: {:?}", i, d0, d1, g.day, g.phase, p, g.log.last().map(|l| &l.text));
            }
        }
        for u in &g.unsupported {
            *unsupported.entry(u.clone()).or_default() += 1;
        }
    }
    let el = t.elapsed();
    println!("{} games in {:.2?} ({:.2} ms/game, {} decisions)", games, el, el.as_secs_f64() * 1000.0 / games as f64, decisions);
    println!("ended {} (avg {:.1} days), not ended {}, wins first seat {} / second {}", ended, days as f64 / ended.max(1) as f64, stuck, wins[0], wins[1]);
    let mut u: Vec<_> = unsupported.into_iter().collect();
    u.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("unsupported effects met ({}):", u.len());
    for (k, n) in u.iter().take(60) {
        println!("  {:4} {}", n, k);
    }
}
