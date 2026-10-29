//! Command line of the AI tournament.
//!
//!   aitool match DECK_A DECK_B N [AGENT_A AGENT_B] [--threads T]   N games (seats alternated)
//!   aitool pass N AGENT [--games G] [--threads T] [--params DIR]     a tournament pass: every
//!       real pairing (private/tournament-617607/pairings.json) G times (default 100, half with
//!       each deck seated first), raw results in private/tournament-617607/passes/PASS_N.json
//!   aitool agree N AGENT [--params DIR]   agreement of AGENT and of the simple bot with the
//!       real players' decisions (server/goldens/tournament_replays), in passes/AGREE_N.json, and
//!       the decisions' records (offered / chosen cards) in private/.../decisions.json
//!
//! Decks: server/goldens/tournament_decks.json (deck id -> API deck content).

use altered_ai::play::{self, Agent, GameResult, Observation, RandomAgent, SimpleBot};
use altered_ai::{legal, replays, search};
use rand::SeedableRng;
use serde_json::json;
use altered_ai::search::{Ai, Params};
use altered_ai::stats::wilson;
use serde_json::Value;

const DECKS: &str = "server/goldens/tournament_decks.json";

fn agent(name: &str) -> Box<dyn Agent> {
    deck_agent(name, None, "ai/params")
}

/// The agent `name` for a deck: `ai` reads the deck's parameters (`DIR/DXX.json`, defaults else)
fn deck_agent(name: &str, deck: Option<&str>, dir: &str) -> Box<dyn Agent> {
    match name {
        "random" => Box::new(RandomAgent),
        "ai" => {
            let params = deck
                .and_then(|d| std::fs::read_to_string(format!("{}/{}.json", dir, d)).ok())
                .map(|s| serde_json::from_str::<Params>(&s).expect("params JSON"))
                .unwrap_or_default();
            Box::new(Ai::new("ai", params))
        }
        _ => Box::new(SimpleBot),
    }
}

const PRIVATE: &str = "private/tournament-617607";

/// A tournament pass: every real pairing `games` times
fn pass(n: u64, agent_name: &str, games: u64, threads: usize, params_dir: &str, decks: &Value) {
    let pairings: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/pairings.json", PRIVATE)).expect("pairings")).unwrap();
    let pairings = pairings["pairings"].as_array().unwrap().clone();
    // Same seeds at every pass: passes differ only by the AI
    let jobs: Vec<(usize, u64)> = (0..pairings.len()).flat_map(|p| (0..games).map(move |g| (p, g))).collect();
    let t = std::time::Instant::now();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results = play::run_parallel(
        &jobs,
        threads,
        |&(p, g)| {
            let (a, b) = (pairings[p]["a"].as_str().unwrap(), pairings[p]["b"].as_str().unwrap());
            let swap = g % 2 == 1;
            let (d0, d1) = if swap { (b, a) } else { (a, b) };
            let pair = [decks[d0].clone(), decks[d1].clone()];
            let mut agents = [deck_agent(agent_name, Some(d0), params_dir), deck_agent(agent_name, Some(d1), params_dir)];
            let seed = 10_000 + p as u64 * 1000 + g;
            let mut r = play::play_game(&pair, &mut agents, seed);
            if swap {
                r.winner = r.winner.map(|w| 1 - w);
                r.decisions.swap(0, 1);
                r.refused.swap(0, 1);
            }
            let k = done.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if k % 500 == 0 {
                eprintln!("{} / {} games, {:.0}s", k, jobs.len(), t.elapsed().as_secs_f64());
            }
            (p, swap, r)
        },
        |&(p, g), msg| (p, g % 2 == 1, GameResult { seed: 10_000 + p as u64 * 1000 + g, error: Some(format!("panic: {}", msg)), ..Default::default() }),
    );
    let mut out = vec![];
    for (i, pr) in pairings.iter().enumerate() {
        let games: Vec<Value> = results
            .iter()
            .filter(|(p, _, _)| *p == i)
            .map(|(_, swap, r)| {
                serde_json::json!({"seed": r.seed, "first": if *swap { "b" } else { "a" }, "winner": r.winner.map(|w| if w == 0 { "a" } else { "b" }),
                    "error": r.error, "day": r.day, "ms": r.millis, "decisions": r.decisions, "refused": r.refused})
            })
            .collect();
        let mut x = pr.clone();
        x["games"] = Value::Array(games);
        out.push(x);
    }
    let file = format!("{}/passes/PASS_{}.json", PRIVATE, n);
    std::fs::create_dir_all(format!("{}/passes", PRIVATE)).unwrap();
    let doc = serde_json::json!({"pass": n, "agent": agent_name, "games": games, "threads": threads, "wallSeconds": t.elapsed().as_secs_f64(), "pairings": out});
    std::fs::write(&file, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let errors = results.iter().filter(|(_, _, r)| r.error.is_some()).count();
    println!("pass {}: {} games, {} errors, {:.0}s wall -> {}", n, results.len(), errors, t.elapsed().as_secs_f64(), file);
}

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The state after an answer, as a comparable string (None: refused)
fn after(step: &replays::Step, answer: &Value) -> Option<String> {
    let mut g = step.game.clone();
    replays::set_next_ids(&mut g, &step.next_ids);
    g.answer(step.player, &legal::with_id(&step.decision, answer)).ok()?;
    Some(format!("{:?}{}", g.waiting_for(), g.oracle_state()))
}

fn class_of(g: &altered_rules::Game, id: &Value) -> String {
    search::card_key(g, id).unwrap_or_default()
}

fn agree(n: u64, agent_name: &str, params_dir: &str, decks: &Value, threads: usize) {
    let dir = "server/goldens/tournament_replays";
    let seats: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/table_seats.json", PRIVATE)).expect("table_seats")).unwrap();
    let mut files: Vec<String> = std::fs::read_dir(dir).expect(dir).flatten().map(|f| f.path().to_string_lossy().to_string()).filter(|f| f.ends_with(".json")).collect();
    files.sort();
    let t = std::time::Instant::now();
    let per_file = play::run_parallel(
        &files,
        threads,
        |f| {
            let table = std::path::Path::new(f).file_stem().unwrap().to_string_lossy().to_string();
            let deck_of = |p: u32| seats[&table][p.to_string()].as_str().unwrap_or("").to_string();
            let pair = [decks[deck_of(2001)].clone(), decks[deck_of(2002)].clone()];
            let mut agents = [deck_agent(agent_name, Some(&deck_of(2001)), params_dir), deck_agent(agent_name, Some(&deck_of(2002)), params_dir)];
            let mut recs = vec![];
            let steps = match replays::steps(f) {
                Ok(s) => s,
                Err(e) => return vec![json!({"table": table, "error": e})],
            };
            for (i, st) in steps.iter().enumerate() {
                let cands = legal::candidates(&st.decision);
                let real_type = st.answer["type"].as_str().unwrap_or("");
                if cands.len() < 2 || real_type == "undo" {
                    continue;
                }
                let Some(real) = after(st, &st.answer) else { continue };
                let seat = (st.player - 2001) as usize;
                let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(i as u64 * 7919 + table.len() as u64);
                let obs = Observation::new(&st.game, st.player as i64, &pair);
                let a = agents[seat].decide(&obs, &st.decision, &mut rng);
                let same_ai = legal::canonical(&a) == legal::canonical(&st.answer) || after(st, &a).as_deref() == Some(real.as_str());
                let mut b = st.game.clone();
                replays::set_next_ids(&mut b, &st.next_ids);
                let same_simple = search::bot_step(&mut b, st.player as i64) && format!("{:?}{}", b.waiting_for(), b.oracle_state()) == real;
                // What was offered and chosen (cards by class), for the decks' tendencies
                let offered: Vec<String> = match st.decision["kind"].as_str() {
                    Some("turn") => st.decision["play"].as_array().into_iter().flatten().map(|p| class_of(&st.game, &p["cardId"])).collect(),
                    Some("select") => st.decision["cardIds"].as_array().into_iter().flatten().map(|c| class_of(&st.game, c)).collect(),
                    _ => vec![],
                };
                let chosen: Vec<String> = match real_type {
                    "play" | "support" | "tap" => vec![class_of(&st.game, &st.answer["cardId"])],
                    "select" => st.answer["cardIds"].as_array().into_iter().flatten().map(|c| class_of(&st.game, c)).collect(),
                    _ => vec![],
                };
                let day = st.game.oracle_state()["day"].clone();
                recs.push(json!({"table": table, "deck": deck_of(st.player), "kind": st.decision["kind"], "prompt": st.decision["prompt"], "day": day,
                    "real": legal::canonical(&st.answer), "ai": legal::canonical(&a), "sameAi": same_ai, "sameSimple": same_simple,
                    "offered": offered, "chosen": chosen, "candidates": cands.len()}));
            }
            if recs.is_empty() {
                recs.push(json!({"table": table, "error": "no step"}));
            }
            recs
        },
        |f, msg| vec![json!({"table": f, "error": format!("panic: {}", msg)})],
    );
    let recs: Vec<Value> = per_file.into_iter().flatten().collect();
    let mut overall = (0, 0);
    let mut simple = (0, 0);
    let mut by_kind: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut simple_by_kind: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut by_deck: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut errors = vec![];
    for r in &recs {
        if r.get("error").is_some() {
            errors.push(r.clone());
            continue;
        }
        let k = r["kind"].as_str().unwrap_or("").to_string();
        let (sa, ss) = (r["sameAi"] == true, r["sameSimple"] == true);
        overall.1 += 1;
        overall.0 += sa as usize;
        simple.1 += 1;
        simple.0 += ss as usize;
        let e = by_kind.entry(k.clone()).or_default();
        e.1 += 1;
        e.0 += sa as usize;
        let e = simple_by_kind.entry(k).or_default();
        e.1 += 1;
        e.0 += ss as usize;
        let e = by_deck.entry(r["deck"].as_str().unwrap_or("").into()).or_default();
        e.1 += 1;
        e.0 += sa as usize;
    }
    let m = |x: &std::collections::BTreeMap<String, (usize, usize)>| -> Value { Value::Object(x.iter().map(|(k, v)| (k.clone(), json!({"same": v.0, "n": v.1}))).collect()) };
    let doc = json!({"pass": n, "agent": agent_name, "overall": {"same": overall.0, "n": overall.1}, "simple": {"same": simple.0, "n": simple.1},
        "byKind": m(&by_kind), "simpleByKind": m(&simple_by_kind), "byDeck": m(&by_deck), "errors": errors, "files": files.len()});
    std::fs::create_dir_all(format!("{}/passes", PRIVATE)).unwrap();
    std::fs::write(format!("{}/passes/AGREE_{}.json", PRIVATE, n), serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    std::fs::write(format!("{}/decisions.json", PRIVATE), serde_json::to_string(&recs).unwrap()).unwrap();
    println!(
        "agree {}: {} files, {} decisions, AI {:.1} %, simple bot {:.1} %, {} errors, {:.0}s",
        n, files.len(), overall.1, 100.0 * overall.0 as f64 / overall.1.max(1) as f64, 100.0 * simple.0 as f64 / simple.1.max(1) as f64, errors.len(), t.elapsed().as_secs_f64()
    );
}

fn load_pairings() -> Vec<(String, String)> {
    let p: Value = serde_json::from_str(&std::fs::read_to_string(format!("{}/pairings.json", PRIVATE)).expect("pairings")).unwrap();
    p["pairings"].as_array().unwrap().iter().map(|x| (x["a"].as_str().unwrap().to_string(), x["b"].as_str().unwrap().to_string())).collect()
}

fn read_params(dir: &str, deck: &str) -> Params {
    std::fs::read_to_string(format!("{}/{}.json", dir, deck)).ok().map(|s| serde_json::from_str::<Params>(&s).expect("params JSON")).unwrap_or_default()
}

/// Version A (params in dir_a) against version B (dir_b) on the real pairings: each pairing is
/// played with A on either deck and in either seat, so decks and seats cancel out. Returns A's
/// wins and the decided games.
fn ab(dir_a: &str, dir_b: &str, per_pairing: u64, limit: usize, threads: usize, decks: &Value, seed0: u64) -> (f64, f64, f64) {
    let pairings: Vec<(String, String)> = load_pairings().into_iter().take(limit).collect();
    let jobs: Vec<(usize, u64)> = (0..pairings.len()).flat_map(|p| (0..per_pairing).map(move |g| (p, g))).collect();
    let res = play::run_parallel(
        &jobs,
        threads,
        |&(p, g)| {
            let (a, b) = &pairings[p];
            // g % 4: which deck A plays, which seat goes first
            let a_on_first_deck = g % 2 == 0;
            let swap = (g / 2) % 2 == 1;
            let (d0, d1) = if swap { (b, a) } else { (a, b) };
            let a_seat = if (d0 == a) == a_on_first_deck { 0 } else { 1 };
            let pair = [decks[d0.as_str()].clone(), decks[d1.as_str()].clone()];
            let dirs = if a_seat == 0 { [dir_a, dir_b] } else { [dir_b, dir_a] };
            let mut agents: [Box<dyn Agent>; 2] = [Box::new(Ai::new("x", read_params(dirs[0], d0))), Box::new(Ai::new("y", read_params(dirs[1], d1)))];
            let r = play::play_game(&pair, &mut agents, seed0 + p as u64 * 1000 + g / 4);
            (r.winner.map(|w| (w == a_seat) as u8 as f64), r.millis)
        },
        |_, _| (None, 0),
    );
    let wins: f64 = res.iter().filter_map(|r| r.0).sum();
    let n = res.iter().filter(|r| r.0.is_some()).count() as f64;
    let ms: u64 = res.iter().map(|r| r.1).sum();
    (wins, n, ms as f64 / 1000.0 / res.len().max(1) as f64)
}

/// Multiplies each weight by exp(sigma * N(0, 1))
fn perturb(p: &Params, sigma: f64, rng: &mut rand_chacha::ChaCha8Rng) -> Params {
    use rand::Rng;
    let mut q = p.clone();
    let mut w = serde_json::to_value(&q.weights).unwrap();
    for (_, v) in w.as_object_mut().unwrap().iter_mut() {
        if let Some(x) = v.as_f64() {
            // Box-Muller
            let (u1, u2): (f64, f64) = (rng.random_range(1e-9..1.0), rng.random_range(0.0..1.0));
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            *v = json!(x * (sigma * z).exp());
        }
    }
    q.weights = serde_json::from_value(w).unwrap();
    let (u1, u2): (f64, f64) = (rng.random_range(1e-9..1.0), rng.random_range(0.0..1.0));
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    q.end_turn_bias = p.end_turn_bias + sigma * 2.0 * z;
    q
}

/// Per-deck evolution strategy: for every deck, `cands` perturbed parameter sets are played,
/// with the same seeds as the current set, against the deck's tournament opponents and two other
/// decks of the field (their current parameters); the best replaces the current one when it wins
/// at least `margin` more of the games. Writes every deck's parameters to `out`.
#[allow(clippy::too_many_arguments)]
fn evolve(round: u64, dir: &str, out: &str, cands: usize, games: u64, sigma: f64, margin: f64, threads: usize, decks: &Value) {
    let ids: Vec<String> = decks.as_object().unwrap().keys().cloned().collect();
    let pairings = load_pairings();
    let opponents = |d: &str| -> Vec<String> {
        let mut o: Vec<String> = pairings.iter().filter_map(|(a, b)| if a == d { Some(b.clone()) } else if b == d { Some(a.clone()) } else { None }).collect();
        let i = ids.iter().position(|x| x == d).unwrap();
        for k in [7, 19] {
            let f = ids[(i + k) % ids.len()].clone();
            if !o.contains(&f) && f != d {
                o.push(f);
            }
        }
        o
    };
    // Candidate sets per deck (index 0: the current parameters)
    let mut sets: std::collections::BTreeMap<String, Vec<Params>> = Default::default();
    for (i, d) in ids.iter().enumerate() {
        let base = read_params(dir, d);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(round * 7_777 + i as u64);
        let mut v = vec![base.clone()];
        for _ in 0..cands {
            v.push(perturb(&base, sigma, &mut rng));
        }
        sets.insert(d.clone(), v);
    }
    let mut jobs = vec![];
    for d in &ids {
        let opps = opponents(d);
        for c in 0..=cands {
            for g in 0..games {
                jobs.push((d.clone(), c, opps[g as usize % opps.len()].clone(), g));
            }
        }
    }
    let t = std::time::Instant::now();
    let res = play::run_parallel(
        &jobs,
        threads,
        |(d, c, o, g)| {
            let swap = g % 2 == 1;
            let me: Box<dyn Agent> = Box::new(Ai::new("me", sets[d][*c].clone()));
            let opp: Box<dyn Agent> = Box::new(Ai::new("opp", read_params(dir, o)));
            let (pair, mut agents, my_seat) = if swap {
                ([decks[o.as_str()].clone(), decks[d.as_str()].clone()], [opp, me], 1)
            } else {
                ([decks[d.as_str()].clone(), decks[o.as_str()].clone()], [me, opp], 0)
            };
            // Held-out seeds (not the pass's), the same for every candidate of a deck
            let r = play::play_game(&pair, &mut agents, 900_000 + round * 100_000 + g * 97 + d[1..].parse::<u64>().unwrap_or(0) * 1000);
            r.winner.map(|w| (w == my_seat) as u8 as f64)
        },
        |_, _| None,
    );
    std::fs::create_dir_all(out).unwrap();
    let mut log = vec![];
    for d in &ids {
        let mut rates = vec![];
        for c in 0..=cands {
            let rs: Vec<f64> = jobs.iter().zip(&res).filter(|((x, k, _, _), _)| x == d && *k == c).filter_map(|(_, r)| *r).collect();
            rates.push(rs.iter().sum::<f64>() / rs.len().max(1) as f64);
        }
        let best = (1..=cands).max_by(|&x, &y| rates[x].partial_cmp(&rates[y]).unwrap()).unwrap_or(0);
        let keep = cands > 0 && rates[best] >= rates[0] + margin;
        let chosen = if keep { &sets[d][best] } else { &sets[d][0] };
        std::fs::write(format!("{}/{}.json", out, d), serde_json::to_string_pretty(chosen).unwrap()).unwrap();
        log.push(json!({"deck": d, "base": rates[0], "best": rates[best], "kept": keep, "rates": rates}));
        println!("{} base {:.0} % best {:.0} % {}", d, 100.0 * rates[0], 100.0 * rates[best], if keep { "-> kept" } else { "" });
    }
    std::fs::write(format!("{}/passes/EVOLVE_{}.json", PRIVATE, round), serde_json::to_string_pretty(&log).unwrap()).unwrap();
    println!("evolve {}: {} games, {:.0}s", round, jobs.len(), t.elapsed().as_secs_f64());
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = opt(&args, "--threads").and_then(|s| s.parse().ok()).unwrap_or(10);
    let decks: Value = serde_json::from_str(&std::fs::read_to_string(DECKS).expect(DECKS)).expect("decks JSON");
    match args.get(1).map(|s| s.as_str()) {
        Some("match") => {
            let (a, b) = (&args[2], &args[3]);
            let n: u64 = args[4].parse().unwrap();
            let (ag_a, ag_b) = (args.get(5).cloned().unwrap_or("simple".into()), args.get(6).cloned().unwrap_or("simple".into()));
            let jobs: Vec<u64> = (0..n).collect();
            let t = std::time::Instant::now();
            let results: Vec<(u64, GameResult)> = play::run_parallel(
                &jobs,
                threads,
                |&i| {
                    // Even games: A seated first; odd games: B first
                    let swap = i % 2 == 1;
                    let (d0, d1, n0, n1) = if swap { (b, a, &ag_b, &ag_a) } else { (a, b, &ag_a, &ag_b) };
                    let deck_pair = [decks[d0].clone(), decks[d1].clone()];
                    let mut agents = [agent(n0), agent(n1)];
                    let seed = 1000 + i;
                    let mut r = play::play_game(&deck_pair, &mut agents, seed);
                    // Report as seat 0 = A
                    if swap {
                        r.winner = r.winner.map(|w| 1 - w);
                        r.decisions.swap(0, 1);
                        r.refused.swap(0, 1);
                    }
                    (i, r)
                },
                |&i, msg| (i, GameResult { seed: 1000 + i, error: Some(format!("panic: {}", msg)), ..Default::default() }),
            );
            let wins = results.iter().filter(|(_, r)| r.winner == Some(0)).count() as f64;
            let decided = results.iter().filter(|(_, r)| r.winner.is_some()).count() as f64;
            let errors: Vec<&GameResult> = results.iter().map(|(_, r)| r).filter(|r| r.error.is_some()).collect();
            let (lo, hi) = wilson(wins, decided);
            let ms: u64 = results.iter().map(|(_, r)| r.millis).sum();
            let dec: usize = results.iter().map(|(_, r)| r.decisions[0] + r.decisions[1]).sum();
            let refused: usize = results.iter().map(|(_, r)| r.refused[0] + r.refused[1]).sum();
            println!(
                "{} ({}) vs {} ({}): {} games, A wins {:.1} % [{:.1}, {:.1}] of {} decided, {} errors, {} refused answers, {:.0} ms CPU/game, {:.0} decisions/game, wall {:.1}s",
                a, ag_a, b, ag_b, n, 100.0 * wins / decided.max(1.0), 100.0 * lo, 100.0 * hi, decided, errors.len(), refused,
                ms as f64 / n as f64, dec as f64 / n as f64, t.elapsed().as_secs_f64()
            );
            for e in errors.iter().take(5) {
                println!("  seed {}: {}", e.seed, e.error.as_deref().unwrap_or(""));
            }
        }
        Some("mirror") => {
            // AGENT_A with each deck against AGENT_B with the same deck: aitool mirror N AGENT_A AGENT_B [--decks D01,D05] [--params DIR] [--params-b DIR]
            let n: u64 = args[2].parse().unwrap();
            let (ag_a, ag_b) = (args[3].clone(), args[4].clone());
            let dir_a = opt(&args, "--params").unwrap_or("ai/params".into());
            let dir_b = opt(&args, "--params-b").unwrap_or(dir_a.clone());
            let seed0: u64 = opt(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or(500_000);
            let list: Vec<String> = match opt(&args, "--decks") {
                Some(l) => l.split(',').map(String::from).collect(),
                None => decks.as_object().unwrap().keys().cloned().collect(),
            };
            let jobs: Vec<(String, u64)> = list.iter().flat_map(|d| (0..n).map(move |g| (d.clone(), g))).collect();
            let t = std::time::Instant::now();
            let res = play::run_parallel(
                &jobs,
                threads,
                |(d, g)| {
                    let swap = g % 2 == 1;
                    let pair = [decks[d].clone(), decks[d].clone()];
                    let a = deck_agent(&ag_a, Some(d), &dir_a);
                    let b = deck_agent(&ag_b, Some(d), &dir_b);
                    let mut agents = if swap { [b, a] } else { [a, b] };
                    let mut r = play::play_game(&pair, &mut agents, seed0 + g + 1000 * d[1..].parse::<u64>().unwrap_or(0));
                    if swap {
                        r.winner = r.winner.map(|w| 1 - w);
                    }
                    (d.clone(), r)
                },
                |(d, g), msg| (d.clone(), GameResult { seed: *g, error: Some(msg), ..Default::default() }),
            );
            let mut tot = (0.0, 0.0);
            let mut line = String::new();
            for d in &list {
                let rs: Vec<&GameResult> = res.iter().filter(|(x, _)| x == d).map(|(_, r)| r).collect();
                let w = rs.iter().filter(|r| r.winner == Some(0)).count() as f64;
                let k = rs.iter().filter(|r| r.winner.is_some()).count() as f64;
                tot.0 += w;
                tot.1 += k;
                line += &format!(" {}:{:.0}", d, 100.0 * w / k.max(1.0));
            }
            let (lo, hi) = wilson(tot.0, tot.1);
            let ms: u64 = res.iter().map(|(_, r)| r.millis).sum();
            let errs = res.iter().filter(|(_, r)| r.error.is_some()).count();
            println!("{} vs {}: {:.1} % [{:.1}, {:.1}] of {} games, {} errors, {:.2} s CPU/game, wall {:.0}s\n {}", ag_a, ag_b, 100.0 * tot.0 / tot.1.max(1.0), 100.0 * lo, 100.0 * hi, tot.1, errs, ms as f64 / 1000.0 / res.len() as f64, t.elapsed().as_secs_f64(), line);
        }
        Some("ab") => {
            // aitool ab GAMES_PER_PAIRING --params A --params-b B [--pairings K] [--seed S]
            let n: u64 = args[2].parse().unwrap();
            let a = opt(&args, "--params").unwrap_or("ai/params".into());
            let b = opt(&args, "--params-b").unwrap_or("ai/params".into());
            let k: usize = opt(&args, "--pairings").and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
            let seed: u64 = opt(&args, "--seed").and_then(|s| s.parse().ok()).unwrap_or(700_000);
            let t = std::time::Instant::now();
            let (w, d, cpu) = ab(&a, &b, n, k, threads, &decks, seed);
            let (lo, hi) = wilson(w, d);
            println!("{} vs {}: A wins {:.1} % [{:.1}, {:.1}] of {} games, {:.2} s CPU/game, wall {:.0}s", a, b, 100.0 * w / d.max(1.0), 100.0 * lo, 100.0 * hi, d, cpu, t.elapsed().as_secs_f64());
        }
        Some("evolve") => {
            // aitool evolve ROUND --params DIR --out DIR2 [--cands C] [--games G] [--sigma S] [--margin M]
            let round: u64 = args[2].parse().unwrap();
            let dir = opt(&args, "--params").unwrap_or("ai/params".into());
            let out = opt(&args, "--out").expect("--out");
            let c: usize = opt(&args, "--cands").and_then(|s| s.parse().ok()).unwrap_or(3);
            let g: u64 = opt(&args, "--games").and_then(|s| s.parse().ok()).unwrap_or(40);
            let sigma: f64 = opt(&args, "--sigma").and_then(|s| s.parse().ok()).unwrap_or(0.3);
            let margin: f64 = opt(&args, "--margin").and_then(|s| s.parse().ok()).unwrap_or(0.075);
            evolve(round, &dir, &out, c, g, sigma, margin, threads, &decks);
        }
        Some("agree") => {
            let n: u64 = args[2].parse().unwrap();
            let dir = opt(&args, "--params").unwrap_or("ai/params".into());
            agree(n, &args[3], &dir, &decks, threads);
        }
        Some("pass") => {
            let n: u64 = args[2].parse().unwrap();
            let games: u64 = opt(&args, "--games").and_then(|s| s.parse().ok()).unwrap_or(100);
            let dir = opt(&args, "--params").unwrap_or("ai/params".into());
            pass(n, &args[3], games, threads, &dir, &decks);
        }
        _ => eprintln!("usage: aitool match DECK_A DECK_B N [AGENT_A AGENT_B] [--threads T] | pass N AGENT [--games G]"),
    }
}
