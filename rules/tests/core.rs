//! Core semantics: PHP values, and the engine loop on a recorded position

use altered_rules::core::{engine, globals, node};
use altered_rules::framework;
use altered_rules::php;
use altered_rules::Game;
use serde_json::{json, Value};

#[test]
fn php_loose_comparisons() {
    assert!(php::loose_eq(&json!("5"), &json!(5)));
    assert!(!php::loose_eq(&json!("abc"), &json!(0)));
    assert!(php::loose_eq(&Value::Null, &json!(0)));
    assert!(php::loose_eq(&Value::Null, &json!("")));
    assert!(php::loose_eq(&Value::Null, &json!([])));
    assert!(!php::loose_eq(&json!([99]), &json!(99)));
    assert!(php::loose_eq(&json!("1e1"), &json!("10")));
    assert!(php::in_array(&json!(2001), &json!(["2001"])));
    assert!(!php::truthy(&json!("0")));
    assert!(php::truthy(&json!("0.0")));
    assert_eq!(php::intval(&json!("12abc")), 12);
}

#[test]
fn php_arrays() {
    let mut a = json!([1, 2, 3]);
    php::unset(&mut a, "1");
    assert_eq!(a, json!({"0": 1, "2": 3}));
    php::push(&mut a, json!(4));
    assert_eq!(a, json!({"0": 1, "2": 3, "3": 4}));
    assert_eq!(php::values(&a), json!([1, 3, 4]));
    let mut b = json!({"0": "x", "1": "y"});
    php::normalize(&mut b);
    assert_eq!(b, json!(["x", "y"]));
    assert_eq!(php::array_merge(&[&json!({"a": 1, "5": 2}), &json!([3])]), json!({"a": 1, "0": 2, "1": 3}));
    assert_eq!(php::union(&json!([1, 2]), &json!([5, 6, 7])), json!([1, 2, 7]));
    let mut n = Value::Null;
    php::set(&mut n, "k", json!(1));
    assert_eq!(n, json!({"k": 1}));
}

fn tiktok() -> Option<Game> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens/rich/AX_Common_TikTok.json");
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    Some(Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load"))
}

#[test]
fn engine_passes_an_impossible_reaction_and_gives_the_turn_back() {
    let Some(mut g) = tiktok() else { return };
    assert_eq!(framework::state_id(&g), framework::ST_CHOOSE_ASSIGNMENT);
    g.begin_request(Some(2001)).unwrap();
    // A reaction of the hero to an event it does not listen to: not doable, auto-passed
    let flow = json!({"childs": [{"action": "ActivateCard", "pId": 2001, "args": {"cardId": 1, "event": {"type": "action", "action": "Foo", "method": "Foo", "pId": 2001}}}]});
    engine::setup(&mut g, &flow, json!({"order": "assignment"})).unwrap();
    engine::proceed(&mut g, false, false).unwrap();
    // The flow is over but nothing was played: a new CHOOSE_ASSIGNMENT is inserted at root
    assert_eq!(framework::state_id(&g), framework::ST_CHOOSE_ASSIGNMENT);
    let n = engine::get_next_unresolved(&mut g).unwrap().unwrap();
    assert_eq!(node::get_action(&g, n), json!("ChooseAssignment"));
    let root = engine::root(&g).unwrap();
    let leaf = node::childs(&g, root)[0];
    assert_eq!(node::get_resolution_args(&g, leaf), json!(99));
    assert_eq!(globals::get_int(&mut g, "engineChoices").unwrap(), 0);
}

#[test]
fn turn_decision_lists_the_playable_cards() {
    let Some(g) = tiktok() else { return };
    let d = g.decision(2001).expect("decision");
    assert_eq!(d["kind"], json!("turn"));
    let play: Vec<i64> = d["play"].as_array().unwrap().iter().map(|p| p["cardId"].as_i64().unwrap()).collect();
    assert_eq!(play, vec![35, 36, 40, 2401, 2418, 2419]);
    assert_eq!(g.waiting_for(), vec![2001]);
}
