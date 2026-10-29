//! The work of the server for games against the bot, on one thread: N games on starter decks
//! (player 2001 human, 2002 bot), each human decision answered as the load test does (answer,
//! bots, view of the player), all games kept in memory.
//!
//!   cargo run --release -p altered-rules --example serve_bench [-- GAMES ANSWERS]
//!
//! Prints the time per answer (with the bot's moves and the view), the time to create a game,
//! the memory per game (RSS) and the size of a saved game, and a checksum of the final views.

use altered_rules::arena::NewPlayer;
use altered_rules::managers::cards::STARTER_DECKS;
use altered_rules::Game;
use serde_json::{json, Value};
use std::time::Instant;

/// Live heap bytes
struct Counting;
static LIVE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        LIVE.fetch_sub(l.size() as isize, std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n as isize - l.size() as isize, std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.realloc(p, l, n)
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

fn live_kb() -> i64 {
    (LIVE.load(std::sync::atomic::Ordering::Relaxed) / 1024) as i64
}

fn rss_kb() -> u64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(0)
}

struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        if n == 0 {
            0
        } else {
            ((self.0 >> 33) % n as u64) as usize
        }
    }
}

/// loadtest's pick_answer
fn pick(d: &Value, rng: &mut Lcg) -> Value {
    let arr = |k: &str| d[k].as_array().cloned().unwrap_or_default();
    let mut a = match d["kind"].as_str().unwrap_or("") {
        "turn" => {
            let play = arr("play");
            if !play.is_empty() && rng.below(10) < 8 {
                let p = &play[rng.below(play.len())];
                let dests = p["destinations"].as_array().cloned().unwrap_or_default();
                json!({"type": "play", "cardId": p["cardId"], "destination": dests[rng.below(dests.len())]})
            } else if d["canEndTurn"].as_bool().unwrap_or(false) {
                json!({"type": "endTurn"})
            } else {
                json!({"type": "pass"})
            }
        }
        "select" => {
            let mut ids = arr("cardIds");
            let max = d["max"].as_u64().unwrap_or(1) as usize;
            for i in (1..ids.len()).rev() {
                ids.swap(i, rng.below(i + 1));
            }
            ids.truncate(max);
            json!({"type": "select", "cardIds": ids})
        }
        "choice" => {
            let en: Vec<Value> = arr("choices").into_iter().filter(|c| c["enabled"].as_bool().unwrap_or(false)).collect();
            if en.is_empty() {
                json!({"type": "pass"})
            } else {
                json!({"type": "choice", "id": en[rng.below(en.len())]["id"]})
            }
        }
        "expedition" => {
            let e = arr("expeditions");
            let n = d["min"].as_u64().unwrap_or(1).max(1) as usize;
            json!({"type": "expedition", "keys": e.iter().take(n).map(|x| x["key"].clone()).collect::<Vec<_>>()})
        }
        "player" if !arr("players").is_empty() => json!({"type": "player", "player": arr("players")[0]}),
        "number" => json!({"type": "number", "value": arr("values").last().cloned().unwrap_or(json!(0))}),
        "location" if !arr("locations").is_empty() => json!({"type": "location", "key": arr("locations")[0]["key"]}),
        "exchange" if !arr("hand").is_empty() && !arr("reserve").is_empty() => json!({"type": "exchange", "hand": arr("hand")[0], "reserve": arr("reserve")[0]}),
        "cleanup" => {
            let r = d["reserve"]["cardIds"].as_array().cloned().unwrap_or_default();
            let l = d["landmarks"]["cardIds"].as_array().cloned().unwrap_or_default();
            let nr = d["reserve"]["n"].as_u64().unwrap_or(0) as usize;
            let nl = d["landmarks"]["n"].as_u64().unwrap_or(0) as usize;
            json!({"type": "cleanup", "reserve": r[..nr.min(r.len())], "landmarks": l[..nl.min(l.len())]})
        }
        "confirm" => json!({"type": "confirm"}),
        _ => json!({"type": "pass"}),
    };
    a["decisionId"] = d["id"].clone();
    a
}

fn main() {
    let games: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(100);
    let answers: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(40);
    std::thread::Builder::new().stack_size(std::env::var("STACK_KB").ok().and_then(|s| s.parse::<usize>().ok()).unwrap_or(256 << 10) << 10).spawn(move || run(games, answers)).unwrap().join().unwrap();
}

fn run(games: usize, answers: usize) {
    let n = STARTER_DECKS.len();
    let rss0 = rss_kb();
    let live0 = live_kb();
    let mut all = vec![];
    let (mut t_create, mut t_answer, mut n_answers, mut refused) = (0.0, 0.0, 0usize, 0usize);
    let mut check = 0u64;
    for i in 0..games {
        let players = vec![
            NewPlayer { name: "Human".into(), bot: false, deck: Some(STARTER_DECKS[(i * 7) % n].0.to_string()), api_deck: None },
            NewPlayer { name: "Bot".into(), bot: true, deck: None, api_deck: None },
        ];
        let dice: Vec<i64> = (0..100).map(|k| (k * 7 + i as i64) % 6 + 1).collect();
        let t = Instant::now();
        let mut g = Game::create(1000 + i as i64, &players, &dice).unwrap();
        g.compact();
        t_create += t.elapsed().as_secs_f64();
        let mut rng = Lcg(i as u64);
        let mut view = g.view(Some(2001), "bench").unwrap();
        for _ in 0..answers {
            if g.is_over() || view["decision"].is_null() {
                break;
            }
            let a = pick(&view["decision"], &mut rng);
            let t = Instant::now();
            let ok = g.answer(2001, &a).is_ok();
            if ok {
                g.run_bots().unwrap();
                g.compact();
            }
            view = g.view(Some(2001), "bench").unwrap();
            t_answer += t.elapsed().as_secs_f64();
            n_answers += 1;
            if !ok {
                refused += 1;
                if view["decision"]["canPass"].as_bool().unwrap_or(false) {
                    let _ = g.answer(2001, &json!({"type": "pass", "decisionId": view["decision"]["id"]}));
                    g.run_bots().unwrap();
                    g.compact();
                    view = g.view(Some(2001), "bench").unwrap();
                }
            }
        }
        for b in serde_json::to_string(&view).unwrap().bytes() {
            check = check.wrapping_mul(31).wrapping_add(b as u64);
        }
        all.push(g);
    }
    let rss = rss_kb();
    let live = live_kb() - live0;
    if std::env::var("BREAKDOWN").is_ok() {
        // Live heap of each part of the games (parts shared with others count where dropped last)
        let mut parts: Vec<(&str, i64)> = vec![];
        macro_rules! part {
            ($name:expr, $field:ident) => {{
                let before = live_kb();
                for g in all.iter_mut() {
                    drop(std::mem::take(&mut g.$field));
                }
                parts.push(($name, before - live_kb()));
            }};
        }
        part!("log", log);
        part!("tree", tree);
        part!("history", history);
        part!("notifications", notifications);
        part!("globals", globals);
        part!("deleted_cards", deleted_cards);
        part!("cards", cards);
        part!("meeples", meeples);
        part!("last_args", last_args);
        part!("dice", dice);
        for (k, v) in parts {
            println!("  {:<14} {:>6} KB/game", k, v / games as i64);
        }
        println!("  rest           {:>6} KB/game", (live_kb() - live0) / games as i64);
        return;
    }
    let saved: usize = all.iter().map(|g| serde_json::to_vec(g).unwrap().len()).sum();
    println!(
        "{} games, {} answers ({} refused): answer+bots+view {:.3} ms, create {:.2} ms, memory {:.0} KB/game (live heap {:.0} KB/game), saved {:.0} KB/game, checksum {:x}",
        games,
        n_answers,
        refused,
        t_answer * 1000.0 / n_answers.max(1) as f64,
        t_create * 1000.0 / games as f64,
        (rss.saturating_sub(rss0)) as f64 / games as f64,
        live as f64 / games as f64,
        saved as f64 / 1024.0 / games as f64,
        check
    );
}
