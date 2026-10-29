//! Replays the golden decision trees recorded with the PHP engine (server/tools/explore.php) against
//! the Rust translation: at every node, the player waited for, the decision asked and the state must
//! be the same; then every recorded answer is applied to a copy of the game.
//!
//!   cargo run --release -p goldens -- [DIR] [--only SUBSTRING] [-v] [--create]
//!
//! DIR defaults to server/goldens. Prints the first difference of each failing file and a summary.
//! --create: whole games start from a game created by the translation (Game::create with the
//! recorded game id, decks and dice), whose tables must be those PHP dumped after V1::createGame.

use altered_rules::arena::NewPlayer;
use altered_rules::Game;
use rayon::prelude::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// Texts don't have to match (PHP_ENGINE.md §2.6: only empty vs non-empty descriptions matter)
fn strip_texts(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, x) in m {
                match k.as_str() {
                    "id" | "prompt" => {}
                    "label" => {
                        out.insert(k.clone(), json!(x.as_str().map(|s| !s.is_empty()).unwrap_or(false)));
                    }
                    _ => {
                        out.insert(k.clone(), strip_texts(x));
                    }
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(strip_texts).collect()),
        _ => v.clone(),
    }
}

/// explore.php compactState: card ids per player zone (deck in order, top last; other zones sorted),
/// non-default marks, positions, day, phase, first player, revealed tumult cards
fn compact_state(state: &Value) -> Value {
    let mut cards: Vec<&Value> = state["cards"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    cards.sort_by_key(|c| (c["order"].as_i64().unwrap_or(0), c["id"].as_i64().unwrap_or(0)));
    let mut zones: Map<String, Value> = Map::new();
    let mut marks: Map<String, Value> = Map::new();
    for c in cards {
        let zone = c["zone"].as_str().unwrap_or("");
        let key = format!("{}:{}", c["owner"], zone);
        zones.entry(key).or_insert_with(|| json!([])).as_array_mut().unwrap().push(c["id"].clone());
        let mut m = Map::new();
        if c["exhausted"].as_bool() == Some(true) && !["deck", "hand", "discard"].contains(&zone) {
            m.insert("x".into(), json!(1));
        }
        if c["boosts"].as_i64().unwrap_or(0) != 0 {
            m.insert("b".into(), c["boosts"].clone());
        }
        if c["statuses"].as_array().is_some_and(|s| !s.is_empty()) {
            m.insert("s".into(), c["statuses"].clone());
        }
        if c["counter"].as_i64().unwrap_or(0) != 0 {
            m.insert("c".into(), c["counter"].clone());
        }
        if !m.is_empty() {
            marks.insert(c["id"].to_string(), Value::Object(m));
        }
    }
    for (k, ids) in zones.iter_mut() {
        if !k.ends_with(":deck") {
            ids.as_array_mut().unwrap().sort_by_key(|v| v.as_i64().unwrap_or(0));
        }
    }
    let mut positions = Map::new();
    for p in state["players"].as_array().into_iter().flatten() {
        positions.insert(p["id"].to_string(), p["position"].clone());
    }
    let storm: Vec<Value> = state["storm"].as_array().into_iter().flatten().map(|t| json!(t["revealed"].as_bool().unwrap_or(false) as i32)).collect();
    json!({
        "zones": zones, "marks": marks, "positions": positions,
        "day": state["day"], "phase": state["phase"], "first": state["first"], "storm": storm,
    })
}

/// Normalizes key order and number types so that equal states compare equal
fn canonical(v: &Value) -> String {
    fn walk(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for k in keys {
                    out.insert(k.clone(), walk(&m[k]));
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(walk).collect()),
            Value::String(s) => s.parse::<i64>().map(|n| json!(n)).unwrap_or_else(|_| v.clone()),
            _ => v.clone(),
        }
    }
    walk(v).to_string()
}

struct Report {
    nodes: usize,
    passed: usize,
    first_failure: Option<String>,
}

fn check_node(game: &Game, node: &Value, path: &str, r: &mut Report) {
    r.nodes += 1;
    let mut problems = vec![];
    let php_state = &node["state"];
    let rust_state = compact_state(&game.oracle_state());
    if canonical(php_state) != canonical(&rust_state) {
        problems.push(format!("state differs\n      php  {}\n      rust {}", canonical(php_state), canonical(&rust_state)));
    }
    let player = node["player"].as_u64().map(|p| p as u32);
    if let Some(p) = player {
        if game.waiting_for().first() != Some(&p) {
            problems.push(format!("waiting for: php {} rust {:?}", p, game.waiting_for()));
        }
        let php_d = strip_texts(&node["decision"]);
        let rust_d = game.decision(p).map(|d| strip_texts(&d)).unwrap_or(Value::Null);
        if canonical(&php_d) != canonical(&rust_d) {
            problems.push(format!("decision differs\n      php  {}\n      rust {}", canonical(&php_d), canonical(&rust_d)));
        }
    }
    if !problems.is_empty() {
        if r.first_failure.is_none() {
            r.first_failure = Some(format!("at {}:\n    {}", if path.is_empty() { "start" } else { path }, problems.join("\n    ")));
        }
        return;
    }
    r.passed += 1;
    let Some(p) = player else { return };
    for (i, next) in node["next"].as_array().into_iter().flatten().enumerate() {
        let mut g = game.clone();
        let mut answer = next["answer"].clone();
        if let Some(id) = game.decision(p).and_then(|d| d.get("id").cloned()) {
            answer["decisionId"] = id;
        }
        let step = format!("{}{}{}", path, if path.is_empty() { "" } else { " > " }, compact_answer(&next["answer"], i));
        match (g.answer(p, &answer), next.get("node")) {
            (Ok(()), Some(child)) => check_node(&g, child, &step, r),
            (Err(_), None) => {}
            (Ok(()), None) => {
                if r.first_failure.is_none() {
                    r.first_failure = Some(format!("at {}: php refused this answer ({}), rust accepted it", step, next["error"]));
                }
            }
            (Err(e), Some(_)) => {
                if r.first_failure.is_none() {
                    r.first_failure = Some(format!("at {}: rust refused this answer: {}", step, e));
                }
            }
        }
    }
}

fn compact_answer(a: &Value, i: usize) -> String {
    let mut m = a.as_object().cloned().unwrap_or_default();
    let t = m.remove("type").and_then(|t| t.as_str().map(String::from)).unwrap_or_default();
    format!("#{} {} {}", i, t, Value::Object(m))
}

fn main() {
    // A request runs whole chains of states recursively (jumpToState is re-entrant, as in PHP):
    // worker threads get large stacks
    rayon::ThreadPoolBuilder::new().stack_size(256 << 20).build_global().expect("thread pool");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    // Whole games start from a game created by the translation instead of PHP's tables
    let create = args.iter().any(|a| a == "--create");
    let only = args.iter().position(|a| a == "--only").and_then(|i| args.get(i + 1)).cloned();
    let dir = args.iter().find(|a| !a.starts_with('-') && Some(*a) != only.as_ref()).cloned().unwrap_or_else(|| "server/goldens".into());
    let mut files: Vec<PathBuf> = vec![];
    for board in std::fs::read_dir(&dir).expect("golden directory").flatten() {
        for f in std::fs::read_dir(board.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "json") && only.as_ref().is_none_or(|o| p.to_string_lossy().contains(o.as_str())) {
                files.push(p);
            }
        }
    }
    files.sort();
    let t = std::time::Instant::now();
    let results: Vec<(String, Report)> = files
        .par_iter()
        .map(|p| {
            let name = format!("{}/{}", p.parent().unwrap().file_name().unwrap().to_string_lossy(), p.file_stem().unwrap().to_string_lossy());
            let mut r = Report { nodes: 0, passed: 0, first_failure: None };
            // Whole games nest one node per step: no recursion limit
            let read = |b: Vec<u8>| -> Option<Value> {
                let mut de = serde_json::Deserializer::from_slice(&b);
                de.disable_recursion_limit();
                serde::Deserialize::deserialize(&mut de).ok()
            };
            let data: Value = match std::fs::read(p).ok().and_then(read) {
                Some(d) => d,
                None => {
                    r.first_failure = Some("unreadable file".into());
                    return (name, r);
                }
            };
            let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            if create && data["mode"] == json!("game") {
                // The game created by the translation (same game id, decks and dice) must have
                // the tables PHP had after V1::createGame, then play the same game
                let decks: Vec<String> = data["decks"].as_array().into_iter().flatten().filter_map(|d| d.as_str().map(String::from)).collect();
                let players: Vec<NewPlayer> = ["A", "B"].iter().enumerate().map(|(i, n)| NewPlayer { name: n.to_string(), bot: false, deck: decks.get(i).cloned() }).collect();
                match Game::create(data["phpGame"].as_i64().unwrap_or(0), &players, &dice) {
                    Ok(g) => {
                        let diffs = altered_rules::load::diff_tables(&data["db"], &g.bga_tables());
                        if diffs.is_empty() {
                            check_node(&g, &data["tree"], "", &mut r);
                        } else {
                            r.first_failure = Some(format!("created tables differ (php vs rust):\n    {}", diffs.iter().take(10).cloned().collect::<Vec<_>>().join("\n    ")));
                        }
                    }
                    Err(e) => r.first_failure = Some(format!("cannot create: {}", e)),
                }
                return (name, r);
            }
            match Game::from_bga_tables(&data["db"], &dice) {
                Ok(g) => check_node(&g, &data["tree"], "", &mut r),
                Err(e) => r.first_failure = Some(format!("cannot load: {}", e)),
            }
            (name, r)
        })
        .collect();
    let (mut ok_files, mut nodes, mut passed) = (0, 0, 0);
    let mut failures: Vec<&(String, Report)> = vec![];
    for res in &results {
        nodes += res.1.nodes;
        passed += res.1.passed;
        if res.1.first_failure.is_none() {
            ok_files += 1;
        } else {
            failures.push(res);
        }
    }
    for (name, r) in failures.iter().take(if verbose { usize::MAX } else { 20 }) {
        println!("FAIL {}: {}", name, r.first_failure.as_deref().unwrap_or(""));
    }
    println!(
        "\n{}/{} files pass, {}/{} nodes checked pass ({:.1}s)",
        ok_files,
        results.len(),
        passed,
        nodes,
        t.elapsed().as_secs_f64()
    );
}
