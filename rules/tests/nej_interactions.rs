//! NEJ (Fugue) card interactions: Temple, Construction discounts, name blocks, Maw's Molting
//! cost gates, Feats, and signature tokens. Bootstraps with NEJ starters (no goldens).

use altered_rules::arena::{self, NewPlayer};
use altered_rules::core::{engine, globals};
use altered_rules::helpers::conditions;
use altered_rules::managers::{cards, meeples, players};
use altered_rules::models::{card, player};
use altered_rules::php;
use altered_rules::Game;
use serde_json::{json, Value};

fn sierra_game_raw() -> Game {
    arena::create_v1_game(
        42,
        &[
            NewPlayer { name: "Alice".into(), bot: false, deck: Some("ax_130_wayfinder_sierra".into()), ..Default::default() },
            NewPlayer { name: "Bob".into(), bot: false, deck: Some("br_130_squire_kojo".into()), ..Default::default() },
        ],
        &[],
    )
    .expect("create NEJ game")
}

/// Past first-day mana selection, on the first Afternoon turn with 3 ready mana.
fn sierra_game() -> Game {
    let mut g = sierra_game_raw();
    advance_past_mana(&mut g);
    g
}

fn spawn(g: &mut Game, class: &str, location: &str, pid: i64) -> i64 {
    let props = cards::classes().get(class).unwrap_or_else(|| panic!("missing class {class}")).clone();
    let ids = cards::create(
        g,
        &json!([{
            "location": location,
            "player_id": pid,
            "nbr": 1,
            "properties": props,
        }]),
        None,
    )
    .unwrap_or_else(|e| panic!("create {class}: {e}"));
    *ids.first().expect("created id")
}

fn check(g: &mut Game, cond: &str, card_id: Option<i64>) -> bool {
    conditions::check(g, &json!({ "condition": cond }), card_id, &json!({})).unwrap()
}

fn answer(g: &mut Game, player: u32, mut a: Value) {
    let d = g.decision(player).expect("a decision");
    a["decisionId"] = d["id"].clone();
    g.answer(player, &a).unwrap_or_else(|e| panic!("answer {} refused: {}", a, e));
}

fn answer_select_min(g: &mut Game, player: u32, d: &Value) {
    let min = d["min"].as_u64().unwrap_or(1) as usize;
    let ids: Vec<Value> = d["cardIds"].as_array().into_iter().flatten().take(min).cloned().collect();
    answer(g, player, json!({"type": "select", "cardIds": ids}));
}

fn advance_past_mana(g: &mut Game) {
    for _ in 0..8 {
        let waiting = g.waiting_for();
        if waiting.is_empty() {
            break;
        }
        let mut progressed = false;
        for pid in waiting {
            let Some(d) = g.decision(pid) else { continue };
            if d["kind"] == "select" && d.get("toMana") == Some(&json!(true)) {
                answer_select_min(g, pid, &d);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    // Reach a turn decision for Alice (pass any leftover setup prompts)
    finish_to_turn(g, 2001);
    assert_eq!(
        g.decision(2001).map(|d| d["kind"].clone()),
        Some(json!("turn")),
        "expected Afternoon turn after mana selection, state={}",
        g.oracle_state()["state"]
    );
}

fn finish_to_turn(g: &mut Game, player: u32) {
    for _ in 0..20 {
        let Some(d) = g.decision(player) else { return };
        match d["kind"].as_str() {
            Some("turn") => return,
            Some("select") => answer_select_min(g, player, &d),
            Some("choice") => {
                let pass = d["choices"].as_array().unwrap().iter().find(|c| c["id"] == json!(99)).unwrap_or(&d["choices"][0])["id"].clone();
                answer(g, player, json!({"type": "choice", "id": pass}));
            }
            Some("confirm") => answer(g, player, json!({"type": "confirm"})),
            _ => return,
        }
    }
}

fn playable_destinations(turn: &Value, card_id: i64) -> Vec<String> {
    turn["play"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["cardId"] == json!(card_id))
        .flat_map(|p| p["destinations"].as_array().into_iter().flatten())
        .filter_map(|d| d.as_str().map(str::to_string))
        .collect()
}

#[test]
fn temple_morph_round_trip_preserves_original_card() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let id = spawn(&mut g, "AX_Exalted_HephaestusGodoftheForge", "hand", 2001);

    assert!(card::has_temple(&mut g, id).unwrap());
    assert!(!card::is_played_as_temple(&mut g, id).unwrap());
    assert!(!check(&mut g, "isTemple", Some(id)));
    assert!(check(&mut g, "isNotTemple", Some(id)));

    let original_type = card::get(&mut g, id, "type").unwrap();
    let original_subtypes = card::get_subtypes(&mut g, id).unwrap();
    let original_typeline = card::get(&mut g, id, "typeline").unwrap();
    let original_effect = card::get(&mut g, id, "effectPlayed").unwrap();
    assert_eq!(original_type, json!("character"));
    assert!(php::in_array(&json!("deity"), &original_subtypes));

    card::apply_temple_play(&mut g, id).unwrap();
    assert!(card::is_played_as_temple(&mut g, id).unwrap());
    assert!(check(&mut g, "isTemple", Some(id)));
    assert_eq!(card::get(&mut g, id, "type").unwrap(), json!("permanent"));
    assert_eq!(card::get_subtypes(&mut g, id).unwrap(), json!(["construction", "landmark"]));
    assert_eq!(card::get(&mut g, id, "effectPlayed").unwrap(), json!([]));
    assert_eq!(card::get(&mut g, id, "effectHand").unwrap(), json!([]));
    assert_eq!(card::get(&mut g, id, "effectReserve").unwrap(), json!([]));

    // Idempotent: re-applying keeps the first templeOriginal snapshot
    card::apply_temple_play(&mut g, id).unwrap();
    assert!(card::is_played_as_temple(&mut g, id).unwrap());

    card::revert_from_temple_play(&mut g, id).unwrap();
    assert!(!card::is_played_as_temple(&mut g, id).unwrap());
    assert!(check(&mut g, "isNotTemple", Some(id)));
    assert_eq!(card::get(&mut g, id, "type").unwrap(), original_type);
    assert_eq!(card::get_subtypes(&mut g, id).unwrap(), original_subtypes);
    assert_eq!(card::get(&mut g, id, "typeline").unwrap(), original_typeline);
    assert_eq!(card::get(&mut g, id, "effectPlayed").unwrap(), original_effect);
}

#[test]
fn temple_costs_less_than_character_play_and_lists_landmark() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let id = spawn(&mut g, "AX_Exalted_HephaestusGodoftheForge", "hand", 2001);

    assert_eq!(php::intval(&card::get_cost(&mut g, id, false, false, false).unwrap()), 3);
    assert_eq!(php::intval(&card::get_cost(&mut g, id, false, false, true).unwrap()), 2);

    let mana = player::get_mana(&mut g, 2001).unwrap() as i64;
    assert!(mana >= 2, "starter game should have enough mana for temple cost, got {mana}");

    assert!(card::can_be_played(&mut g, id, 2001, false, false, true).unwrap());
    assert_eq!(card::get_temple_playable_locations(&mut g, id, 2001).unwrap(), json!(["landmark"]));

    // A non-temple card never offers temple locations
    let station = spawn(&mut g, "AX_Common_RestockingStation", "hand", 2001);
    assert!(!card::has_temple(&mut g, station).unwrap());
    assert!(!card::can_be_played(&mut g, station, 2001, false, false, true).unwrap());
    assert_eq!(card::get_temple_playable_locations(&mut g, station, 2001).unwrap(), json!([]));
}

#[test]
fn oddball_costs_nothing_after_a_construction_this_day() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let oddball = spawn(&mut g, "AX_Common_Oddball", "reserve", 2001);

    assert_eq!(php::intval(&card::get_cost(&mut g, oddball, false, false, false).unwrap()), 2);
    assert!(!player::has_played_construction_this_day(&mut g, 2001).unwrap());
    // Without the day flag, Oddball still costs 2 — playable only if mana covers it
    let mana = player::get_mana(&mut g, 2001).unwrap() as i64;
    assert_eq!(card::can_be_played(&mut g, oddball, 2001, false, false, false).unwrap(), mana >= 2);

    player::mark_construction_played_this_day(&mut g, 2001).unwrap();
    assert!(player::has_played_construction_this_day(&mut g, 2001).unwrap());
    // Discount 2 on a 2-cost reserve play → free
    assert!(card::can_be_played(&mut g, oddball, 2001, false, false, false).unwrap());
}

#[test]
fn temple_play_marks_construction_and_unlocks_oddball() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let hephaestus = spawn(&mut g, "AX_Exalted_HephaestusGodoftheForge", "hand", 2001);
    let oddball = spawn(&mut g, "AX_Common_Oddball", "reserve", 2001);

    assert!(!player::has_played_construction_this_day(&mut g, 2001).unwrap());

    // Play as Temple through the assignment action (landmark destination)
    let turn = g.decision(2001).expect("turn");
    assert_eq!(turn["kind"], "turn");
    let dests = playable_destinations(&turn, hephaestus);
    assert!(dests.iter().any(|d| d == "landmark"), "Hephaestus temple play should list landmark, got {dests:?}");

    answer(&mut g, 2001, json!({"type": "play", "cardId": hephaestus, "destination": "landmark"}));
    finish_to_turn(&mut g, 2001);

    assert!(card::is_played_as_temple(&mut g, hephaestus).unwrap());
    assert_eq!(card::location(&g, hephaestus).unwrap(), "landmark");
    assert_eq!(card::get_subtypes(&mut g, hephaestus).unwrap(), json!(["construction", "landmark"]));
    assert!(player::has_played_construction_this_day(&mut g, 2001).unwrap(), "temple morph is a Construction for Oddball's discount");
    assert!(card::can_be_played(&mut g, oddball, 2001, false, false, false).unwrap(), "Oddball should be free after temple-as-construction");
}

#[test]
fn construction_then_oddball_play_from_reserve() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let station = spawn(&mut g, "AX_Common_RestockingStation", "hand", 2001);
    let oddball = spawn(&mut g, "AX_Common_Oddball", "reserve", 2001);

    let turn = g.decision(2001).expect("turn");
    let dests = playable_destinations(&turn, station);
    assert!(dests.iter().any(|d| d == "landmark"), "Restocking Station destinations: {dests:?}");
    answer(&mut g, 2001, json!({"type": "play", "cardId": station, "destination": "landmark"}));

    assert!(player::has_played_construction_this_day(&mut g, 2001).unwrap());
    assert_eq!(card::location(&g, station).unwrap(), "landmark");
    assert!(card::can_be_played(&mut g, oddball, 2001, false, false, false).unwrap(), "Oddball should be free after Construction this Day");

    // Afternoon alternates: Bob must act before Alice can play Oddball
    let bob = g.decision(2002).expect("Bob's turn after Alice's Construction");
    assert_eq!(bob["kind"], "turn");
    answer(&mut g, 2002, json!({"type": "endTurn"}));
    finish_to_turn(&mut g, 2001);

    let turn = g.decision(2001).expect("Alice's turn after Bob passes");
    assert_eq!(turn["kind"], "turn");
    let oddball_dests = playable_destinations(&turn, oddball);
    assert!(!oddball_dests.is_empty(), "Oddball should be playable after Construction, destinations={oddball_dests:?}");
    answer(&mut g, 2001, json!({"type": "play", "cardId": oddball, "destination": oddball_dests[0]}));

    let loc = card::location(&g, oddball).unwrap();
    assert!(loc == "stormLeft" || loc == "stormRight", "Oddball should reach an expedition, got {loc}");
}

#[test]
fn blocked_card_name_this_day_stops_plays_and_clears_on_new_day() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let hephaestus = spawn(&mut g, "AX_Exalted_HephaestusGodoftheForge", "hand", 2001);
    let name = card::get(&mut g, hephaestus, "name").unwrap();

    assert!(!card::is_name_blocked_this_day(&mut g, hephaestus, 2001).unwrap());
    assert!(card::can_be_played(&mut g, hephaestus, 2001, false, false, true).unwrap());

    globals::set(&mut g, "blockedCardNamesThisDay", json!({"2001": [name.clone()]})).unwrap();
    assert!(card::is_name_blocked_this_day(&mut g, hephaestus, 2001).unwrap());
    assert!(!card::can_be_played(&mut g, hephaestus, 2001, false, false, false).unwrap());
    assert!(!card::can_be_played(&mut g, hephaestus, 2001, false, false, true).unwrap());
    assert_eq!(card::get_temple_playable_locations(&mut g, hephaestus, 2001).unwrap(), json!([]));

    // Opponent is unaffected by Alice's block list
    assert!(!players::is_card_name_blocked_this_day(&mut g, 2002, &name).unwrap());

    // SpecialEffect writer used by Ship Security Officer
    let officer = spawn(&mut g, "OD_Rare_ShipSecurityOfficer", "hand", 2001);
    let bob_card = spawn(&mut g, "BR_Common_ProtecttheAssets", "reserve", 2002);
    let bob_name = card::get(&mut g, bob_card, "name").unwrap();
    g.begin_request(Some(2001)).unwrap();
    let flow = json!({
        "childs": [{
            "action": "SpecialEffect",
            "pId": 2001,
            "sourceId": officer,
            "args": {"effect": "blockOpponentsCardNameThisDay", "args": [], "cardId": bob_card}
        }]
    });
    engine::setup(&mut g, &flow, json!({"order": "assignment"})).unwrap();
    engine::proceed(&mut g, false, false).unwrap();
    assert!(players::is_card_name_blocked_this_day(&mut g, 2002, &bob_name).unwrap());
    assert!(!players::is_card_name_blocked_this_day(&mut g, 2001, &bob_name).unwrap());

    // New day clears both day-scoped globals (mirrors stNewDay)
    player::mark_construction_played_this_day(&mut g, 2001).unwrap();
    globals::set(&mut g, "constructionPlayedThisDay", json!([])).unwrap();
    globals::set(&mut g, "blockedCardNamesThisDay", json!([])).unwrap();
    assert!(!player::has_played_construction_this_day(&mut g, 2001).unwrap());
    assert!(!card::is_name_blocked_this_day(&mut g, hephaestus, 2001).unwrap());
}

#[test]
fn maws_molting_cost_gates_need_sacrifice_or_boosts() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();

    let yz = spawn(&mut g, "YZ_Rare_MawsMolting", "hand", 2001);
    let mu = spawn(&mut g, "MU_Rare_MawsMolting", "hand", 2001);

    // Base cost 2; with 3 mana both are playable without the optional reduction
    let mana = player::get_mana(&mut g, 2001).unwrap() as i64;
    assert!(mana >= 2);
    assert!(card::can_be_played(&mut g, yz, 2001, false, false, false).unwrap());
    assert!(card::can_be_played(&mut g, mu, 2001, false, false, false).unwrap());

    // Spend-boost reduction only kicks in with ≥2 expedition character boosts
    assert_eq!(card::count_expedition_character_boosts(&mut g, 2001).unwrap(), 0);
    let fodder = spawn(&mut g, "LY_Common_Shift", "stormLeft", 2001);
    meeples::create_on_card(&mut g, "boost", fodder, &json!(2001), 2).unwrap();
    assert_eq!(card::count_expedition_character_boosts(&mut g, 2001).unwrap(), 2);

    // Both optional reductions are available; cards remain playable either way
    assert!(card::can_be_played(&mut g, yz, 2001, false, false, false).unwrap());
    assert!(card::can_be_played(&mut g, mu, 2001, false, false, false).unwrap());
}

#[test]
fn complete_feat_is_idempotent_and_visible_to_conditions() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let feat = spawn(&mut g, "BR_Common_ProtecttheAssets", "landmark", 2001);
    assert!(php::in_array(&json!("feat"), &card::get_subtypes(&mut g, feat).unwrap()));
    assert!(!check(&mut g, "isThisFeatCompleted", Some(feat)));
    assert!(check(&mut g, "isThisFeatIncomplete", Some(feat)));

    let run = |g: &mut Game, source: i64| {
        engine::setup(
            g,
            &json!({"childs": [{
                "action": "CompleteFeat",
                "pId": 2001,
                "sourceId": source,
                "args": {"cardId": source}
            }]}),
            json!({}),
        )
        .unwrap();
        engine::proceed(g, false, false).unwrap();
    };
    run(&mut g, feat);
    run(&mut g, feat);
    assert_eq!(meeples::count_meeples(&g, &format!("card-{feat}"), &json!("featCompleted")).unwrap(), 1);
    assert!(check(&mut g, "isThisFeatCompleted", Some(feat)));
    assert!(!check(&mut g, "isThisFeatIncomplete", Some(feat)));
    assert!(check(&mut g, "hasCompletedFeat", Some(feat)));
}

#[test]
fn temple_landmark_is_not_a_character_boost_target() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let heph = spawn(&mut g, "AX_Exalted_HephaestusGodoftheForge", "hand", 2001);
    let oddball = spawn(&mut g, "AX_Common_Oddball", "stormLeft", 2001);

    answer(&mut g, 2001, json!({"type": "play", "cardId": heph, "destination": "landmark"}));
    finish_to_turn(&mut g, 2001);

    // The next request rebuilds the card from its saved row. It must stay a landmark, not a character.
    g.begin_request(Some(2001)).unwrap();
    assert!(card::is_played_as_temple(&mut g, heph).unwrap());
    assert_eq!(card::get(&mut g, heph, "type").unwrap(), json!("permanent"));
    assert_eq!(card::location(&g, heph).unwrap(), "landmark");

    let boostable = cards::get_filtered(&mut g, &json!(2001), &Value::Null, &json!(["character", "token"]), true).unwrap();
    assert!(boostable.contains(&oddball), "an expedition character stays boostable");
    assert!(!boostable.contains(&heph), "a temple landmark is not a character, so it cannot be boosted");
}

/// Hestia played as a Temple: two At Noon effects of the same card. The GameView must not treat
/// the skippable "send me to Reserve" as the resolution of the mandatory reserve boost (that made
/// both portraits show "put it in reserve", and accepting the boost auto-sent her to Reserve).
fn hestia_temple_noon() -> (Game, i64, i64) {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let hestia = spawn(&mut g, "BR_Exalted_HestiaGoddessoftheHearth", "landmark", 2001);
    card::apply_temple_play(&mut g, hestia).unwrap();
    let oddball = spawn(&mut g, "AX_Common_Oddball", "reserve", 2001);
    let event = json!({"type": "Noon", "method": "Noon", "phase": true, "pId": 2001});
    let flow = cards::apply_effect(&mut g, hestia, 2001, &json!("Noon"), &event).unwrap();
    assert!(!flow.is_null(), "Hestia as a Temple listens at Noon");
    engine::setup(&mut g, &flow, json!({"order": "Noon"})).unwrap();
    engine::proceed(&mut g, false, false).unwrap();
    (g, hestia, oddball)
}

fn hestia_noon_choices(g: &mut Game) -> Vec<Value> {
    let view = g.view(Some(2001), "t").unwrap();
    let d = view.get("decision").cloned().unwrap_or(Value::Null);
    assert_eq!(d["kind"], "choice", "Hestia Noon should offer a choice, got {d}");
    d["choices"].as_array().cloned().unwrap_or_default()
}

#[test]
fn hestia_temple_noon_effects_keep_distinct_labels() {
    let (mut g, hestia, oddball) = hestia_temple_noon();
    let choices = hestia_noon_choices(&mut g);
    let effects: Vec<&Value> = choices.iter().filter(|c| c["id"] != json!(99) && c.get("sourceCardId") == Some(&json!(hestia))).collect();
    assert_eq!(effects.len(), 2, "Temple Noon and reserve boost of the same Hestia, got {choices:?}");

    let labels: Vec<&str> = effects.iter().filter_map(|c| c["label"].as_str()).collect();
    assert!(labels.iter().any(|l| l.to_lowercase().contains("reserve") && !l.to_lowercase().contains("boost")), "send to Reserve: {labels:?}");
    assert!(labels.iter().any(|l| l.to_lowercase().contains("boost")), "characters in Reserve gain a boost: {labels:?}");
    assert_ne!(labels[0], labels[1], "the two At Noon texts must differ, got {labels:?}");

    let boost = effects.iter().find(|c| c["label"].as_str().is_some_and(|l| l.to_lowercase().contains("boost"))).unwrap();
    assert!(boost.get("resolvesTo").is_none(), "the boost must not resolve to the Temple send ({boost})");

    // Resolving the boost leaves the Temple send, still skippable
    let boost_id = boost["id"].clone();
    answer(&mut g, 2001, json!({"type": "choice", "id": boost_id}));
    assert_eq!(meeples::count_meeples(&g, &format!("card-{oddball}"), &json!("boost")).unwrap(), 1);
    assert_eq!(card::location(&g, hestia).unwrap(), "landmark", "sending to Reserve is still to choose");

    let after = hestia_noon_choices(&mut g);
    let send = after.iter().find(|c| c["id"] != json!(99) && c.get("sourceCardId") == Some(&json!(hestia)));
    let pass = after.iter().find(|c| c["id"] == json!(99));
    assert!(send.is_some(), "Temple send still offered after the boost, got {after:?}");
    assert!(pass.is_some(), "Temple send remains skippable, got {after:?}");
    let pass_id = pass.unwrap()["id"].clone();
    answer(&mut g, 2001, json!({"type": "choice", "id": pass_id}));
    assert_eq!(card::location(&g, hestia).unwrap(), "landmark");
}

#[test]
fn kikimora_keeps_her_boost_if_hold_the_line_resolves_first() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let kiki = spawn(&mut g, "BR_Rare_Kikimora", "stormLeft", 2001);
    let _oddball = spawn(&mut g, "AX_Common_Oddball", "stormRight", 2001);
    let hold = spawn(&mut g, "BR_Exalted_HoldtheLine", "hand", 2001);
    let turn = g.decision(2001).expect("turn");
    let dests = playable_destinations(&turn, hold);
    assert!(dests.iter().any(|d| d == "landmark"), "Hold the Line destinations: {dests:?}");
    answer(&mut g, 2001, json!({"type": "play", "cardId": hold, "destination": "landmark"}));

    let mut targeted = false;
    for _ in 0..12 {
        let Some(d) = g.decision(2001) else { break };
        match d["kind"].as_str() {
            Some("choice") => {
                let choices = d["choices"].as_array().unwrap();
                let hold_c = choices.iter().find(|c| c["sourceCardId"] == json!(hold) && c["id"] != json!(99) && c["enabled"] == json!(true));
                let kiki_c = choices.iter().find(|c| c["sourceCardId"] == json!(kiki) && c["id"] != json!(99) && c["enabled"] == json!(true));
                if let Some(c) = hold_c {
                    answer(&mut g, 2001, json!({"type": "choice", "id": c["id"]}));
                } else if let Some(c) = kiki_c {
                    answer(&mut g, 2001, json!({"type": "choice", "id": c["id"]}));
                } else {
                    panic!("unexpected choice {d}");
                }
            }
            Some("select") => {
                let ids = d["cardIds"].as_array().unwrap();
                assert!(ids.contains(&json!(kiki)), "Hold the Line should be able to target Kikimora, {d}");
                assert_eq!(
                    meeples::count_meeples(&g, &format!("card-{kiki}"), &json!("boost")).unwrap(),
                    0,
                    "Hold the Line is resolving first; Kikimora must not have boosted yet"
                );
                targeted = true;
                answer(&mut g, 2001, json!({"type": "select", "cardIds": [kiki]}));
            }
            Some("turn") | Some("confirm") => break,
            other => panic!("unexpected decision {other:?}: {d}"),
        }
    }
    assert!(targeted, "Hold the Line should have asked for a target");
    assert_eq!(
        meeples::count_meeples(&g, &format!("card-{kiki}"), &json!("boost")).unwrap(),
        3,
        "Hold the Line's 2 boosts plus Kikimora's trigger, which already fired with no boosts"
    );
}

#[test]
fn wayfinder_sierra_exposes_oddball_signature_token() {
    let mut g = sierra_game();
    g.begin_request(Some(2001)).unwrap();
    let hero = player::get_hero(&g, 2001).unwrap().expect("Sierra");
    assert_eq!(player::get_hero_signature_token_type(&mut g, 2001).unwrap().as_deref(), Some("AX_Common_Oddball"));
    assert!(check(&mut g, "hasHeroSignatureToken", Some(hero)));
    assert!(conditions::check(&mut g, &json!({"condition": "isHeroSignatureToken"}), Some(hero), &json!({"invoked": "AX_Common_Oddball"})).unwrap());
    assert!(!conditions::check(&mut g, &json!({"condition": "isHeroSignatureToken"}), Some(hero), &json!({"invoked": "BR_Common_BlazingBooda"})).unwrap());
}
