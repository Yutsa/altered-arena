//! Replays the golden decision trees recorded with the PHP engine (server/tools/explore.php) against
//! the Rust translation: at every node, the player waited for, the decision asked and the state must
//! be the same; then every recorded answer is applied to a copy of the game.
//!
//!   cargo run --release -p goldens -- [DIR] [--only SUBSTRING] [--shard I/N] [-v] [--create]
//!
//! DIR defaults to server/goldens. Prints the first difference of each failing file and a summary.
//! --shard I/N: only the files I, I+N, I+2N... of the sorted list (I from 1), for parallel CI jobs.
//! --create: whole games start from a game created by the translation (Game::create with the
//! recorded game id, decks and dice), whose tables must be those PHP dumped after V1::createGame.

use altered_rules::arena::NewPlayer;
use altered_rules::game::ShuffleHook;
use altered_rules::Game;
use rayon::prelude::*;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

/// Fields the arena added to decisions after the files were recorded, for the client's display
/// only (`toMana`: the picked cards go to the Mana zone face down; `markRegion`: the regions and
/// markers a button choice is about). They don't exist in PHP and change no rule.
const CLIENT_HINTS: &[&str] = &["toMana", "markRegion"];

/// Texts don't have to match (PHP_ENGINE.md §2.6: only empty vs non-empty descriptions matter);
/// client hints are left out
fn strip_texts(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, x) in m {
                match k.as_str() {
                    "id" | "prompt" => {}
                    k if CLIENT_HINTS.contains(&k) => {}
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

/// Unique cards (server/tools/uniques.php goldens): Cards::generateUnique on each API content must
/// give the same properties, in the same order, or fail as PHP did
fn check_uniques(cases: &Value, r: &mut Report) {
    for (i, case) in cases.as_array().into_iter().flatten().enumerate() {
        r.nodes += 1;
        let content = &case["content"];
        let got = altered_rules::managers::cards::generate_unique(content).map(|mut v| {
            altered_rules::php::normalize(&mut v);
            v
        });
        let ok = match (&got, case.get("error")) {
            (Err(_), Some(_)) => true,
            (Ok(v), None) => serde_json::to_string(v).ok() == serde_json::to_string(&case["out"]).ok(),
            _ => false,
        };
        if ok {
            r.passed += 1;
        } else if r.first_failure.is_none() {
            let got = match got {
                Ok(v) => v.to_string(),
                Err(e) => format!("error {}", e),
            };
            let want = case.get("error").map(|e| format!("error {}", e)).unwrap_or_else(|| case["out"].to_string());
            r.first_failure = Some(format!("case {} {}:\n    php:  {}\n    rust: {}", i, content["uniqueReduced"], want, got));
        }
    }
}

/// Card references (server/tools/api_decks.php cardclass): Cards::getCardClass must give the same
/// class and properties (in the same order) and the same art getters, or fail as PHP did
fn check_card_classes(cases: &Value, r: &mut Report) {
    for case in cases.as_array().into_iter().flatten() {
        r.nodes += 1;
        let reference = case["ref"].as_str().unwrap_or("");
        let got = altered_rules::managers::cards::get_card_class(reference).map(|c| {
            let mut props = Value::Object(c.props.clone());
            altered_rules::php::normalize(&mut props);
            json!({"ref": reference, "class": c.class, "props": props, "token": c.is_token(), "asset": c.get_asset(), "mainAsset": c.get_main_asset(), "fullArt": c.get_full_art()})
        });
        let ok = match (&got, case.get("error")) {
            (Err(_), Some(_)) => true,
            (Ok(v), None) => serde_json::to_string(v).ok() == serde_json::to_string(case).ok(),
            _ => false,
        };
        if ok {
            r.passed += 1;
        } else if r.first_failure.is_none() {
            let got = match got {
                Ok(v) => v.to_string(),
                Err(e) => format!("error {}", e),
            };
            r.first_failure = Some(format!("{}:\n    php:  {}\n    rust: {}", reference, case, got));
        }
    }
}

/// A game created with a Reunion deck (server/tools/api_decks.php decks): the tables after
/// V1::createGame must be PHP's, or the creation must fail as in PHP
fn check_api_deck(data: &Value, r: &mut Report) {
    r.nodes += 1;
    let players: Vec<NewPlayer> = data["players"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| NewPlayer {
            name: p["name"].as_str().unwrap_or("").to_string(),
            bot: false,
            deck: p["deck"].as_str().map(String::from),
            api_deck: None,
            reunion_deck: p.get("reunionDeck").cloned(),
        })
        .collect();
    let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    match (Game::create(data["phpGame"].as_i64().unwrap_or(0), &players, &dice), data.get("error")) {
        (Err(_), Some(_)) => r.passed += 1,
        (Ok(g), None) => {
            let diffs = altered_rules::load::diff_tables(&data["db"], &g.bga_tables());
            if diffs.is_empty() {
                r.passed += 1;
            } else {
                r.first_failure =
                    Some(format!("created tables differ (php vs rust):\n    {}", diffs.iter().take(10).cloned().collect::<Vec<_>>().join("\n    ")));
            }
        }
        (Ok(_), Some(e)) => r.first_failure = Some(format!("php refused the deck ({}), rust created the game", e)),
        (Err(e), None) => r.first_failure = Some(format!("cannot create: {}", e)),
    }
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
        // Replays name every player waited for (in a multiactive state, the one who answers may be
        // any of them); the other trees, the first one
        if let Some(w) = node.get("waiting") {
            let php_w: Vec<u32> = w.as_array().into_iter().flatten().filter_map(Value::as_u64).map(|x| x as u32).collect();
            if game.waiting_for() != php_w || !php_w.contains(&p) {
                problems.push(format!("waiting for: php {:?} (answered by {}) rust {:?}", php_w, p, game.waiting_for()));
            }
        } else if game.waiting_for().first() != Some(&p) {
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
        set_next_ids(&mut g, node);
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

/// Replays: ids that BGA consumed in requests an undo erased from its log, set before answering
/// (the AUTO_INCREMENT of `cards` / `meeples`)
fn set_next_ids(g: &mut Game, node: &Value) {
    if let Some(id) = node["nextIds"]["cards"].as_i64() {
        g.next_card_id = id;
    }
    if let Some(id) = node["nextIds"]["meeples"].as_i64() {
        g.next_meeple_id = id;
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
    let shard = args.iter().position(|a| a == "--shard").and_then(|i| args.get(i + 1)).cloned();
    let (shard_i, shard_n) = match &shard {
        Some(s) => {
            let (i, n) = s.split_once('/').and_then(|(i, n)| Some((i.parse::<usize>().ok()?, n.parse::<usize>().ok()?))).expect("--shard I/N");
            assert!(i >= 1 && i <= n, "--shard I/N: 1 <= I <= N");
            (i - 1, n)
        }
        None => (0, 1),
    };
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-') && Some(*a) != only.as_ref() && Some(*a) != shard.as_ref())
        .cloned()
        .unwrap_or_else(|| "server/goldens".into());
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
    let files: Vec<PathBuf> = files.into_iter().enumerate().filter(|(k, _)| k % shard_n == shard_i).map(|(_, p)| p).collect();
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
            if data["mode"] == json!("uniques") {
                check_uniques(&data["cases"], &mut r);
                return (name, r);
            }
            if data["mode"] == json!("cardclass") {
                check_card_classes(&data["cases"], &mut r);
                return (name, r);
            }
            if data["mode"] == json!("apideck") {
                check_api_deck(&data, &mut r);
                return (name, r);
            }
            let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            if create && data["mode"] == json!("game") {
                // The game created by the translation (same game id, decks and dice) must have
                // the tables PHP had after V1::createGame, then play the same game
                let decks: Vec<String> = data["decks"].as_array().into_iter().flatten().filter_map(|d| d.as_str().map(String::from)).collect();
                // Custom decks (explore.php deckgame): the API deck contents
                let api: Vec<Value> = data["apiDecks"].as_array().cloned().unwrap_or_default();
                let players: Vec<NewPlayer> = ["A", "B"]
                    .iter()
                    .enumerate()
                    .map(|(i, n)| match api.get(i) {
                        Some(a) => NewPlayer { name: n.to_string(), bot: false, deck: None, api_deck: Some(a.clone()), reunion_deck: None },
                        None => NewPlayer { name: n.to_string(), bot: false, deck: decks.get(i).cloned(), api_deck: None, reunion_deck: None },
                    })
                    .collect();
                match Game::create(data["phpGame"].as_i64().unwrap_or(0), &players, &dice) {
                    Ok(g) => {
                        let diffs = altered_rules::load::diff_tables(&data["db"], &g.bga_tables());
                        if diffs.is_empty() {
                            check_node(&g, &data["tree"], "", &mut r);
                        } else {
                            r.first_failure = Some(format!(
                                "created tables differ (php vs rust):\n    {}",
                                diffs.iter().take(10).cloned().collect::<Vec<_>>().join("\n    ")
                            ));
                        }
                    }
                    Err(e) => r.first_failure = Some(format!("cannot create: {}", e)),
                }
                return (name, r);
            }
            match Game::from_bga_tables(&data["db"], &dice) {
                Ok(mut g) => {
                    // Replays: the deck shuffles BGA made, in order
                    if let Some(orders) = data["injectShuffles"].as_array() {
                        g.shuffle_hook =
                            ShuffleHook::Injected(orders.iter().map(|o| o.as_array().into_iter().flatten().filter_map(Value::as_i64).collect()).collect());
                    }
                    check_node(&g, &data["tree"], "", &mut r)
                }
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
    let known = known_failures();
    let (expected, unexpected): (Vec<&(String, Report)>, Vec<_>) = failures.into_iter().partition(|res| known.contains_key(&res.0));
    for (name, r) in unexpected.iter().take(if verbose { usize::MAX } else { 20 }) {
        println!("FAIL {}: {}", name, r.first_failure.as_deref().unwrap_or(""));
    }
    for (name, r) in expected.iter().filter(|_| verbose) {
        println!("KNOWN {} ({}): {}", name, known[name], r.first_failure.as_deref().unwrap_or(""));
    }
    // A known failure that passes again must leave the list, so that it cannot regress unnoticed
    let fixed: Vec<&String> = results.iter().filter(|(name, r)| r.first_failure.is_none() && known.contains_key(name)).map(|(name, _)| name).collect();
    for name in &fixed {
        println!("FIXED {}: passes now, remove it from goldens/known_failures.txt", name);
    }
    println!(
        "\n{}/{} files pass ({} known failures), {}/{} nodes checked pass ({:.1}s)",
        ok_files,
        results.len(),
        expected.len(),
        passed,
        nodes,
        t.elapsed().as_secs_f64()
    );
    if results.is_empty() {
        println!("no golden file in {}", dir);
    }
    if results.is_empty() || !unexpected.is_empty() || !fixed.is_empty() {
        std::process::exit(1);
    }
}

/// goldens/known_failures.txt: `board/file  reason` per line (`#` comments). These files may fail
/// without failing the run; any other failure fails it.
fn known_failures() -> std::collections::HashMap<String, String> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/known_failures.txt")).unwrap_or_default();
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once(char::is_whitespace) {
            Some((name, reason)) => (name.to_string(), reason.trim().to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect()
}
