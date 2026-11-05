//! Position tests (`altered_ai::positions`): take positions from games played on the arena
//! server, say what a good answer is, and check the AIs on them.
//!
//!   positions list GAME [--all]          the decisions of a game (GAME: an id of data/games, or
//!                                        a path): event number, player, day, the answer given.
//!                                        Turn decisions only, unless --all; a card played for
//!                                        nothing is marked
//!   positions add GAME EVENT NAME [--avoid P]... [--prefer P]... [--note TEXT]
//!                                        writes ai/positions/NAME.json: the position before that
//!                                        event, and the expectation
//!   positions show NAME                  the position: header, candidates, answer given, expectation
//!   positions run [FILTER] [--ai ID]... [--n N]
//!                                        asks each AI N times (default 5, other random streams)
//!                                        every position whose name contains FILTER; default
//!                                        AI: the one that played the position. --off: without
//!                                        the filter of the plays that do nothing
//!
//! P: `TYPE[:CARD][@DESTINATION]`, `select:CARD|CARD` (e.g. `play:Sticky Note Seals`, `endTurn`).
//! AI: `deck:DXX` (ai/params), `imitation:STEM` (private/tournament-617607/imitation/players),
//! `simple`, `random`.
use altered_ai::play::{Agent, Observation, RandomAgent, SimpleBot};
use altered_ai::positions::{self, Expect, Pattern, Position};
use altered_ai::record::{self, Record};
use altered_ai::search::{Ai, Params};
use altered_imitate::agent::{Imitator, PlayerFile, Shared};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const DIR: &str = "ai/positions";
const IMITATION: &str = "private/tournament-617607/imitation";

fn main() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(run).unwrap().join().unwrap();
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path, e))?).map_err(|e| format!("{}: {}", path, e))
}

/// The value after `--flag` for every occurrence
fn flags(a: &[String], flag: &str) -> Vec<String> {
    a.windows(2).filter(|w| w[0] == flag).map(|w| w[1].clone()).collect()
}

/// The arguments that are neither flags nor flag values
fn plain(a: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut skip = false;
    for x in a {
        if skip {
            skip = false;
        } else if x == "--all" || x == "--off" {
        } else if x.starts_with("--") {
            skip = true;
        } else {
            out.push(x.clone());
        }
    }
    out
}

fn agent(id: &str) -> Result<Box<dyn Agent>, String> {
    if id == "simple" {
        return Ok(Box::new(SimpleBot));
    }
    if id == "random" {
        return Ok(Box::new(RandomAgent));
    }
    let params = |d: &str| read::<Params>(&format!("ai/params/{}.json", d));
    if let Some(stem) = id.strip_prefix("imitation:") {
        let p: PlayerFile = read(&format!("{}/players/{}.json", IMITATION, stem))?;
        let shared: Shared = read(&format!("{}/shared.json", IMITATION))?;
        return Ok(Box::new(Imitator::new(&p.label, &shared, Some(&p), params(&p.deck).ok())));
    }
    if let Some(d) = id.strip_prefix("deck:") {
        return Ok(Box::new(Ai::new(id, params(d)?)));
    }
    Err(format!("unknown AI {}", id))
}

/// A saved game of the arena server: its record and seats
fn arena_game(game: &str) -> Result<(String, Record, Vec<Value>), String> {
    let path = if game.ends_with(".json") { game.to_string() } else { format!("{}/games/{}.json", std::env::var("DATA_DIR").unwrap_or("data".into()), game) };
    let v: Value = read(&path)?;
    let record: Record = serde_json::from_value(v["record"].clone()).unwrap_or_default();
    if record.initial.is_none() {
        return Err(format!("{}: no record of the moves (a game saved before the server recorded them)", path));
    }
    Ok((v["id"].as_str().unwrap_or(game).to_string(), record, v["seats"].as_array().cloned().unwrap_or_default()))
}

fn seat_label(seats: &[Value], pid: u32) -> String {
    seats
        .iter()
        .find(|s| s["id"] == json!(pid))
        .map(|s| format!("{} ({})", s["name"].as_str().unwrap_or(""), s["ai"].as_str().unwrap_or(if s["bot"] == true { "bot" } else { "human" })))
        .unwrap_or_default()
}

fn list(a: &[String]) -> Result<(), String> {
    let p = plain(a);
    let (_, record, seats) = arena_game(p.first().ok_or("positions list GAME")?)?;
    let all = a.iter().any(|x| x == "--all");
    let mut g = record.initial.clone().unwrap();
    for (k, e) in record.events.iter().enumerate() {
        if let (Some(pid), Some(body)) = (e.player(), e.body()) {
            let d = g.try_decision(pid).ok().flatten().unwrap_or(Value::Null);
            if all || d["kind"] == "turn" {
                let decks = [Value::Null, Value::Null];
                let nothing = altered_ai::useful::is_useless_play(&Observation::new(&g, pid as i64, &decks), &d, body);
                println!(
                    "#{:<4} {} — {} — {}: {}{}",
                    k,
                    seat_label(&seats, pid),
                    positions::header(&g, pid),
                    d["kind"].as_str().unwrap_or("?"),
                    positions::describe(&g, body),
                    if nothing { "  ← does nothing" } else { "" }
                );
            }
        }
        record::apply(&mut g, e).map_err(|err| format!("event {}: {}", k, err))?;
    }
    Ok(())
}

fn add(a: &[String]) -> Result<(), String> {
    let p = plain(a);
    let [game, event, name] = [p.first(), p.get(1), p.get(2)].map(|x| x.cloned().unwrap_or_default());
    if name.is_empty() {
        return Err("positions add GAME EVENT NAME [--avoid P]... [--prefer P]... [--note TEXT]".into());
    }
    let k: usize = event.trim_start_matches('#').parse().map_err(|_| format!("bad event number {}", event))?;
    let (id, record, seats) = arena_game(&game)?;
    let e = record.events.get(k).ok_or(format!("the game has {} events", record.events.len()))?;
    let pid = e.player().ok_or(format!("event {} is not a player's answer", k))?;
    let g = record.replay(k)?;
    let expect = Expect {
        avoid: flags(a, "--avoid").iter().map(|s| Pattern::parse(s)).collect(),
        prefer: flags(a, "--prefer").iter().map(|s| Pattern::parse(s)).collect(),
    };
    if expect.avoid.is_empty() && expect.prefer.is_empty() {
        return Err("give at least one --avoid or --prefer".into());
    }
    let ai = seats.iter().find(|s| s["id"] == json!(pid)).and_then(|s| s["ai"].as_str()).map(String::from);
    let pos = Position {
        name: name.clone(),
        note: flags(a, "--note").join(" "),
        source: json!({"game": id, "event": k}),
        seat: pid,
        ai,
        played: e.body().cloned(),
        expect,
        game: g,
    };
    let d = pos.decision()?;
    // Each pattern must name a candidate of the decision, else it checks nothing
    for p in pos.expect.avoid.iter().chain(&pos.expect.prefer) {
        if !altered_ai::legal::candidates(&d).iter().any(|c| p.matches(&pos.game, c)) {
            return Err(format!("{} matches no candidate of the decision: {}", p, positions::describe_candidates(&pos.game, &d).join("; ")));
        }
    }
    std::fs::create_dir_all(DIR).map_err(|e| e.to_string())?;
    let path = format!("{}/{}.json", DIR, name);
    pos.save(&path)?;
    println!("{}: {} — played {}", path, positions::header(&pos.game, pid), pos.played.as_ref().map(|b| positions::describe(&pos.game, b)).unwrap_or_default());
    Ok(())
}

fn position_path(name: &str) -> String {
    if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{}/{}.json", DIR, name)
    }
}

fn show(a: &[String]) -> Result<(), String> {
    let p = plain(a);
    let pos = Position::load(&position_path(p.first().ok_or("positions show NAME")?))?;
    let d = pos.decision()?;
    println!("{} — {}", pos.name, pos.note);
    println!("source {}, player {} ({}), {}", pos.source, pos.seat, pos.ai.as_deref().unwrap_or("-"), positions::header(&pos.game, pos.seat));
    println!("decision {}: {}", d["kind"], d["prompt"]);
    for c in altered_ai::legal::candidates(&d) {
        println!("  {} {}", if pos.expect.check(&pos.game, &c) { "ok  " } else { "FAIL" }, positions::describe(&pos.game, &c));
    }
    if let Some(b) = &pos.played {
        println!("played in the game: {}", positions::describe(&pos.game, b));
    }
    let pats = |v: &[Pattern]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ");
    println!("avoid: {}; prefer: {}", pats(&pos.expect.avoid), pats(&pos.expect.prefer));
    Ok(())
}

fn run_tests(a: &[String]) -> Result<(), String> {
    let filter = plain(a).first().cloned().unwrap_or_default();
    let n: u64 = flags(a, "--n").first().and_then(|x| x.parse().ok()).unwrap_or(5);
    let ais = flags(a, "--ai");
    if a.iter().any(|x| x == "--off") {
        altered_ai::useful::DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let mut files: Vec<String> = std::fs::read_dir(DIR)
        .map_err(|e| format!("{}: {}", DIR, e))?
        .flatten()
        .map(|f| f.path().to_string_lossy().to_string())
        .filter(|p| p.ends_with(".json"))
        .collect();
    files.sort();
    let (mut passed, mut total) = (0, 0);
    println!("| Position | AI | Passed | Answers |");
    println!("| --- | --- | --- | --- |");
    for f in files {
        let pos = Position::load(&f)?;
        if !pos.name.contains(&filter) {
            continue;
        }
        let d = pos.decision()?;
        let ids: Vec<String> = if ais.is_empty() { vec![pos.ai.clone().unwrap_or("simple".into())] } else { ais.clone() };
        let decks = [Value::Null, Value::Null];
        let obs = Observation::new(&pos.game, pos.seat as i64, &decks);
        for id in ids {
            let mut ok = 0;
            let mut answers: BTreeMap<String, usize> = BTreeMap::new();
            for seed in 0..n {
                let mut ag = agent(&id)?;
                let mut rng = ChaCha8Rng::seed_from_u64(seed);
                let ans = ag.decide(&obs, &d, &mut rng);
                ok += pos.expect.check(&pos.game, &ans) as u64;
                *answers.entry(positions::describe(&pos.game, &ans)).or_default() += 1;
            }
            passed += (ok == n) as usize;
            total += 1;
            let list: Vec<String> = answers.iter().map(|(a, k)| format!("{} ×{}", a, k)).collect();
            println!("| {} | {} | {}/{}{} | {} |", pos.name, id, ok, n, if ok == n { "" } else { " ✗" }, list.join(", "));
        }
    }
    println!();
    println!("{} of {} (position, AI) pairs pass every time", passed, total);
    Ok(())
}

fn run() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let rest = a.get(1..).unwrap_or(&[]);
    let r = match a.first().map(String::as_str) {
        Some("list") => list(rest),
        Some("add") => add(rest),
        Some("show") => show(rest),
        Some("run") => run_tests(rest),
        _ => Err("positions list|add|show|run (see the source header)".into()),
    };
    if let Err(e) = r {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
