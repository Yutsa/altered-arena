//! SpecialEffect cases that change globals or card data: flags for the next played cards, cost
//! reductions, counters, effects delayed to after Rest, first player, dice rolls.

use super::{add, ctype, player_of, req_player, sub, Case, Out, Se};
use crate::core::globals;
use crate::game::Game;
use crate::helpers::{conditions_extra, ft, utils};
use crate::managers::{cards, players};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub(super) fn case_of(effect: &str) -> Option<Case> {
    let f: Case = match effect {
        "useCard" => use_card,
        "unuseCard" => unuse_card,
        "armSmokeThemOut" => arm_smoke_them_out,
        "disarmSmokeThemOut" => disarm_smoke_them_out,
        "costReduction" => cost_reduction,
        "gainCounter" | "incCounter" => gain_counter,
        "nextCharacterGains1Boost" => next_character_gains_1_boost,
        "nextCharacterGains1BoostAndAsleep" => next_character_gains_1_boost_and_asleep,
        "nextCharacterAsleep" => next_character_asleep,
        "nextAnimalGains1Boost" => next_animal_gains_1_boost,
        "nextReserveCharacterGains1Boost" => next_reserve_character_gains_1_boost,
        "nextCharacterGains2Boost" => next_character_gains_2_boost,
        "nextSpellIsFree" => next_spell_is_free,
        "nextCharacterCost3Anchored" => |g, _| set_flag(g, "nextCharacterCost3Anchored"),
        "nextCharacterAnchored" => |g, _| set_flag(g, "nextCharacterAnchored"),
        "nextTokenAnchored" => |g, _| set_flag(g, "nextTokenAnchored"),
        // Globals has no removeFleetingIfPlayedHand variable: the setter throws
        "removeFleetingIfPlayedHand" => |g, _| set_flag(g, "removeFleetingIfPlayedHand"),
        "removeFleetingSpellPlayed" => |g, _| set_flag(g, "removeFleetingSpellPlayed"),
        "removeFleetingCharacterPlayed" => |g, _| set_flag(g, "removeFleetingCharacterPlayed"),
        "removeFleetingIfSpellPlayedHand" => |g, _| set_flag(g, "removeFleetingIfSpellPlayedHand"),
        "nextCharacterFleeting" => |g, _| set_flag(g, "nextCharacterFleeting"),
        "instantWin" => instant_win,
        "triggerEffectOfNextCharacter" => trigger_effect_of_next_character,
        "AfterRestSabotage" => after_rest_sabotage,
        "AfterRestOrdisRecruit" => after_rest_ordis_recruit,
        "AfterRest2OrdisRecruit" => after_rest_2_ordis_recruit,
        "afterRest" => after_rest,
        "removeFleetingCharacterStat0Played" => |g, _| set_flag(g, "removeFleetingCharacterStat0Played"),
        "removeFleetingSongArtistPlayed" => |g, _| set_flag(g, "removeFleetingSongArtistPlayed"),
        "counterPerCharacter" => counter_per_character,
        "nextCharacterVGainsBoost" => next_character_v_gains_boost,
        "switchPlayer" => switch_player,
        "playAnotherTurn" => play_another_turn,
        "globalTough" => global_tough,
        "nextTokenAsleep" => |g, _| set_flag(g, "nextTokenAsleep"),
        "doppelganger" => doppelganger,
        "resetCard" => reset_card,
        "nextCharactInExpeditionBoost" => next_charact_in_expedition_boost,
        "nextCharacterBaseCost3Anchored" => |g, _| set_flag(g, "nextCharacterBaseCost3Anchored"),
        "addToCurrentRolls" => add_to_current_rolls,
        _ => return None,
    };
    Some(f)
}

/// `Globals::set<Name>(true)`
fn set_flag(g: &mut Game, name: &str) -> R<Out> {
    globals::set(g, name, json!(true))?;
    Ok(Out::Break)
}

fn set_user_power(g: &mut Game, s: &mut Se, v: bool) -> R<Out> {
    let c = s.card()?;
    let mut data = card::get(g, c, "extraDatas")?;
    php::set(&mut data, "userPower", json!(v));
    card::set_property(g, c, "extraDatas", data, true)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:348
fn use_card(g: &mut Game, s: &mut Se) -> R<Out> {
    set_user_power(g, s, true)
}

// PHP: Actions/SpecialEffect.php:353
fn unuse_card(g: &mut Game, s: &mut Se) -> R<Out> {
    set_user_power(g, s, false)
}

// PHP: Actions/SpecialEffect.php:358
fn arm_smoke_them_out(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = card::pid(g, c)?;
    conditions_extra::arm_smoke_them_out_pid(g, pid, c)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:361
fn disarm_smoke_them_out(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = card::pid(g, c)?;
    conditions_extra::consume_smoke_them_out_charge_pid(g, pid, Some(c))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:365
fn cost_reduction(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut reduction = globals::get(g, "costReduction")?;
    let c = s.card()?;
    let pk = card::pid(g, c)?.to_string();
    let args = s.args.clone();
    let ty = php::get(&args, "type").clone();
    let tk = php::key_of(&ty);
    if !php::isset(&args, "minimum") {
        let mine = php::coalesce(php::get(&reduction, &pk), &php::arr()).clone();
        for cost_type in php::keys(&mine) {
            let ck = php::key_of(&cost_type);
            if !php::is_arr(php::get(php::get(&reduction, &pk), &ck)) {
                continue;
            }
            if utils::cost_reduction_buckets_overlap(&ty, &cost_type) {
                php::unset(php::get_mut(php::get_mut(&mut reduction, &pk), &ck), "minimum");
            }
        }
    }
    let bucket = php::get(php::get(&reduction, &pk), &tk).clone();
    let r = add(php::coalesce(php::get(&bucket, "reduction"), &json!(0)), php::get(&args, "reduction"));
    php::set(php::get_mut(php::get_mut(&mut reduction, &pk), &tk), "reduction", r);
    let bucket = php::get(php::get(&reduction, &pk), &tk).clone();
    let perm = php::truthy(php::coalesce(php::get(&bucket, "permanent"), &json!(false))) || php::truthy(php::coalesce(php::get(&args, "permanent"), &json!(false)));
    php::set(php::get_mut(php::get_mut(&mut reduction, &pk), &tk), "permanent", json!(perm));
    if php::isset(&args, "minimum") {
        php::set(php::get_mut(php::get_mut(&mut reduction, &pk), &tk), "minimum", php::get(&args, "minimum").clone());
    }
    globals::set(g, "costReduction", reduction)?;
    Ok(Out::Break)
}

/// `gainCounter` and `incCounter` (same code)
// PHP: Actions/SpecialEffect.php:387
fn gain_counter(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let mut data = card::get(g, c, "extraDatas")?;
    let loc = card::location(g, c)?;
    let current = php::coalesce(php::get(&data, "counter"), &json!(0)).clone();
    if php::gt(&current, &json!(0))
        && ["stormLeft", "stormRight", "landmark", "reserve"].contains(&loc.as_str())
        && card::has_counters(g, c)?
        && players::has_block_gain_new_counters(g)?
    {
        g.notify("message", json!({"msg": "No new counter can be added to cards"}));
        s.resolve(g, php::arr())?;
        return Ok(Out::Done(None));
    }
    // No counter can be gained in hand
    if loc == "hand" {
        s.resolve(g, php::arr())?;
        return Ok(Out::Done(None));
    }
    let max = php::coalesce(php::get(&s.args, "maxCounter"), &json!(99)).clone();
    if php::gt(&add(&current, php::coalesce(php::get(&s.args, "counter"), &json!(0))), &max) {
        let v = sub(&max, &current);
        php::set(&mut s.args, "counter", v);
    }
    // `$args['counter'] ?? 0 > 0` is `$args['counter'] ?? false`
    if php::truthy(php::coalesce(php::get(&s.args, "counter"), &json!(false))) {
        let n = add(&current, php::coalesce(php::get(&s.args, "counter"), &json!(0)));
        php::set(&mut data, "counter", n);
        php::set(&mut data, "counterName", php::coalesce(php::get(&s.args, "counterName"), &json!("")).clone());
        card::set_property(g, c, "extraDatas", data, true)?;
        g.notify("gainCounter", json!({"cardId": c, "n": php::get(&s.args, "counter")}));
        let p = player_of(g, c)?;
        s.after_listeners(g, p, json!({"sourceId": c, "specialEffect": "gainCounter"}), true, None)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:475
fn next_character_gains_1_boost(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextCharacterBoost", 1)?;
    globals::inc(g, "nextCharacterBoostOccurence", 1)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:479
fn next_character_gains_1_boost_and_asleep(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextCharacterBoost", 1)?;
    globals::inc(g, "nextCharacterBoostOccurence", 1)?;
    globals::set(g, "nextCharacterAsleep", json!(true))?;
    Ok(Out::Break)
}

/// `getArg('optional')` throws when the node has no `optional` arg (no default)
// PHP: Actions/SpecialEffect.php:484
fn next_character_asleep(g: &mut Game, s: &mut Se) -> R<Out> {
    let opt = s.arg(g, "optional")?;
    let v = if php::truthy(&opt) { json!({"value": true, "optional": true}) } else { json!(true) };
    globals::set(g, "nextCharacterAsleep", v)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:487
fn next_animal_gains_1_boost(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextAnimalBoost", 1)?;
    globals::inc(g, "nextAnimalBoostOccurence", 1)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:491
fn next_reserve_character_gains_1_boost(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextReserveCharacterBoost", 1)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:494
fn next_character_gains_2_boost(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextCharacterBoost", 2)?;
    globals::inc(g, "nextCharacterBoostOccurence", 1)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:498
fn next_spell_is_free(g: &mut Game, s: &mut Se) -> R<Out> {
    globals::set(g, "nextSpellIsFree", json!(true))?;
    g.notify("message", json!({"msg": "Next spell played this turn will be free", "cardId": s.card}));
    Ok(Out::Break)
}

/// Stats are not kept by the port; the hero lookups are (a missing hero is an error in PHP)
// PHP: Actions/SpecialEffect.php:698
fn instant_win(g: &mut Game, s: &mut Se) -> R<Out> {
    if !globals::is(g, "instantWin")? {
        let c = s.card()?;
        let p = player_of(g, c)?;
        g.players.get_mut(&p).ok_or_else(|| PhpError::new("Error: Call to a member function setScore() on null"))?.score = 1;
        player::get_hero(g, p)?.ok_or_else(|| PhpError::new("Error: Call to a member function getStatData() on null"))?;
        let next = req_player(players::get_next(g, p)?)?;
        player::get_hero(g, next)?.ok_or_else(|| PhpError::new("Error: Call to a member function getStatData() on null"))?;
        globals::set(g, "instantWin", json!(true))?;
        g.notify("message", json!({"msg": "${player_name} wins the game with ${card_name}'s effect", "player_id": p, "cardId": c}));
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:777
fn trigger_effect_of_next_character(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut add_effects = globals::get(g, "additionalEffect")?;
    let c = s.card()?;
    let args = s.args.clone();
    let limit = php::coalesce(php::get(&args, "limit"), &json!(1000)).clone();
    let to = php::coalesce(php::get(&args, "to"), &Value::Null).clone();
    let mut found = false;
    for e in php::vals(&add_effects) {
        let mut e = e.clone();
        if php::isset(&e, "boost") {
            php::unset(&mut e, "boost");
        }
        let probe = json!({
            "type": php::get(&args, "type"),
            "from": php::get(&args, "from"),
            "limit": limit,
            "effect": php::get(&args, "effect"),
            "to": to,
            "sourceId": c,
        });
        if php::loose_eq(&e, &probe) {
            g.notify("message", json!({"msg": "Effect is ignored as already triggered"}));
            found = true;
            break;
        }
    }
    if !found {
        let mut eff = json!({
            "type": php::get(&args, "type"),
            "from": php::get(&args, "from"),
            "effect": php::get(&args, "effect"),
            "to": to,
            "sourceId": c,
            "limit": limit,
        });
        if php::gt(php::coalesce(php::get(&args, "boost"), &json!(0)), &json!(0)) {
            php::set(&mut eff, "boost", php::get(&args, "boost").clone());
        }
        add_effects = php::array_merge(&[&add_effects, &json!([eff])]);
        globals::set(g, "additionalEffect", add_effects)?;
        let active = players::get_active_id(g);
        g.notify("message", json!({"msg": "${player_name} will trigger {R} effect of next played character", "player_id": active}));
    }
    Ok(Out::Break)
}

/// `$afterRest[$pId] = array_merge($afterRest[$pId] ?? [], $nodes)`
fn push_after_rest(g: &mut Game, pid: i64, nodes: Value) -> R<()> {
    let mut after_rest = globals::get(g, "afterRest")?;
    let pk = pid.to_string();
    if !php::isset(&after_rest, &pk) {
        php::set(&mut after_rest, &pk, php::arr());
    }
    let merged = php::array_merge(&[php::get(&after_rest, &pk), &nodes]);
    php::set(&mut after_rest, &pk, merged);
    globals::set(g, "afterRest", after_rest)?;
    Ok(())
}

// PHP: Actions/SpecialEffect.php:816
fn after_rest_sabotage(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = player_of(g, c)?;
    let node = ft::action(
        "Target",
        json!({
            "targetType": ["character", "spell", "token", "permanent"],
            "targetLocation": ["reserve"],
            "upTo": true,
            "effect": ft::action("Discard", php::arr(), php::arr()),
        }),
        json!({"sourceId": c}),
    );
    push_after_rest(g, pid, json!([node]))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:836
fn after_rest_ordis_recruit(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let expedition = s.ctx_arg(g, "expedition");
    let expedition = if expedition == json!("source") { json!([card::location(g, c)?]) } else { json!(["stormLeft", "stormRight"]) };
    let pid = player_of(g, c)?;
    let node = ft::action("InvokeToken", json!({"pId": "source", "tokenType": "OD_Common_OrdisRecruit", "targetLocation": expedition}), json!({"sourceId": c}));
    push_after_rest(g, pid, json!([node]))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:905
fn after_rest_2_ordis_recruit(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = player_of(g, c)?;
    let node = ft::action(
        "InvokeToken",
        json!({"pId": "source", "tokenType": "OD_Common_OrdisRecruit", "n": 2, "targetLocation": ["stormLeft", "stormRight"]}),
        json!({"sourceId": c}),
    );
    push_after_rest(g, pid, json!([node]))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:925
fn after_rest(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = player_of(g, c)?;
    // We tag the tree to transport the source
    let mut args = utils::tag_tree(&s.args, &json!({"sourceId": c}), false);
    // A card invoking with the parameter source gets the current location
    let loc = card::location(g, c)?;
    args = utils::update_tree(&args, &json!(["source"]), &json!([loc]), &["targetLocation"]);
    args = utils::update_tree(&args, &json!(["initialSource"]), &json!([loc]), &["targetLocation"]);
    s.args = args.clone();
    push_after_rest(g, pid, json!([args]))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1525
fn counter_per_character(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut count = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if php::in_list(&ctype(g, x)?, &["token", "character"]) {
            count += 1;
        }
    }
    if count > 0 {
        let mut data = card::get(g, c, "extraDatas")?;
        let n = add(php::coalesce(php::get(&data, "counter"), &json!(0)), &json!(count));
        php::set(&mut data, "counter", n);
        card::set_property(g, c, "extraDatas", data, true)?;
        g.notify("gainCounter", json!({"cardId": c, "n": count}));
        let p = player_of(g, c)?;
        s.after_listeners(g, p, json!({"sourceId": c, "specialEffect": "gainCounter"}), true, None)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1541
fn next_character_v_gains_boost(g: &mut Game, _s: &mut Se) -> R<Out> {
    globals::inc(g, "nextCharacterBoostV", 1)?;
    globals::inc(g, "nextCharacterBoostOccurence", 1)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1889
fn switch_player(g: &mut Game, s: &mut Se) -> R<Out> {
    let new_first = s.ctx_arg(g, "pId");
    globals::set(g, "firstPlayer", new_first.clone())?;
    let p = players::get_by_value(g, &new_first);
    g.notify("switchPlayer", json!({"player_id": p}));
    Ok(Out::Break)
}

/// Rewind one step so nextPlayerCustomOrder picks the same player once (`--` on null is null)
// PHP: Actions/SpecialEffect.php:1894
fn play_another_turn(g: &mut Game, _s: &mut Se) -> R<Out> {
    let mut orders = globals::get(g, "customTurnOrders")?;
    if php::isset(&orders, "assignment") {
        let idx = php::get(php::get(&orders, "assignment"), "index").clone();
        let idx = if idx.is_null() { Value::Null } else { sub(&idx, &json!(1)) };
        php::set(php::get_mut(&mut orders, "assignment"), "index", idx);
        globals::set(g, "customTurnOrders", orders)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1998
fn global_tough(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut gt = globals::get(g, "globalTough")?;
    let c = s.card()?;
    let pk = card::pid(g, c)?.to_string();
    php::push(php::get_mut(&mut gt, &pk), s.args.clone());
    globals::set(g, "globalTough", gt)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2020
fn doppelganger(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let mut extra = card::get(g, c, "extraDatas")?;
    php::set(&mut extra, "subtypes", card::get_subtypes(g, c)?);
    php::set(&mut extra, "typeline", card::get(g, c, "typeline")?);
    let target_id = php::get(&action_ctx_args(g, s), "cardId").clone();
    let t = cards::get(g, &target_id)?;
    let st = card::get_subtypes(g, t)?;
    card::set_property(g, c, "subtypes", st, true)?;
    let t = cards::get(g, &target_id)?;
    let tl = card::get(g, t, "typeline")?;
    card::set_property(g, c, "typeline", tl, true)?;
    card::set_property(g, c, "extraDatas", extra, true)?;
    g.notify("refreshCard", json!({"cardId": c}));
    // Gain boost
    let mut max = json!(0);
    let t = cards::get(g, &target_id)?;
    let biomes = card::get_biomes(g, t, true, false)?;
    for v in php::vals(&biomes) {
        if php::gt(v, &max) {
            max = v.clone();
        }
    }
    if php::gt(&max, &json!(0)) {
        s.insert(g, &ft::action("Gain", json!({"cardId": c, "type": "boost", "n": max}), json!({"sourceId": c})))?;
    }
    Ok(Out::Break)
}

fn action_ctx_args(g: &Game, s: &Se) -> Value {
    crate::models::action::get_ctx_args(g, s.ctx)
}

// PHP: Actions/SpecialEffect.php:2046
fn reset_card(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let extra = card::get(g, c, "extraDatas")?;
    card::set_property(g, c, "subtypes", php::get(&extra, "subtypes").clone(), true)?;
    card::set_property(g, c, "typeline", php::get(&extra, "typeline").clone(), true)?;
    g.notify("refreshCard", json!({"cardId": c}));
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2120
fn next_charact_in_expedition_boost(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut n = php::coalesce(php::get(&s.args, "n"), &json!(1)).clone();
    if php::loose_eq(php::coalesce(php::get(&s.args, "X"), &json!("")), &json!("paidMana")) {
        let nd = s.node()?;
        let parent = crate::core::node::parent(g, nd).ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))?;
        let pa = crate::core::node::to_array(g, parent);
        n = php::geti(php::get(php::geti(php::get(&pa, "childs"), 0), "actionResolutionArgs"), 0).clone();
    }
    let mut boosts = globals::get(g, "nextCharacterInExpeditionBoost")?;
    let c = s.card()?;
    let pk = card::pid(g, c)?.to_string();
    let loc = card::location(g, c)?;
    let cur = php::coalesce(php::get(php::get(&boosts, &pk), &loc), &json!(0)).clone();
    php::set(php::get_mut(&mut boosts, &pk), &loc, add(&n, &cur));
    globals::set(g, "nextCharacterInExpeditionBoost", boosts)?;
    let active = players::get_active_id(g);
    g.notify("message", json!({"msg": "${player_name} will boost next character played in ${card_name}'s expedition", "player_id": active, "cardId": c}));
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2391
fn add_to_current_rolls(g: &mut Game, s: &mut Se) -> R<Out> {
    let rolls = globals::get(g, "diceRolls")?;
    let n = php::intval(php::coalesce(php::get(&s.args, "n"), &json!(1)));
    if !php::empty(&rolls) && n > 0 {
        let mut new_rolls: Vec<Value> = php::vals(&rolls).into_iter().cloned().collect();
        for r in php::vals(&rolls) {
            new_rolls.push(add(r, &json!(n)));
        }
        // sort(SORT_NUMERIC) then array_unique(SORT_NUMERIC): the first of equal values is kept
        new_rolls.sort_by(|a, b| php::floatval(a).partial_cmp(&php::floatval(b)).unwrap_or(std::cmp::Ordering::Equal));
        let mut out: Vec<Value> = vec![];
        for r in new_rolls {
            if !out.iter().any(|x| php::floatval(x) == php::floatval(&r)) {
                out.push(r);
            }
        }
        globals::set(g, "diceRolls", Value::Array(out))?;
    }
    Ok(Out::Break)
}
