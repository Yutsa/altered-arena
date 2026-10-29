//! SpecialEffect cases run through the engine on a recorded position

use altered_rules::core::{engine, globals};
use altered_rules::helpers::{conditions_extra, utils};
use altered_rules::Game;
use serde_json::{json, Value};

fn tiktok() -> Option<Game> {
    // The goldens are not in git: ALTERED_GOLDENS points to them from a worktree
    let dir = std::env::var("ALTERED_GOLDENS").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens").to_string());
    let data: Value = serde_json::from_slice(&std::fs::read(format!("{}/rich/AX_Common_TikTok.json", dir)).ok()?).ok()?;
    Some(Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load"))
}

/// Runs one SpecialEffect node whose source is card 2401 (in the hand of player 2001)
fn run(g: &mut Game, effect: &str, args: Value) {
    g.begin_request(Some(2001)).unwrap();
    let flow = json!({"childs": [{"action": "SpecialEffect", "pId": 2001, "sourceId": 2401, "args": {"effect": effect, "args": args}}]});
    engine::setup(g, &flow, json!({"order": "assignment"})).unwrap();
    engine::proceed(g, false, false).unwrap();
}

#[test]
fn next_character_boost_flags() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, "nextCharacterGains2Boost", json!([]));
    assert_eq!(globals::get_int(&mut g, "nextCharacterBoost").unwrap(), 2);
    assert_eq!(globals::get_int(&mut g, "nextCharacterBoostOccurence").unwrap(), 1);
}

#[test]
fn cost_reduction_accumulates_per_bucket() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, "costReduction", json!({"type": "character", "reduction": 1}));
    run(&mut g, "costReduction", json!({"type": "character", "reduction": 2, "permanent": true}));
    let r = globals::get(&mut g, "costReduction").unwrap();
    assert_eq!(r["2001"]["character"], json!({"reduction": 3, "permanent": true}));
}

#[test]
fn add_to_current_rolls_sorts_and_dedupes() {
    let Some(mut g) = tiktok() else { return };
    globals::set(&mut g, "diceRolls", json!([2, 3])).unwrap();
    run(&mut g, "addToCurrentRolls", json!({"n": 1}));
    assert_eq!(globals::get(&mut g, "diceRolls").unwrap(), json!([2, 3, 4]));
}

#[test]
fn missing_globals_variable_throws() {
    let Some(mut g) = tiktok() else { return };
    g.begin_request(Some(2001)).unwrap();
    let flow = json!({"childs": [{"action": "SpecialEffect", "pId": 2001, "sourceId": 2401, "args": {"effect": "removeFleetingIfPlayedHand"}}]});
    engine::setup(&mut g, &flow, json!({"order": "assignment"})).unwrap();
    let e = engine::proceed(&mut g, false, false).unwrap_err();
    assert!(e.0.contains("removeFleetingIfPlayedHand"), "{}", e.0);
}

#[test]
fn cost_reduction_bucket_overlap() {
    assert!(utils::cost_reduction_buckets_overlap(&json!("all"), &json!("spell")));
    assert!(utils::cost_reduction_buckets_overlap(&json!("character"), &json!("artist")));
    assert!(utils::cost_reduction_buckets_overlap(&json!("animal"), &json!("spirit")));
    assert!(!utils::cost_reduction_buckets_overlap(&json!("spell"), &json!("artist")));
    assert!(utils::cost_reduction_buckets_overlap(&json!("feat"), &json!("character")));
}

#[test]
fn card_search_name_matches_whole_segments() {
    let Some(mut g) = tiktok() else { return };
    let uid = altered_rules::models::card::get_str(&mut g, 2401, "uid").unwrap();
    let parts: Vec<&str> = uid.split('_').collect();
    if parts.len() < 3 {
        return;
    }
    let name = format!("{}_{}", parts[parts.len() - 3], parts[parts.len() - 2]);
    assert!(conditions_extra::is_card_matching_search_name(&mut g, 2401, &json!(name)).unwrap());
    assert!(!conditions_extra::is_card_matching_search_name(&mut g, 2401, &json!(format!("{}9", name))).unwrap());
    assert!(!conditions_extra::is_card_matching_search_name(&mut g, 2401, &json!("")).unwrap());
}
