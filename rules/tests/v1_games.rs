//! Games created by the arena (V1::createGame) against the PHP oracle: the reference runs are
//! produced by `tests/data/v1_games_php.php` in the server container. The translation creates
//! each game with the same game id (so the same seeded randomness), decks, bots and dice: the
//! tables right after creation must be PHP's, then both players' GameViews after every answer.

use altered_rules::arena::NewPlayer;
use altered_rules::Game;
use serde_json::{json, Map, Value};

fn data() -> Value {
    serde_json::from_str(include_str!("data/v1_games_php.json")).unwrap()
}

/// What is not compared: the log (texts made from the translation's notifications), the
/// sequence numbers (number of notifications) and the decision ids made from them
fn strip(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(m) = v.as_object_mut() {
        m.remove("log");
        m.remove("seq");
        if let Some(d) = m.get_mut("decision").and_then(Value::as_object_mut) {
            d.remove("id");
            // Choice labels are the engine's effect descriptions: only empty vs non-empty matters
            // to the rules (PHP_ENGINE.md §2.6), the translation's texts differ
            for c in d.get_mut("choices").and_then(Value::as_array_mut).into_iter().flatten() {
                let l = c["label"].as_str().is_some_and(|s| !s.is_empty());
                c["label"] = json!(l);
            }
        }
    }
    canonical(&v)
}

fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), canonical(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        Value::Number(n) => n.as_f64().filter(|f| f.fract() == 0.0).map(|f| json!(f as i64)).unwrap_or_else(|| v.clone()),
        _ => v.clone(),
    }
}

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                if let Some(d) = first_diff(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), &format!("{}.{}", path, k)) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| first_diff(p, q, &format!("{}[{}]", path, i)))
        }
        _ if a == b => None,
        _ => Some(format!("{}: php {} / rust {}", path, a, b)),
    }
}

#[test]
fn games_created_and_played_as_php() {
    // A request runs whole chains of states recursively: a large stack
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}

fn run() {
    for r in data()["runs"].as_array().unwrap() {
        let cfg = &r["config"];
        let players: Vec<NewPlayer> = cfg["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| NewPlayer { name: p["name"].as_str().unwrap_or("").to_string(), bot: p["bot"].as_bool().unwrap_or(false), deck: p["deck"].as_str().map(String::from) })
            .collect();
        let dice: Vec<i64> = cfg["dice"].as_array().unwrap().iter().filter_map(Value::as_i64).collect();
        let game_id = r["phpGame"].as_i64().unwrap();
        let mut g = Game::create(game_id, &players, &dice).unwrap();
        let diffs = altered_rules::load::diff_tables(&r["db"], &g.bga_tables());
        assert!(diffs.is_empty(), "game {}: created tables differ (php vs rust):\n{}", game_id, diffs.join("\n"));

        for (i, step) in r["steps"].as_array().unwrap().iter().enumerate() {
            for (pid, php_view) in step["views"].as_object().unwrap() {
                let pid: u32 = pid.parse().unwrap();
                let view = g.view(Some(pid), &game_id.to_string()).unwrap();
                if let Some(d) = first_diff(&strip(php_view), &strip(&view), "") {
                    panic!("game {} step {} view of {}: {}", game_id, i, pid, d);
                }
            }
            // The server saves games as JSON and reloads them: a reloaded game plays the same
            if i % 3 == 0 {
                let saved = serde_json::to_string(&g).unwrap();
                g = serde_json::from_str(&saved).unwrap();
            }
            let Some(p) = step["player"].as_u64() else { continue };
            let p = p as u32;
            let mut answer = step["answer"].clone();
            answer["decisionId"] = g.decision(p).unwrap()["id"].clone();
            match (g.answer(p, &answer), step.get("error")) {
                (Ok(()), None) => g.run_bots().unwrap(),
                (Err(_), Some(_)) => {}
                (res, err) => panic!("game {} step {}: rust {:?}, php error {:?}", game_id, i, res, err),
            }
        }
    }
}
