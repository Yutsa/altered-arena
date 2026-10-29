//! The effect actions (Gain, Loose, Exhaust, Ready, Tap, CompleteFeat, ActivateEffect,
//! CheckCondition, BoostExchange, MoveCard, InvokeToken) run through the engine on a recorded
//! position (AX_Common_TikTok: 2416 / 2417 are player 2001's characters, 2419 a Kappa in their
//! Reserve, 2420 their landmark; 2422 / 2423 are player 2002's characters).

use altered_rules::core::{engine, globals, node};
use altered_rules::managers::{cards, meeples};
use altered_rules::models::card;
use altered_rules::{php, Game, PhpError};
use serde_json::{json, Value};

fn tiktok() -> Option<Game> {
    let local = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens/rich/AX_Common_TikTok.json");
    let shared = "/Users/edouardwillisseck/dev/perso/altered/altered-arena/server/goldens/rich/AX_Common_TikTok.json";
    let bytes = std::fs::read(local).or_else(|_| std::fs::read(shared)).ok()?;
    let data: Value = serde_json::from_slice(&bytes).ok()?;
    let mut g = Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load");
    g.begin_request(Some(2001)).unwrap();
    Some(g)
}

/// Runs a flow of leaves for player 2001 from the root
fn run(g: &mut Game, childs: Value) {
    try_run(g, childs).unwrap()
}

fn try_run(g: &mut Game, childs: Value) -> Result<(), PhpError> {
    let flow = json!({"childs": childs});
    engine::setup(g, &flow, json!({"order": "assignment"}))?;
    engine::proceed(g, false, false)
}

/// Code of other modules still untranslated (the listeners reach card conditions)
fn untranslated(e: &PhpError) -> bool {
    format!("{:?}", e).contains("not translated")
}

fn count(g: &Game, card: i64, ty: &str) -> usize {
    meeples::count_meeples(g, &format!("card-{}", card), &json!(ty)).unwrap()
}

#[test]
fn gain_respects_up_to_then_loose_removes() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2416, "type": "boost", "n": 3, "upTo": 2}}]));
    assert_eq!(count(&g, 2416, "boost"), 2);
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2416, "type": "boost", "n": 1}}]));
    assert_eq!(count(&g, 2416, "boost"), 3);
    run(&mut g, json!([{"action": "Loose", "pId": 2001, "args": {"cardId": 2416, "type": "boost", "n": 2}}]));
    assert_eq!(count(&g, 2416, "boost"), 1);
}

#[test]
fn gain_fleeting_only_once() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2417, "type": "fleeting"}}]));
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2417, "type": "fleeting"}}]));
    assert_eq!(count(&g, 2417, "fleeting"), 1);
}

#[test]
fn augment_without_boost_increases_the_counter() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2417, "augment": true}}]));
    let data = card::get(&mut g, 2417, "extraDatas").unwrap();
    assert_eq!(data["counter"], json!(1));
    assert_eq!(count(&g, 2417, "boost"), 0);
}

#[test]
fn exhaust_then_ready() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "Exhaust", "pId": 2001, "args": {"cardId": 2419}}]));
    assert!(card::is_tapped(&g, 2419).unwrap());
    run(&mut g, json!([{"action": "Ready", "pId": 2001, "args": {"cardId": 2419}}]));
    assert!(!card::is_tapped(&g, 2419).unwrap());
}

#[test]
fn tap_pays_and_records_the_ability() {
    let Some(mut g) = tiktok() else { return };
    let ready = |g: &mut Game| altered_rules::models::player::get_mana(g, 2001).unwrap();
    let before = ready(&mut g);
    run(&mut g, json!([{"action": "Tap", "pId": 2001, "sourceId": 2420, "args": {"pay": 1}}]));
    assert!(card::is_tapped(&g, 2420).unwrap());
    assert_eq!(ready(&mut g), before - 1);
    let a = globals::get(&mut g, "abilityActivatedThisTurnTypeCount").unwrap();
    assert_eq!(a["2001"]["tap"], json!(1));
}

#[test]
fn complete_feat_once() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "CompleteFeat", "pId": 2001, "args": {"cardId": 2420}}]));
    run(&mut g, json!([{"action": "CompleteFeat", "pId": 2001, "sourceId": 2420, "args": {"cardId": "me"}}]));
    assert_eq!(count(&g, 2420, "featCompleted"), 1);
}

#[test]
fn boost_exchange_swaps() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "Gain", "pId": 2001, "args": {"cardId": 2416, "type": "boost", "n": 2}}]));
    run(&mut g, json!([{"action": "BoostExchange", "pId": 2001, "args": {"cardId": [2416, 2417]}}]));
    assert_eq!(count(&g, 2416, "boost"), 0);
    assert_eq!(count(&g, 2417, "boost"), 2);
}

#[test]
fn move_card_to_the_opposite_expedition() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "MoveCard", "pId": 2001, "sourceId": 2416, "args": {"cardId": 2423}}]));
    assert_eq!(card::location(&g, 2423).unwrap(), "stormLeft");
}

#[test]
fn invoke_token_in_a_single_location() {
    let Some(mut g) = tiktok() else { return };
    let before = cards::get_in_location(&g, "stormLeft", None).unwrap().len();
    if let Err(e) = try_run(&mut g, json!([{"action": "InvokeToken", "pId": 2001, "args": {"tokenType": "AX_Common_Brassbug", "targetLocation": ["stormLeft"]}}])) {
        assert!(untranslated(&e), "{:?}", e);
        return;
    }
    let after = cards::get_in_location(&g, "stormLeft", None).unwrap();
    assert_eq!(after.len(), before + 1);
    let t = *after.last().unwrap();
    assert!(card::is_token(&g, t).unwrap());
    assert_eq!(card::pid(&g, t).unwrap(), 2001);
    assert_eq!(card::get(&mut g, t, "name").unwrap(), json!("Brassbug"));
}

#[test]
fn check_condition_pushes_the_effect() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "CheckCondition", "pId": 2001, "sourceId": 2416, "args": {"effect": {"action": "Gain", "args": {"cardId": 2416, "type": "boost"}}}}]));
    assert_eq!(count(&g, 2416, "boost"), 1);
    let root = engine::root(&g).unwrap();
    let leaf = node::childs(&g, root)[0];
    assert_eq!(php::geti(&node::get_action_resolution_args(&g, leaf), 0), &json!("met"));
}

#[test]
fn activate_effect_of_a_card_without_effect_does_nothing() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!([{"action": "ActivateEffect", "pId": 2001, "args": {"cardId": 2419, "effectType": "Played"}}]));
    let root = engine::root(&g).unwrap();
    let leaf = node::childs(&g, root)[0];
    assert!(node::is_action_resolved(&g, leaf));
}
