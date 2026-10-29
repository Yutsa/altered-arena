//! Replays a whole-game golden file (server/goldens/games/SEED.json, one `next` per node) in the
//! Rust translation and prints the first divergence in detail: the step, the last answers, both
//! decisions with their ids, the zones that differ (with card refs) and the Rust notifications of
//! the last answers.
//!
//!   cargo run --release -p goldens --example game_step -- server/goldens/games/12.json [--notifs N]

use altered_rules::Game;
use serde_json::{json, Map, Value};

fn compact_state(state: &Value) -> Map<String, Value> {
    let mut cards: Vec<&Value> = state["cards"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    cards.sort_by_key(|c| (c["order"].as_i64().unwrap_or(0), c["id"].as_i64().unwrap_or(0)));
    let mut out = Map::new();
    let mut zones: Map<String, Value> = Map::new();
    for c in &cards {
        let key = format!("{}:{}", c["owner"], c["zone"].as_str().unwrap_or(""));
        zones.entry(key).or_insert_with(|| json!([])).as_array_mut().unwrap().push(c["id"].clone());
    }
    for (k, ids) in zones.iter_mut() {
        if !k.ends_with(":deck") {
            ids.as_array_mut().unwrap().sort_by_key(|v| v.as_i64().unwrap_or(0));
        }
        out.insert(format!("zones.{}", k), ids.clone());
    }
    for c in &cards {
        let zone = c["zone"].as_str().unwrap_or("");
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
            out.insert(format!("marks.{}", c["id"]), Value::Object(m));
        }
    }
    for p in state["players"].as_array().into_iter().flatten() {
        out.insert(format!("positions.{}", p["id"]), p["position"].clone());
    }
    for k in ["day", "phase", "first"] {
        out.insert(k.into(), state[k].clone());
    }
    let storm: Vec<Value> = state["storm"].as_array().into_iter().flatten().map(|t| json!(t["revealed"].as_bool().unwrap_or(false) as i32)).collect();
    out.insert("storm".into(), json!(storm));
    out
}

/// The PHP compact state flattened the same way
fn flatten_php(s: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in s["zones"].as_object().into_iter().flatten() {
        out.insert(format!("zones.{}", k), v.clone());
    }
    for (k, v) in s["marks"].as_object().into_iter().flatten() {
        out.insert(format!("marks.{}", k), v.clone());
    }
    for (k, v) in s["positions"].as_object().into_iter().flatten() {
        out.insert(format!("positions.{}", k), v.clone());
    }
    for k in ["day", "phase", "first", "storm"] {
        out.insert(k.into(), s[k].clone());
    }
    out
}

fn norm(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().filter(|k| *k != "id" && *k != "prompt").collect();
            keys.sort();
            let inner: Vec<String> = keys.iter().map(|k| format!("{:?}:{}", k, if *k == "label" { json!(m[*k].as_str().is_some_and(|s| !s.is_empty())).to_string() } else { norm(&m[*k]) })).collect();
            format!("{{{}}}", inner.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(norm).collect::<Vec<_>>().join(",")),
        Value::String(s) => s.parse::<i64>().map(|n| n.to_string()).unwrap_or_else(|_| v.to_string()),
        _ => v.to_string(),
    }
}

/// Sorted keys, numeric strings as numbers (PHP loose types)
fn canon(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            format!("{{{}}}", keys.iter().map(|k| format!("{:?}:{}", k, canon(&m[*k]))).collect::<Vec<_>>().join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        Value::String(s) => s.parse::<i64>().map(|n| n.to_string()).unwrap_or_else(|_| v.to_string()),
        Value::Bool(b) => (*b as i32).to_string(),
        _ => v.to_string(),
    }
}

/// Differences between the replayed game and the PHP tables dumped at the same step
/// (explore.php game SEED --dump=STEP): globals (with the engine tree as Rust holds it),
/// card rows (location, state) and meeples
fn diff_tables(g: &mut Game, t: &Value) {
    println!("--- tables diff (php vs rust)");
    if let Some(r) = g.tree.root {
        let tree = altered_rules::core::node::to_array(g, r);
        g.globals.insert("engine".into(), tree);
    }
    for row in t["global_variables"].as_array().into_iter().flatten() {
        let name = row["name"].as_str().unwrap_or("");
        let php: Value = serde_json::from_str(row["value"].as_str().unwrap_or("null")).unwrap_or(Value::Null);
        let rust = g.globals.get(name).cloned().unwrap_or(Value::Null);
        if canon(&php) != canon(&rust) {
            println!("  global {}\n    php  {}\n    rust {}", name, php, rust);
        }
    }
    for row in t["cards"].as_array().into_iter().flatten() {
        let id = row["card_id"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let php = format!("{} {}", row["card_location"].as_str().unwrap_or(""), row["card_state"].as_str().unwrap_or(""));
        let rust = g.cards.get(&id).map(|c| format!("{} {}", c.location, c.state)).unwrap_or("-".into());
        if php != rust {
            println!("  card {}: php {} rust {}", id, php, rust);
        }
    }
    let mut seen = std::collections::HashSet::new();
    for row in t["meeples"].as_array().into_iter().flatten() {
        let id = row["meeple_id"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        seen.insert(id);
        let php = format!("{} {} {} {}", row["type"].as_str().unwrap_or(""), row["meeple_location"].as_str().unwrap_or(""), row["meeple_state"].as_str().unwrap_or(""), row["player_id"].as_str().unwrap_or(""));
        let rust = g.meeples.get(&id).map(|m| format!("{} {} {} {}", m.kind, m.location, m.state, m.pid)).unwrap_or("-".into());
        if php != rust {
            println!("  meeple {}: php {} rust {}", id, php, rust);
        }
    }
    for (id, m) in &g.meeples {
        if !seen.contains(id) {
            println!("  meeple {}: php - rust {} {} {}", id, m.kind, m.location, m.state);
        }
    }
    println!("---");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.iter().find(|a| !a.starts_with('-') && a.ends_with(".json")).expect("game file");
    let n_notifs: usize = args.iter().position(|a| a == "--notifs").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(3);
    let bytes = std::fs::read(path).expect("file");
    let mut de = serde_json::Deserializer::from_slice(&bytes);
    de.disable_recursion_limit();
    let data: Value = serde::Deserialize::deserialize(&mut de).expect("json");
    let refs: Map<String, Value> = data["initial"]["cards"].as_array().into_iter().flatten().map(|c| (c["id"].to_string(), c["ref"].clone())).collect();
    let rf = |id: &Value| refs.get(&id.to_string()).and_then(|r| r.as_str()).unwrap_or("?").to_string();
    let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    let mut g = Game::from_bga_tables(&data["db"], &dice).expect("load");
    println!("php game {}, decks {}, random events {}", data["phpGame"], data["decks"], data["random"]);
    let mut node = &data["tree"];
    let mut history: Vec<(String, Vec<Value>)> = vec![];
    let mut step = 0;
    let tables_at: Option<(usize, Value)> = args.iter().position(|a| a == "--tables").map(|i| {
        let f = &args[i + 1];
        let at = f.rsplit(".step").next().and_then(|s| s.trim_end_matches(".json").parse().ok()).expect("tables file name: SEED.stepN.json");
        (at, serde_json::from_slice(&std::fs::read(f).expect("tables file")).expect("tables json"))
    });
    loop {
        if let Some((at, t)) = &tables_at {
            if *at == step {
                diff_tables(&mut g, t);
            }
        }
        let php = flatten_php(&node["state"]);
        let rust = compact_state(&g.oracle_state());
        let mut problems = vec![];
        let mut keys: Vec<&String> = php.keys().chain(rust.keys()).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            let (a, b) = (php.get(k).map(norm), rust.get(k).map(norm));
            if a != b {
                let names = |v: Option<&Value>| -> String {
                    match v {
                        Some(Value::Array(ids)) if k.starts_with("zones.") => ids.iter().map(|i| format!("{}={}", i, rf(i))).collect::<Vec<_>>().join(" "),
                        Some(v) => v.to_string(),
                        None => "-".into(),
                    }
                };
                let label = if let Some(id) = k.strip_prefix("marks.") { format!("{} ({})", k, rf(&json!(id.parse::<i64>().unwrap_or(0)))) } else { k.clone() };
                problems.push(format!("  {}\n    php  {}\n    rust {}", label, names(php.get(k)), names(rust.get(k))));
            }
        }
        let player = node["player"].as_u64().map(|p| p as u32);
        if let Some(p) = player {
            if g.waiting_for().first() != Some(&p) {
                problems.push(format!("  waiting for: php {} rust {:?}", p, g.waiting_for()));
            }
            let rd = g.try_decision(p);
            let rd_s = match &rd {
                Ok(Some(d)) => norm(d),
                Ok(None) => "null".into(),
                Err(e) => format!("error {}", e),
            };
            if norm(&node["decision"]) != rd_s {
                problems.push(format!("  decision\n    php  {}\n    rust {}", node["decision"], rd.map(|d| d.map(|d| d.to_string()).unwrap_or_default()).unwrap_or_else(|e| e.to_string())));
            }
        }
        if !problems.is_empty() {
            println!("\nDIVERGENCE before step {} (day {} phase {})", step, node["state"]["day"], node["state"]["phase"]);
            for (a, notifs) in history.iter().rev().take(n_notifs).rev() {
                println!("  answered {}", a);
                for n in notifs {
                    println!("      {} {}", n["name"].as_str().unwrap_or(""), n["args"].to_string().chars().take(300).collect::<String>());
                }
            }
            println!("{}", problems.join("\n"));
            return;
        }
        let Some(p) = player else {
            println!("all {} steps match ({})", step, node["end"]);
            return;
        };
        let Some(next) = node["next"].as_array() else {
            println!("all {} steps match ({})", step, node["end"]);
            return;
        };
        for n in next {
            let mut answer = n["answer"].clone();
            if let Some(id) = g.decision(p).and_then(|d| d.get("id").cloned()) {
                answer["decisionId"] = id;
            }
            if n.get("node").is_none() {
                let mut c = g.clone();
                if c.answer(p, &answer).is_ok() {
                    println!("step {}: rust accepted {} that php refused ({})", step, n["answer"], n["error"]);
                    return;
                }
                continue;
            }
            let label = format!("{}: player {} {}", step, p, n["answer"]);
            if let Err(e) = g.answer(p, &answer) {
                println!("step {}: rust refused {}: {}", step, n["answer"], e);
                return;
            }
            history.push((label, g.notifications.clone()));
            node = &n["node"];
        }
        step += 1;
    }
}
