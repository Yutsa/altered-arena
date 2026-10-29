//! Card selection and movement actions (Draw, DrawMana, Resupply, DiscardFromDeck, Shuffle,
//! Discard, DiscardDo, Exchange, NightCleanup, Target) run on a recorded position (the rich board
//! of AX_Common_TikTok: player 2001 to play).

use altered_rules::core::engine;
use altered_rules::framework;
use altered_rules::Game;
use serde_json::{json, Value};

/// The golden files are not in git: `GOLDENS_DIR`, else `server/goldens` of this checkout
fn tiktok() -> Option<Game> {
    let dir = std::env::var("GOLDENS_DIR").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens").to_string());
    let path = format!("{}/rich/AX_Common_TikTok.json", dir);
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    Some(Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load"))
}

/// Runs a flow as player 2001's engine, as a server request
fn run(g: &mut Game, flow: Value) {
    g.begin_request(Some(2001)).unwrap();
    engine::setup(g, &json!({"childs": [flow]}), json!({"order": "assignment"})).unwrap();
    engine::proceed(g, false, false).unwrap();
}

/// A V1 answer of player 2001 to the current decision
fn answer(g: &mut Game, mut a: Value) -> Result<(), altered_rules::PhpError> {
    let d = g.try_decision(2001)?.expect("decision");
    a["decisionId"] = d["id"].clone();
    g.answer(2001, &a)
}

fn loc(g: &Game, id: i64) -> String {
    g.cards[&id].location.clone()
}

fn count(g: &Game, location: &str) -> usize {
    g.cards.values().filter(|c| c.location == location).count()
}

#[test]
fn draw_me_two_cards() {
    let Some(mut g) = tiktok() else { return };
    let (hand, deck) = (count(&g, "hand"), count(&g, "deck-2001"));
    run(&mut g, json!({"action": "Draw", "pId": 2001, "args": {"players": "me", "n": 2}}));
    assert_eq!(count(&g, "hand"), hand + 2);
    assert_eq!(count(&g, "deck-2001"), deck - 2);
    assert_eq!(framework::state_id(&g), framework::ST_CHOOSE_ASSIGNMENT);
}

#[test]
fn draw_all_players() {
    let Some(mut g) = tiktok() else { return };
    let (d1, d2) = (count(&g, "deck-2001"), count(&g, "deck-2002"));
    run(&mut g, json!({"action": "Draw", "pId": 2001, "args": {}}));
    assert_eq!(count(&g, "deck-2001"), d1 - 1);
    assert_eq!(count(&g, "deck-2002"), d2 - 1);
}

#[test]
fn draw_mana_exhausted() {
    let Some(mut g) = tiktok() else { return };
    let mana = count(&g, "mana");
    run(&mut g, json!({"action": "DrawMana", "pId": 2001, "args": {"n": 1}}));
    assert_eq!(count(&g, "mana"), mana + 1);
    let new: Vec<_> = g.cards.values().filter(|c| c.location == "mana" && c.props.get("tapped") == Some(&json!(true))).collect();
    assert_eq!(new.len(), 1);
}

#[test]
fn resupply_to_reserve() {
    let Some(mut g) = tiktok() else { return };
    let reserve = count(&g, "reserve");
    run(&mut g, json!({"action": "Resupply", "pId": 2001, "args": {"n": 1}}));
    assert_eq!(count(&g, "reserve"), reserve + 1);
}

#[test]
fn discard_from_deck_all() {
    let Some(mut g) = tiktok() else { return };
    let discard = count(&g, "discard");
    run(&mut g, json!({"action": "DiscardFromDeck", "pId": 2001, "args": {"players": "all", "n": 2}}));
    assert_eq!(count(&g, "discard"), discard + 4);
}

#[test]
fn discard_from_hand_asks_then_moves() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!({"action": "Discard", "pId": 2001, "args": {"source": "hand", "n": 1}}));
    let d = g.decision(2001).expect("decision");
    assert_eq!(d["kind"], json!("select"), "{}", d);
    assert_eq!(d["cardIds"], json!([35, 36, 40, 2401]));
    answer(&mut g, json!({"type": "select", "cardIds": [40]})).unwrap();
    assert_eq!(loc(&g, 40), "discard");
}

#[test]
fn discard_card_id_is_automatic() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!({"action": "Discard", "pId": 2001, "args": {"cardId": 2418, "destination": "hand"}}));
    assert_eq!(loc(&g, 2418), "hand");
    assert_eq!(framework::state_id(&g), framework::ST_CHOOSE_ASSIGNMENT);
}

#[test]
fn discard_to_top_of_deck() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!({"action": "Discard", "pId": 2001, "args": {"cardId": 35, "destination": "topOfDeck"}}));
    assert_eq!(loc(&g, 35), "deck-2001");
    let top = g.cards.values().filter(|c| c.location == "deck-2001").max_by_key(|c| c.state).unwrap();
    assert_eq!(top.id, 35);
}

#[test]
fn exchange_reserve_and_hand() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!({"action": "Exchange", "pId": 2001, "args": {"targetType": ["character", "token", "spell", "permanent"]}}));
    let d = g.decision(2001).expect("decision");
    answer(&mut g, json!({"type": "exchange", "reserve": 2418, "hand": 40})).unwrap_or_else(|e| panic!("{:?} {}", e, d));
    assert_eq!(loc(&g, 2418), "hand");
    assert_eq!(loc(&g, 40), "reserve");
}

#[test]
fn discard_do_then_draw_n() {
    let Some(mut g) = tiktok() else { return };
    let hand = count(&g, "hand");
    run(
        &mut g,
        json!({"action": "DiscardDo", "pId": 2001, "args": {"effect": {"action": "Draw", "args": {"players": "me"}}}}),
    );
    let d = g.decision(2001).expect("decision");
    assert_eq!(d["kind"], json!("select"), "{}", d);
    assert_eq!(d["cardIds"], json!([2418, 2419]));
    answer(&mut g, json!({"type": "select", "cardIds": [2418, 2419]})).unwrap();
    assert_eq!(loc(&g, 2418), "discard");
    assert_eq!(loc(&g, 2419), "discard");
    assert_eq!(count(&g, "hand"), hand + 2);
}

#[test]
fn shuffle_reforms_the_deck() {
    let Some(mut g) = tiktok() else { return };
    let deck = count(&g, "deck-2001");
    // A leaf runs its state (ST_SHUFFLE is a game state): optional only offers a pass in a choice
    run(&mut g, json!({"action": "Shuffle", "pId": 2001, "args": {}}));
    assert_eq!(count(&g, "deck-2001"), deck + 1);
    assert_eq!(loc(&g, 2421), "deck-2001");
}

#[test]
fn night_cleanup_discards_the_selection() {
    let Some(mut g) = tiktok() else { return };
    run(&mut g, json!({"action": "NightCleanup", "pId": 2001, "args": {"nReserve": 1, "nLandmarks": 1}}));
    let d = g.decision(2001).expect("decision");
    assert_eq!(d["kind"], json!("cleanup"), "{}", d);
    assert_eq!(d["reserve"]["cardIds"], json!([2418, 2419]));
    assert_eq!(d["landmarks"]["cardIds"], json!([2420]));
    assert!(answer(&mut g, json!({"type": "cleanup", "reserve": [2418, 2419], "landmarks": [2420]})).is_err());
    answer(&mut g, json!({"type": "cleanup", "reserve": [2419], "landmarks": [2420]})).unwrap();
    assert_eq!(loc(&g, 2419), "discard");
    assert_eq!(loc(&g, 2420), "discard");
    assert_eq!(loc(&g, 2418), "reserve");
}

#[test]
#[ignore = "needs Players::biomesModifier (port-storm)"]
fn target_opponent_character_then_discard() {
    let Some(mut g) = tiktok() else { return };
    run(
        &mut g,
        json!({"action": "Target", "pId": 2001, "args": {"targetPlayer": "opponent", "targetType": ["character"], "effect": {"action": "Discard", "args": {}}}}),
    );
    let d = g.try_decision(2001).unwrap().expect("decision");
    assert_eq!(d["kind"], json!("select"), "{}", d);
    assert_eq!(d["cardIds"], json!([2422, 2423]));
    answer(&mut g, json!({"type": "select", "cardIds": [2422]})).unwrap();
    assert_eq!(loc(&g, 2422), "discard");
    assert_eq!(loc(&g, 2423), "stormRight");
}

#[test]
#[ignore = "needs Players::biomesModifier (port-storm)"]
fn target_forced_choice_is_automatic() {
    let Some(mut g) = tiktok() else { return };
    run(
        &mut g,
        json!({"action": "Target", "pId": 2001, "args": {"targetPlayer": "me", "targetType": ["permanent"], "effect": {"action": "Discard", "args": {"destination": "hand"}}}}),
    );
    assert_eq!(loc(&g, 2420), "hand");
}
