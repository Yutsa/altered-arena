//! States/TurnTrait.php: afternoon (assignment turns), dusk, night

use crate::core::engine;
use crate::core::globals;
use crate::framework::{self, ST_END_OF_NIGHT, ST_NEW_DAY, ST_PRE_END_OF_GAME};
use crate::game::Game;
use crate::game_methods;
use crate::helpers::conditions;
use crate::managers::{cards, meeples, players};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `actOrderCards($cardIds)`
pub fn act_order_cards(g: &mut Game, card_ids: &Value) -> R<()> {
    let p = players::get_current(g);
    for (i, cid) in php::vals(card_ids).into_iter().enumerate() {
        let c = cards::get_single(g, cid, true)?;
        let bad = match c {
            None => true,
            Some(c) => card::is_played(g, c)? || Some(g.card(c)?.pid) != p,
        };
        if bad {
            return Err(PhpError::new("You can't reorder that card"));
        }
        card::set_state(g, c.unwrap(), i as i64)?;
    }
    Ok(())
}

/// `stBeforeAssignment()`: afternoon resets, then the looping `assignment` order
// PHP: States/TurnTrait.php:41
pub fn st_before_assignment(g: &mut Game) -> R<()> {
    globals::set(g, "phase", json!(2))?;
    g.notify("newPhase", json!({"phase": "afternoon"}));
    for (k, v) in [
        ("stormMoves", json!([])),
        ("afterRest", json!([])),
        ("afterNightCleanup", json!([])),
        ("playedCards", json!(0)),
        ("skippedPlayers", json!([])),
        ("costReduction", json!([])),
        ("nextCharacterBoost", json!(0)),
        ("nextCharacterBoostOccurence", json!(0)),
        ("nextAnimalBoost", json!(0)),
        ("nextAnimalBoostOccurence", json!(0)),
        ("nextReserveCharacterBoost", json!(0)),
        ("playedForFree", json!(false)),
        ("nextCharacterInExpeditionBoost", json!([])),
        ("abilityActivatedThisTurn", json!([])),
        ("abilityActivatedThisTurnCount", json!([])),
        ("abilityActivatedThisTurnTypeCount", json!([])),
        ("smokeThemOutArmed", json!([])),
        ("dayPhase", json!(true)),
    ] {
        globals::set(g, k, v)?;
    }
    for c in cards::get_all(g) {
        let extra = card::get(g, c, "extraDatas")?;
        if php::loose_eq(php::coalesce(php::get(&extra, "userPower"), &json!(false)), &json!(true)) {
            let mut dat = extra.clone();
            php::set(&mut dat, "userPower", json!(false));
            card::set_property(g, c, "extraDatas", dat, true)?;
        }
    }
    game_methods::init_custom_default_turn_order(g, "assignment", json!(framework::ST_ASSIGNMENT), json!("stPreBeforeDusk"), true, true)
}

fn reset_cost_reductions(g: &mut Game) -> R<()> {
    let mut all = globals::get(g, "costReduction")?;
    for pk in php::keys_str(&all) {
        let reductions = php::get_mut(&mut all, &pk);
        for tk in php::keys_str(reductions) {
            let r = php::get_mut(reductions, &tk);
            if !php::is_arr(r) {
                continue;
            }
            if !php::isset(r, "permanent") || php::loose_eq(php::get(r, "permanent"), &json!(false)) {
                php::set(r, "reduction", json!(0));
                php::unset(r, "minimum");
            }
        }
    }
    globals::set(g, "costReduction", all)?;
    Ok(())
}

/// `stAssignment()`: the active player's afternoon turn, or the end of the afternoon
// PHP: States/TurnTrait.php:82
pub fn st_assignment(g: &mut Game) -> R<()> {
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let skipped = globals::get(g, "skippedPlayers")?;
    if php::in_array(&json!(p), &skipped) {
        let remaining = php::array_diff(&json!(players::get_all(g)), &skipped);
        if php::empty(&remaining) {
            globals::set(g, "dayPhase", json!(false))?;
            game_methods::end_custom_order(g, "assignment")?;
        } else {
            game_methods::next_player_custom_order(g, "assignment")?;
        }
        return Ok(());
    }
    globals::set(g, "playedCards", json!(0))?;
    reset_cost_reductions(g)?;
    for (k, v) in [
        ("nextCharacterBoost", json!(0)),
        ("nextCharacterBoostOccurence", json!(0)),
        ("nextAnimalBoost", json!(0)),
        ("nextAnimalBoostOccurence", json!(0)),
        ("nextReserveCharacterBoost", json!(0)),
        ("nextCharacterCost3Anchored", json!(false)),
        ("nextCharacterBaseCost3Anchored", json!(false)),
        ("nextCharacterAsleep", json!(false)),
        ("nextCharacterAnchored", json!(false)),
        ("nextCharacterFleeting", json!(false)),
        ("nextTokenAnchored", json!(false)),
        ("nextTokenAsleep", json!(false)),
        ("additionalEffect", json!([])),
        ("activePId", json!(p)),
    ] {
        globals::set(g, k, v)?;
    }
    conditions::disarm_smoke_them_out_pid(g, p)?;
    for (k, v) in [
        ("nextSpellIsFree", json!(false)),
        ("removeFleetingIfSpellPlayedHand", json!(false)),
        ("removeFleetingSpellPlayed", json!(false)),
        ("removeFleetingCharacterPlayed", json!(false)),
        ("removeFleetingCharacterStat0Played", json!(false)),
        ("removeFleetingSongArtistPlayed", json!(false)),
        ("playedForFree", json!(false)),
        ("nextCharacterBoostV", json!(0)),
        ("nextCharacterInExpeditionBoost", json!([])),
    ] {
        globals::set(g, k, v)?;
    }
    let mut a = globals::get(g, "abilityActivatedThisTurn")?;
    php::set(&mut a, &p.to_string(), json!([]));
    globals::set(g, "abilityActivatedThisTurn", a)?;
    let mut a = globals::get(g, "abilityActivatedThisTurnCount")?;
    php::set(&mut a, &p.to_string(), json!(0));
    globals::set(g, "abilityActivatedThisTurnCount", a)?;
    let mut a = globals::get(g, "abilityActivatedThisTurnTypeCount")?;
    php::set(&mut a, &p.to_string(), json!([]));
    globals::set(g, "abilityActivatedThisTurnTypeCount", a)?;
    let node = json!({"childs": [{"action": "ChooseAssignment", "pId": p}]});
    engine::setup(g, &node, json!({"order": "assignment"}))?;
    engine::proceed(g, false, false)
}

/// `stPreBeforeDusk()`
// PHP: States/TurnTrait.php:181
pub fn st_pre_before_dusk(g: &mut Game) -> R<()> {
    globals::set(g, "phase", json!(3))?;
    g.notify("newPhase", json!({"phase": "dusk"}));
    reset_cost_reductions(g)?;
    for (k, v) in [
        ("nextCharacterBoost", json!(0)),
        ("nextCharacterBoostOccurence", json!(0)),
        ("nextAnimalBoost", json!(0)),
        ("nextAnimalBoostOccurence", json!(0)),
        ("nextReserveCharacterBoost", json!(0)),
        ("nextCharacterCost3Anchored", json!(false)),
        ("nextCharacterAsleep", json!(false)),
        ("nextCharacterAnchored", json!(false)),
        ("nextCharacterFleeting", json!(false)),
        ("nextTokenAnchored", json!(false)),
        ("nextTokenAsleep", json!(false)),
        ("additionalEffect", json!([])),
        ("nextSpellIsFree", json!(false)),
        ("removeFleetingIfSpellPlayedHand", json!(false)),
        ("removeFleetingSpellPlayed", json!(false)),
        ("removeFleetingCharacterPlayed", json!(false)),
        ("removeFleetingCharacterStat0Played", json!(false)),
        ("removeFleetingSongArtistPlayed", json!(false)),
        ("playedForFree", json!(false)),
        ("nextCharacterBoostV", json!(0)),
        ("stormMoves", json!([])),
    ] {
        globals::set(g, k, v)?;
    }
    game_methods::check_card_listeners(g, "BeforeDusk", json!("stBeforeDusk"), json!([]), None)
}

/// `stBeforeDusk()`
pub fn st_before_dusk(g: &mut Game) -> R<()> {
    if globals::get(g, "instantWin")? == json!(true) {
        return framework::jump(g, ST_PRE_END_OF_GAME);
    }
    game_methods::check_card_listeners(g, "AtDusk", json!("stDusk"), json!([]), None)
}

/// `stDusk()`
pub fn st_dusk(g: &mut Game) -> R<()> {
    let a = players::get_active_id(g);
    game_methods::init_custom_turn_order(g, "advancePhase", Some(vec![a]), json!("stAdvanceDusk"), json!("stAfterDusk"), false, true, json!([]))
}

/// `stAdvanceDusk()`: the storm moves (tie-break mode: the tie-break region decides)
// PHP: States/TurnTrait.php:245
pub fn st_advance_dusk(g: &mut Game) -> R<()> {
    g.notify("startDusk", json!({}));
    if globals::get(g, "tieBreakerMode")? == json!(false) || php::loose_eq(&globals::get(g, "tieBreakerMode")?, &json!(false)) {
        engine::setup(g, &json!({"type": "seq", "childs": []}), json!({"order": "advancePhase"}))?;
        players::compute_storm(g, true)?;
        return engine::proceed(g, false, false);
    }
    // Tie breaker mode: needs Players::biomesModifier and Player::getBiomeStrength
    Err(PhpError::nt("States/TurnTrait.php:258 stAdvanceDusk (tie breaker)"))
}

/// `stAfterDusk()`
pub fn st_after_dusk(g: &mut Game) -> R<()> {
    g.notify("endDusk", json!({}));
    game_methods::check_card_listeners(g, "AfterDusk", json!("stBeforeNight"), json!([]), None)
}

/// `stBeforeNight()`
// PHP: States/TurnTrait.php:347
pub fn st_before_night(g: &mut Game) -> R<()> {
    if globals::get(g, "instantWin")? == json!(true) {
        return framework::jump(g, ST_PRE_END_OF_GAME);
    }
    globals::set(g, "phase", json!(4))?;
    g.notify("newPhase", json!({"phase": "night"}));
    globals::set(g, "playedForFree", json!(false))?;
    let a = players::get_active_id(g);
    game_methods::init_custom_turn_order(g, "nightCleanup", Some(vec![a]), json!("stNightCleanup"), json!("stAfterNightCleanup"), false, true, json!([]))
}

/// `stNightCleanup()`
// PHP: States/TurnTrait.php:368
pub fn st_night_cleanup(g: &mut Game) -> R<()> {
    engine::setup(g, &json!({"type": "seq", "childs": []}), json!({"order": "nightCleanup"}))?;
    for pid in players::get_turn_order(g, None)? {
        player::night_cleanup(g, pid)?;
    }
    meeples::night_cleanup(g)?;
    let fp = globals::get(g, "firstPlayer")?;
    globals::set(g, "activePId", fp.clone())?;
    engine::update_parallel_childs(g, &json!({"noIndependent": true}), None)?;
    framework::change_active_player(g, &fp);
    engine::proceed(g, false, false)
}

/// `stAfterNightCleanup()`
pub fn st_after_night_cleanup(g: &mut Game) -> R<()> {
    globals::set(g, "stormMoves", json!([]))?;
    game_methods::check_card_listeners(g, "BeforeNight", json!("stPreNight"), json!([]), None)
}

/// `stPreNight()`
pub fn st_pre_night(g: &mut Game) -> R<()> {
    globals::set(g, "playedCards", json!(1))?;
    game_methods::init_custom_default_turn_order(g, "nightPhase", json!("stNight"), json!(ST_END_OF_NIGHT), false, true)
}

/// `stNight()`: discards above the Reserve / Landmark slots
// PHP: States/TurnTrait.php:400
pub fn st_night(g: &mut Game) -> R<()> {
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getLandmarks() on null"))?;
    let lm = player::get_landmarks(g, p)?.len() as i64;
    let lslots = player::get_landmark_slots(g, p)?;
    let n_landmarks = (lm - lslots).max(0);
    let reserve = player::get_reserve_cards(g, p)?;
    let mut exhausted = 0i64;
    for c in &reserve {
        if card::is_tapped(g, *c)? {
            exhausted += 1;
        }
    }
    let exhausted_slots = player::get_exhausted_reserve_slots(g, p)?;
    let rslots = player::get_reserve_slots(g, p)?;
    let mut n_reserve = (reserve.len() as i64 - rslots).max(0);
    if n_reserve > 0 && exhausted_slots > 0 {
        n_reserve -= exhausted.min(exhausted_slots);
    }
    let n_reserve = n_reserve.max(0);
    if !(n_reserve > 0 || n_landmarks > 0) {
        return game_methods::next_player_custom_order(g, "nightPhase");
    }
    let node = json!({"childs": [{"action": "NightCleanup", "pId": p, "args": {"destination": "discard", "nReserve": n_reserve, "nLandmarks": n_landmarks}}]});
    engine::setup(g, &node, json!({"order": "nightPhase"}))?;
    engine::proceed(g, false, false)
}

/// `stEndOfNight()`
pub fn st_end_of_night(g: &mut Game) -> R<()> {
    game_methods::check_card_listeners(g, "EndOfNight", json!(ST_NEW_DAY), json!([]), None)
}
