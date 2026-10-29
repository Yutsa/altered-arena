//! Expeditions, Dusk and Night: Collection::sortBy order, storm computations, the storm actions.

use altered_rules::core::{engine, globals};
use altered_rules::framework;
use altered_rules::helpers::collection;
use altered_rules::states::turn;
use altered_rules::managers::{cards, meeples, players};
use altered_rules::models::{card, player};
use altered_rules::php;
use altered_rules::Game;
use serde_json::{json, Value};

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

/// `json_decode(json_encode($v), true)`, with the keys sorted (key order is not compared)
fn norm(v: &Value) -> Value {
    fn sort(v: Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut items: Vec<(String, Value)> = m.into_iter().collect();
                items.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Object(items.into_iter().map(|(k, x)| (k, sort(x))).collect())
            }
            Value::Array(a) => Value::Array(a.into_iter().map(sort).collect()),
            x => x,
        }
    }
    sort(php::round_trip(v))
}

#[track_caller]
fn same(scenario: &str, what: &str, rust: &Value, php_v: &Value) {
    assert_eq!(norm(rust), norm(php_v), "{}: {}", scenario, what);
}

/// The cards and meeples tables as the reference script dumps them
fn tables(g: &Game) -> (Value, Value) {
    let mut ids: Vec<i64> = g.cards.keys().copied().collect();
    ids.sort();
    let cs: Vec<Value> = ids
        .iter()
        .map(|id| {
            let c = &g.cards[id];
            json!([c.id, c.location, c.state, c.pid, c.db_props.get("tapped").cloned().unwrap_or(Value::Null)])
        })
        .collect();
    let mut mids: Vec<i64> = g.meeples.keys().copied().collect();
    mids.sort();
    let ms: Vec<Value> = mids
        .iter()
        .map(|id| {
            let m = &g.meeples[id];
            json!([m.id, m.location, m.state, m.kind, m.pid])
        })
        .collect();
    (Value::Array(cs), Value::Array(ms))
}

/// Storm computations, Dusk moves and Night cleanup on positions with storm powers, against the
/// original engine (`tests/data/storm_php.php`, run in the server container)
#[test]
fn storm_matches_php() {
    let data: Value = serde_json::from_str(include_str!("data/storm_php.json")).unwrap();
    for (name, sc) in data.as_object().unwrap() {
        let mut g = Game::from_bga_tables(&sc["db"], &[3, 1, 6, 4, 2, 5]).expect("load");
        let initial = g.clone();
        let v = &sc["values"];

        g.begin_request(None).unwrap();
        same(name, "biomeTotals", &players::get_biome_totals(&mut g).unwrap(), &v["biomeTotals"]);
        let mut strength = json!({});
        let mut biomes = json!({});
        for p in players::get_all(&g) {
            php::set(&mut strength, &p.to_string(), player::get_biome_strength(&mut g, p, &STORMS, false).unwrap());
            let mut b = json!({});
            for s in STORMS {
                php::set(&mut b, s, player::get_biomes(&mut g, p, s, false).unwrap());
            }
            php::set(&mut biomes, &p.to_string(), b);
        }
        same(name, "strengthNoModifiers", &strength, &v["strengthNoModifiers"]);
        same(name, "movements", &players::compute_storm(&mut g, false).unwrap(), &v["movements"]);
        same(name, "blocked", &players::get_blocked_expeditions(&mut g).unwrap(), &v["blocked"]);
        same(name, "defenders", &players::get_defenders(&mut g, false).unwrap(), &v["defenders"]);
        same(name, "regions", &players::get_regions_info(&mut g).unwrap(), &v["regions"]);
        same(name, "winning", &players::get_winning_player_by_storms(&mut g).unwrap(), &v["winning"]);
        same(name, "actionInstead", &players::get_action_instead_of_advance(&mut g).unwrap(), &v["actionInstead"]);
        let mut is_defender = json!({});
        for c in cards::get_played_cards(&mut g, &Value::Null, &Value::Null).unwrap() {
            php::set(&mut is_defender, &c.to_string(), json!(card::is_defender(&mut g, c).unwrap()));
        }
        same(name, "isDefender", &is_defender, &v["isDefender"]);
        same(name, "biomes", &biomes, &v["biomes"]);

        // Dusk
        let d = &sc["dusk"];
        g.begin_request(None).unwrap();
        engine::setup(&mut g, &json!({"type": "seq", "childs": []}), json!({"order": "advancePhase"})).unwrap();
        let movements = players::compute_storm(&mut g, true).unwrap();
        same(name, "dusk movements", &movements, &d["movements"]);
        same(name, "dusk stormMoves", &globals::get(&mut g, "stormMoves").unwrap(), &d["stormMoves"]);
        same(name, "dusk storm", &globals::get(&mut g, "storm").unwrap(), &d["storm"]);
        same(name, "dusk engine", &globals::get(&mut g, "engine").unwrap(), &d["engine"]);
        let (cs, ms) = tables(&g);
        same(name, "dusk cards", &cs, &d["cards"]);
        same(name, "dusk meeples", &ms, &d["meeples"]);

        // Night
        let n = &sc["night"];
        g.begin_request(None).unwrap();
        engine::setup(&mut g, &json!({"type": "seq", "childs": []}), json!({"order": "nightCleanup"})).unwrap();
        let mut returns = json!({});
        for p in players::get_turn_order(&mut g, None).unwrap() {
            php::set(&mut returns, &p.to_string(), player::night_cleanup(&mut g, p).unwrap());
        }
        meeples::night_cleanup(&mut g).unwrap();
        same(name, "night returns", &returns, &n["returns"]);
        same(name, "night engine", &globals::get(&mut g, "engine").unwrap(), &n["engine"]);
        let (cs, ms) = tables(&g);
        same(name, "night cards", &cs, &n["cards"]);
        same(name, "night meeples", &ms, &n["meeples"]);

        // stAdvanceDusk on the position (the tie-breaker branch decides the game)
        let a = &sc["advance"];
        let mut g = initial;
        g.begin_request(None).unwrap();
        let r = turn::st_advance_dusk(&mut g);
        assert!(r.is_ok() == a["error"].is_null(), "{}: stAdvanceDusk {:?} / php {}", name, r, a["error"]);
        let (cs, ms) = tables(&g);
        same(name, "advance cards", &cs, &a["cards"]);
        same(name, "advance meeples", &ms, &a["meeples"]);
        let mut scores = json!({});
        for (pid, p) in &g.players {
            php::set(&mut scores, &pid.to_string(), json!(p.score.to_string()));
        }
        same(name, "advance scores", &scores, &a["scores"]);
        assert_eq!(framework::state_id(&g).to_string(), php::strval(&a["state"]), "{}: advance state", name);
    }
}

/// `Collection::sortBy` (uasort with a bool comparator) gives PHP 8.2's order: reference cases
/// produced by `tests/data/sortby_php82.php` in the server container
#[test]
fn sort_by_matches_php82() {
    let data: Value = serde_json::from_str(include_str!("data/sortby_php82.json")).unwrap();
    for case in data.as_array().unwrap() {
        let keys: Vec<String> = case["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap().to_string()).collect();
        let ids: Vec<i64> = (0..keys.len() as i64).map(|i| 100 + i).collect();
        let expected: Vec<i64> = case["order"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        assert_eq!(collection::sort_by(&ids, &keys), expected, "keys {:?}", keys);
    }
}

/// Texts don't have to match (as in the golden runner): no id, no prompt, labels empty or not
fn strip_texts(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = serde_json::Map::new();
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

/// Cards (with tapped and extraDatas), meeples and the expedition globals, as the reference
/// script dumps them
fn action_tables(g: &mut Game) -> Value {
    let mut ids: Vec<i64> = g.cards.keys().copied().collect();
    ids.sort();
    let cs: Vec<Value> = ids
        .iter()
        .map(|id| {
            let c = &g.cards[id];
            let p = |k: &str| c.db_props.get(k).cloned().unwrap_or(Value::Null);
            json!([c.id, c.location, c.state, c.pid, p("tapped"), p("extraDatas")])
        })
        .collect();
    let (_, ms) = tables(g);
    let mut gl = json!({});
    for k in ["blockedExpeditions", "expeditionMoves", "stormMoves", "diceRolls"] {
        // rows created on first use only
        if g.globals.contains_key(k) {
            php::set(&mut gl, k, globals::get(g, k).unwrap());
        }
    }
    json!({"cards": cs, "meeples": ms, "globals": gl})
}

/// The expedition actions run from a flow, then answered as the original engine was
/// (`tests/data/storm_actions_php.php`)
#[test]
fn storm_actions_match_php() {
    let data: Value = serde_json::from_str(include_str!("data/storm_actions_php.json")).unwrap();
    let initial = Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load");
    for (name, case) in data["cases"].as_object().unwrap() {
        let mut g = initial.clone();
        let steps = case["steps"].as_array().unwrap();
        g.begin_request(None).unwrap();
        let r = engine::setup(&mut g, &json!({"type": "seq", "childs": [case["flow"]]}), json!({"order": "assignment"}))
            .and_then(|_| engine::proceed(&mut g, false, false));
        assert_eq!(r.is_ok(), steps[0]["error"].is_null(), "{}: setup {:?}", name, r);
        same(name, "after setup", &action_tables(&mut g), &steps[0]["tables"]);
        for (i, step) in steps.iter().enumerate().skip(1) {
            let p = g.waiting_for()[0];
            assert_eq!(json!(p), step["player"], "{} #{}: player", name, i);
            let d = g.try_decision(p).unwrap_or_else(|e| panic!("{} #{}: decision: {:?}", name, i, e)).unwrap_or(Value::Null);
            assert_eq!(strip_texts(&d), strip_texts(&step["decision"]), "{} #{}: decision", name, i);
            if step["answer"].is_null() {
                break;
            }
            let mut answer = step["answer"].clone();
            php::set(&mut answer, "decisionId", d["id"].clone());
            let r = g.answer(p, &answer);
            assert_eq!(r.is_ok(), step["error"].is_null(), "{} #{}: answer {:?}", name, i, r);
            same(name, &format!("tables #{}", i), &action_tables(&mut g), &step["tables"]);
        }
    }
}
