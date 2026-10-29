//! SpecialEffect cases that insert flows: effects for every player, mass discards and
//! statuses, token invocations, Man in the Maze, ascension, control changes.

use super::{add, ctype, ext, gain, gigantic, has_subtype, player_of, req_player, type_in, Case, Out, Se, STORMS};
use crate::core::globals;
use crate::game::{CardId, Game, PId};
use crate::helpers::{conditions, conditions_extra, ft, utils};
use crate::managers::{cards, meeples, players};
use crate::models::{action, card, player};
use crate::php;
use crate::R;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(super) fn case_of(effect: &str) -> Option<Case> {
    let f: Case = match effect {
        "activateAllPermanents" => activate_all_permanents,
        "activateAllOtherCharacters" => activate_all_other_characters,
        "discardAllHand" => |g, s| discard_all(g, s, "allHand"),
        "discardAllHandReserve" => |g, s| discard_all(g, s, "allHandReserve"),
        "discardAllReserve" => |g, s| discard_all(g, s, "allReserve"),
        "invokeOrdisRecruitBureaucrat" => invoke_ordis_recruit_bureaucrat,
        "invokeManaMothPerStigmaInDiscard" => invoke_mana_moth_per_stigma_in_discard,
        "AllPlayersSacrifice1" => all_players_sacrifice_1,
        "eachPlayerOptionalResupply" => each_player_optional_resupply,
        "eachPlayerResupply" => each_player_resupply,
        "fleetingAllCharacters" => fleeting_all_characters,
        "sleepingAllCharactersinExpedition" => sleeping_all_characters_in_expedition,
        "sendToReserveCharactersAndExhaustInExpedition" => send_to_reserve_characters_and_exhaust_in_expedition,
        "readyAllReserve" => ready_all_reserve,
        "exhaustAllCards" => exhaust_all_cards,
        "playAll1Card" => play_all_1_card,
        "eachPlayerOptionalHandReserve" => each_player_optional_hand_reserve,
        "eachPlayerOptionalHandReserveDraw" => each_player_optional_hand_reserve_draw,
        "eachPlayerExhaust" => each_player_exhaust,
        "eachPlayerAsleep" => each_player_asleep,
        "ManInTheMaze" => |g, s| man_in_the_maze(g, s, false),
        "ManInTheMazeRare" => man_in_the_maze_rare,
        "ManInTheMazeYzmir" => |g, s| man_in_the_maze(g, s, true),
        "manInTheMazeUnique" => man_in_the_maze_unique,
        "boostAndRemoveFromExpedition" => boost_and_remove_from_expedition,
        "invokeXRecruitReserve" => invoke_x_recruit_reserve,
        "invokeRecruitBehind" | "invokeBrassbugBehind" => invoke_behind,
        "doCounter1" => do_counter_1,
        "doEachBoost" => do_each_boost,
        "hunger" => hunger,
        "ascend" => ascend,
        "ascendOnLeave" => ascend_on_leave,
        "allCharacterFleeting" => |g, s| all_character_fleeting(g, s, false),
        "allOtherCharacterFleeting" => |g, s| all_character_fleeting(g, s, true),
        "allPass" => all_pass,
        "defect" => defect,
        "discardAllAndBackward" => discard_all_and_backward,
        "copyGift" => copy_gift,
        "sacrificeHighestCharacter" | "sacrificeHighestCharacterPermanent" => sacrifice_highest_character,
        "tiktok" => tiktok,
        "invokeRecruitOnAscendedExpeditions" => invoke_recruit_on_ascended_expeditions,
        "doEachCardInDiscardPileSourceName" => do_each_card_in_discard_pile_source_name,
        "swapBoostsWithSource" => swap_boosts_with_source,
        "targetOpponentOptionalExhaustedResupply" => target_opponent_optional_exhausted_resupply,
        _ => return None,
    };
    Some(f)
}

/// The players from the active one, in table order (`do { ... } while` on `Players::getNext`)
fn players_from_active(g: &Game) -> R<Vec<PId>> {
    let active = req_player(players::get_active(g))?;
    let mut out = vec![];
    let mut p = active;
    loop {
        out.push(p);
        p = req_player(players::get_next(g, p)?)?;
        if p == active {
            break;
        }
    }
    Ok(out)
}

// PHP: Actions/SpecialEffect.php:439
fn activate_all_permanents(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut childs = vec![];
    for x in player::get_permanents(g, p)? {
        let xp = card::pid(g, x)?;
        childs.push(json!({"action": "ActivateEffect", "optional": true, "args": {"cardId": x}, "sourceId": x, "pId": xp}));
    }
    s.push_parallel(g, childs)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:453
fn activate_all_other_characters(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut childs = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if php::loose_ne(&ctype(g, x)?, &json!("character")) || php::loose_eq(&json!(x), &s.source_id(g)?) {
            continue;
        }
        if php::empty(&card::get(g, x, "effectPlayed")?) {
            continue;
        }
        let xp = card::pid(g, x)?;
        childs.push(json!({"action": "ActivateEffect", "optional": true, "args": {"cardId": x}, "sourceId": x, "pId": xp}));
    }
    s.push_parallel(g, childs)?;
    Ok(Out::Break)
}

/// `discardAllHand` / `discardAllHandReserve` / `discardAllReserve`
// PHP: Actions/SpecialEffect.php:673
fn discard_all(g: &mut Game, s: &mut Se, special: &str) -> R<Out> {
    let mut nodes = vec![];
    for p in players::get_all(g) {
        nodes.push(ft::action("Discard", json!({"pId": p, "special": special}), php::arr()));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:861
fn invoke_ordis_recruit_bureaucrat(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let _pid = player_of(g, c)?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if has_subtype(g, x, "bureaucrat")? {
            n += 1;
        }
    }
    if n != 0 {
        let mut nodes = vec![];
        for _ in 0..n {
            nodes.push(ft::action("InvokeToken", json!({"pId": "source", "tokenType": "OD_Common_OrdisRecruit", "targetLocation": ["source"]}), json!({"sourceId": c})));
        }
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:884
fn invoke_mana_moth_per_stigma_in_discard(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let pid = card::pid(g, c)?;
    let mut nodes = vec![];
    for d in cards::get_filtered(g, &json!(pid), &json!("discard"), &Value::Null, false)? {
        if !conditions_extra::is_card_matching_search_name(g, d, &json!("YZ_115"))? {
            continue;
        }
        nodes.push(ft::action("InvokeToken", json!({"pId": "source", "tokenType": "YZ_Common_ManaMoth", "targetLocation": STORMS}), json!({"sourceId": c})));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:945
fn all_players_sacrifice_1(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in players_from_active(g)? {
        let c = s.card()?;
        nodes.push(ft::action(
            "Target",
            json!({"targetPlayer": "me", "targetType": ["character", "token"], "effect": ft::action("Discard", json!({"desc": "sacrifice"}), php::arr())}),
            json!({"pId": p, "sourceId": c}),
        ));
    }
    s.insert_seq(g, nodes)?;
    g.notify("message", json!({"msg": "All players must sacrifice 1 character", "cardId": s.card}));
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:966
fn each_player_optional_resupply(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in players::get_all(g) {
        let sid = s.source_id(g)?;
        nodes.push(json!({"type": "seq", "optional": true, "pId": p, "childs": [ft::action("Resupply", php::arr(), json!({"pId": p, "sourceId": sid}))]}));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:978
fn each_player_resupply(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in players::get_all(g) {
        if !player::has_deck_cards(g, p)? {
            continue;
        }
        let sid = s.source_id(g)?;
        nodes.push(json!({"type": "seq", "pId": p, "childs": [ft::action("Resupply", php::arr(), json!({"pId": p, "sourceId": sid}))]}));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// Characters and tokens without the status
fn char_without(g: &mut Game, x: CardId, token: &str) -> R<bool> {
    Ok(type_in(g, x, &["character", "token"])? && !card::has_token(g, x, token)?)
}

// PHP: Actions/SpecialEffect.php:994
fn fleeting_all_characters(g: &mut Game, s: &mut Se) -> R<Out> {
    let p = req_player(players::get_active(g))?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if !char_without(g, x, "fleeting")? {
            continue;
        }
        nodes.push(gain(json!(x), "fleeting", json!(1)));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// Not in the expedition (and not gigantic)
fn outside(g: &mut Game, x: CardId, expedition: &Value) -> R<bool> {
    Ok(php::loose_ne(&json!(card::location(g, x)?), expedition) && !gigantic(g, x)?)
}

// PHP: Actions/SpecialEffect.php:1008
fn sleeping_all_characters_in_expedition(g: &mut Game, s: &mut Se) -> R<Out> {
    let expedition = s.ctx_arg(g, "expedition");
    let pv = s.ctx_arg(g, "player");
    let p = req_player(players::get_by_value(g, &pv))?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if !char_without(g, x, "asleep")? || outside(g, x, &expedition)? {
            continue;
        }
        nodes.push(gain(json!(x), "asleep", json!(1)));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1023
fn send_to_reserve_characters_and_exhaust_in_expedition(g: &mut Game, s: &mut Se) -> R<Out> {
    let expedition = s.ctx_arg(g, "expedition");
    let pv = s.ctx_arg(g, "player");
    let c = s.card()?;
    let owner = card::pid(g, c)?;
    let p = req_player(players::get_by_value(g, &pv))?;
    let mut nodes = vec![];
    let mut exhaust = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if !type_in(g, x, &["character", "token"])? || outside(g, x, &expedition)? {
            continue;
        }
        let sid = s.source_id(g)?;
        nodes.push(ft::action("Discard", json!({"cardId": x, "destination": "reserve"}), json!({"sourceId": sid, "pId": owner})));
        exhaust.push(ft::action("Exhaust", json!({"cardId": x}), json!({"sourceId": sid, "pId": owner})));
    }
    if !nodes.is_empty() {
        s.insert(g, &ft::seq(vec![ft::seq(nodes), ft::seq(exhaust)]))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1047
fn ready_all_reserve(g: &mut Game, s: &mut Se) -> R<Out> {
    let p = req_player(players::get_active(g))?;
    let mut nodes = vec![];
    for x in player::get_reserve_cards(g, p)? {
        if !card::is_tapped(g, x)? {
            continue;
        }
        nodes.push(ft::action("Ready", json!({"cardId": x}), json!({"sourceId": s.source_id(g)?})));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1060
fn exhaust_all_cards(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    let c = s.card()?;
    let cpid = card::pid(g, c)?;
    for p in players::get_all(g) {
        for x in player::get_reserve_cards(g, p)? {
            if card::is_tapped(g, x)? {
                continue;
            }
            nodes.push(ft::action("Exhaust", json!({"cardId": x}), json!({"sourceId": s.source_id(g)?, "pId": cpid})));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// `Players::getTurnOrder(Players::getActiveId())`
fn turn_order_from_active(g: &mut Game) -> R<Vec<PId>> {
    let a = players::get_active_id(g);
    players::get_turn_order(g, Some(a))
}

// PHP: Actions/SpecialEffect.php:1075
fn play_all_1_card(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in turn_order_from_active(g)? {
        nodes.push(ft::action(
            "ChooseAssignment",
            json!({"actions": ["play"], "maxHandCost": 3, "free": true}),
            json!({"pId": p, "optional": true, "sourceId": s.source_id(g)?}),
        ));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

fn each_player_hand_reserve(g: &mut Game, s: &mut Se, effect: Value) -> R<Out> {
    let mut nodes = vec![];
    for p in turn_order_from_active(g)? {
        nodes.push(ft::action(
            "Target",
            json!({
                "targetType": ["character", "spell", "permanent"],
                "targetPlayer": "me",
                "upTo": true,
                "targetLocation": ["hand"],
                "effect": effect,
            }),
            json!({"optional": true, "pId": p, "sourceId": s.source_id(g)?}),
        ));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1085
fn each_player_optional_hand_reserve(g: &mut Game, s: &mut Se) -> R<Out> {
    each_player_hand_reserve(g, s, ft::discard_to_reserve())
}

// PHP: Actions/SpecialEffect.php:1104
fn each_player_optional_hand_reserve_draw(g: &mut Game, s: &mut Se) -> R<Out> {
    each_player_hand_reserve(g, s, ft::seq(vec![ft::discard_to_reserve(), ft::action("Draw", json!({"players": "me"}), php::arr())]))
}

// PHP: Actions/SpecialEffect.php:1123
fn each_player_exhaust(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in turn_order_from_active(g)? {
        nodes.push(ft::action(
            "Target",
            json!({
                "targetType": ["character", "spell", "permanent"],
                "targetPlayer": "me",
                "targetLocation": ["reserve"],
                "effect": ft::action("Exhaust", php::arr(), php::arr()),
            }),
            json!({"pId": p, "sourceId": s.source_id(g)?}),
        ));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1152
fn each_player_asleep(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in turn_order_from_active(g)? {
        nodes.push(ft::action(
            "Target",
            json!({"targetType": ["character", "token"], "targetPlayer": "me", "effect": gain(json!("EFFECT"), "asleep", json!(1))}),
            json!({"pId": p, "sourceId": s.source_id(g)?}),
        ));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

/// Removes every boost of the characters and tokens in play or in Reserve; the number removed
// PHP: Actions/SpecialEffect.php:1381
fn remove_all_boosts(g: &mut Game) -> R<usize> {
    let ct = json!(["character", "token"]);
    let played = cards::get_played_cards(g, &Value::Null, &ct)?;
    let reserve = cards::get_reserve_cards(g, &Value::Null, &ct)?;
    let mut deleted = 0;
    for l in crate::helpers::collection::merge(&played, &reserve) {
        let ids: Vec<i64> = meeples::get_in_location(g, &format!("card-{}", l), None)?.into_iter().filter(|m| php::loose_eq(&json!(g.meeples[m].kind), &json!("boost"))).collect();
        if !ids.is_empty() {
            meeples::delete(g, &json!(ids))?;
            deleted += ids.len();
            g.notify("looseMeeples", json!({"type": "boost", "cardId": l, "meepleIds": ids}));
        }
    }
    Ok(deleted)
}

/// `ManInTheMaze` / `ManInTheMazeYzmir` (the 9+ effect moves the opponent back)
// PHP: Actions/SpecialEffect.php:1380
fn man_in_the_maze(g: &mut Game, s: &mut Se, yzmir: bool) -> R<Out> {
    let deleted = remove_all_boosts(g)?;
    match deleted {
        0 => {}
        1..=4 => {
            let c = s.card()?;
            s.insert(g, &gain(json!(c), "boost", json!(2)))?;
        }
        5..=8 => {
            let sid = s.source_id(g)?;
            s.insert(g, &ft::action("ChooseAssignment", json!({"actions": ["play"], "maxHandCost": 3, "free": true}), json!({"sourceId": sid})))?;
        }
        _ => {
            let sid = s.source_id(g)?;
            let args = if yzmir {
                json!({"n": -1, "expedition": ["EFFECT"], "pId": "opponent"})
            } else {
                let c = s.card()?;
                json!({"expedition": ["EFFECT"], "pId": card::pid(g, c)?})
            };
            s.insert(g, &ft::action("MoveExpedition", args, json!({"sourceId": sid})))?;
        }
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1413
fn man_in_the_maze_rare(g: &mut Game, s: &mut Se) -> R<Out> {
    let deleted = remove_all_boosts(g)?;
    let mut nodes = vec![];
    if deleted >= 1 {
        let c = s.card()?;
        nodes.push(gain(json!(c), "boost", json!(2)));
    }
    if deleted >= 5 {
        nodes.push(ft::action("ChooseAssignment", json!({"actions": ["play"], "maxHandCost": 3, "free": true}), json!({"sourceId": s.source_id(g)?})));
    }
    if deleted >= 9 {
        let c = s.card()?;
        let pid = card::pid(g, c)?;
        nodes.push(ft::action("MoveExpedition", json!({"expedition": ["EFFECT"], "pId": pid}), json!({"sourceId": s.source_id(g)?})));
    }
    if !nodes.is_empty() {
        s.insert(g, &json!({"type": "parallel", "noIndependent": true, "childs": nodes}))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1472
fn man_in_the_maze_unique(g: &mut Game, s: &mut Se) -> R<Out> {
    let deleted = remove_all_boosts(g)?;
    let mut nodes = vec![];
    if deleted >= 1 {
        nodes.push(php::get(&s.args, "1+").clone());
    }
    if deleted >= 4 {
        nodes.push(php::get(&s.args, "4+").clone());
    }
    if deleted >= 9 {
        nodes.push(php::get(&s.args, "9+").clone());
    }
    if !nodes.is_empty() {
        for node in nodes.iter_mut() {
            let sid = s.source_id(g)?;
            php::set(node, "sourceId", sid.clone());
            let c = s.card()?;
            let pid = card::pid(g, c)?;
            php::set(node, "pId", json!(pid));
            if php::isset(node, "childs") {
                *node = utils::tag_tree(node, &json!({"sourceId": sid, "pId": pid}), false);
            }
        }
        s.insert(g, &json!({"type": "parallel", "noIndependent": true, "childs": nodes}))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1545
fn boost_and_remove_from_expedition(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut exp_cards = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        let keep = type_in(g, x, &["token", "character"])? && {
            let cl = card::location(g, c)?;
            let xl = card::location(g, x)?;
            cl == xl || (gigantic(g, c)? && STORMS.contains(&xl.as_str()))
        } && c != x;
        if keep {
            exp_cards.push(x);
        }
    }
    if !exp_cards.is_empty() {
        let mut nodes = vec![];
        for e in &exp_cards {
            nodes.push(ft::action("Discard", json!({"destination": "reserve", "cardId": e}), json!({"sourceId": c})));
        }
        s.insert(g, &ft::seq(vec![gain(json!(c), "boost", json!(exp_cards.len())), ft::seq(nodes)]))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1565
fn invoke_x_recruit_reserve(g: &mut Game, s: &mut Se) -> R<Out> {
    let pv = s.ctx_arg(g, "pId");
    let expedition = s.ctx_arg(g, "expedition");
    let p = req_player(players::get_by_value(g, &pv))?;
    let n = player::get_reserve_cards(g, p)?.len();
    if n > 0 {
        let c = s.card()?;
        let target_pid = s.ctx_arg(g, "player");
        s.insert(
            g,
            &ft::action("InvokeToken", json!({"pId": target_pid, "tokenType": "OD_Common_OrdisRecruit", "n": n, "targetLocation": [expedition]}), json!({"sourceId": c})),
        )?;
    }
    Ok(Out::Break)
}

/// `invokeRecruitBehind` / `invokeBrassbugBehind`
// PHP: Actions/SpecialEffect.php:1602
fn invoke_behind(g: &mut Game, s: &mut Se) -> R<Out> {
    let winners = players::get_winning_player_by_storms(g)?;
    let mut nodes = vec![];
    let invoke = if s.effect == "invokeBrassbugBehind" { "AX_Common_Brassbug" } else { "OD_Common_OrdisRecruit" };
    for storm in STORMS {
        let win = php::coalesce(php::get(&winners, storm), &Value::Null).clone();
        let c = s.card()?;
        let cpid = json!(card::pid(g, c)?);
        let behind = win.is_null() || php::loose_eq(&win, &json!(-1)) || php::loose_ne(&win, &cpid);
        let ok = (behind && !globals::is(g, "tieBreakerMode")?) || globals::is(g, "tieBreakerMode")? || {
            let p = player_of(g, c)?;
            player::has_override_behind(g, p, storm)?
        };
        if ok {
            nodes.push(ft::action("InvokeToken", json!({"pId": cpid, "tokenType": invoke, "targetLocation": [storm]}), json!({"sourceId": c})));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// `$count` copies of `$args['effect']` (with the card's controller), tagged with the source
fn repeat_effect(g: &mut Game, s: &mut Se, count: &Value) -> R<Out> {
    let c = s.card()?;
    let mut effect = php::get(&s.args, "effect").clone();
    let p = player_of(g, c)?;
    php::set(&mut effect, "pId", json!(p));
    let mut nodes = vec![];
    let mut i = 0;
    while php::lt(&json!(i), count) {
        nodes.push(effect.clone());
        i += 1;
    }
    let tagged = utils::tag_tree(&json!({"childs": nodes}), &json!({"sourceId": c}), false);
    s.insert(g, &json!({"type": "seq", "childs": php::get(&tagged, "childs")}))?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1666
fn do_counter_1(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let data = card::get(g, c, "extraDatas")?;
    let count = add(php::coalesce(php::get(&data, "counter"), &json!(0)), &json!(1));
    repeat_effect(g, s, &count)
}

// PHP: Actions/SpecialEffect.php:1679
fn do_each_boost(g: &mut Game, s: &mut Se) -> R<Out> {
    let e = s.event(g)?;
    let count = php::coalesce(php::get(&e, "boost"), &json!(0)).clone();
    repeat_effect(g, s, &count)
}

// PHP: Actions/SpecialEffect.php:2498
fn do_each_card_in_discard_pile_source_name(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let e = s.event(g)?;
    let count = conditions::count_cards_in_discard_pile_source_name(g, Some(c), &e, &[])?;
    repeat_effect(g, s, &count)
}

// PHP: Actions/SpecialEffect.php:1761
fn hunger(g: &mut Game, s: &mut Se) -> R<Out> {
    let all = cards::get_all(g);
    let to_discard: Vec<CardId> = all.into_iter().filter(|c| ["stormRight", "stormLeft", "landmark", "reserve"].contains(&g.cards[c].location.as_str())).collect();
    let mut floral: HashSet<(&str, PId, String)> = HashSet::new();
    let mut nodes = vec![];
    for t in players::get_all(g) {
        if ext::has_protect_boosted_in_expedition(g, t, "stormLeft", false)? {
            floral.insert(("boosted", t, "stormLeft".to_string()));
        }
        if ext::has_protect_boosted_in_expedition(g, t, "stormRight", false)? {
            floral.insert(("boosted", t, "stormRight".to_string()));
        }
        if ext::has_protect_anchored_in_expedition(g, t, "stormLeft", false)? {
            floral.insert(("anchored", t, "stormLeft".to_string()));
        }
        if ext::has_protect_anchored_in_expedition(g, t, "stormRight", false)? {
            floral.insert(("anchored", t, "stormRight".to_string()));
        }
    }
    for d in to_discard {
        let dp = card::pid(g, d)?;
        let c = s.card()?;
        if d == c {
            continue;
        }
        let loc = card::location(g, d)?;
        let other = if loc == "stormLeft" { "stormRight" } else { "stormLeft" };
        if STORMS.contains(&loc.as_str()) && card::has_token(g, d, "boost")? && type_in(g, d, &["token", "character"])? {
            if floral.contains(&("boosted", dp, loc.clone())) || (gigantic(g, d)? && floral.contains(&("boosted", dp, other.to_string()))) {
                g.notify("message", json!({"msg": "${card_name} is not discarded but loose <BOOST> instead", "cardId": d}));
                nodes.push(json!({"action": "Loose", "args": {"cardId": d, "type": "boost", "n": 99}}));
                continue;
            }
        }
        if STORMS.contains(&loc.as_str()) && card::has_token(g, d, "anchored")? && type_in(g, d, &["token", "character"])? {
            if floral.contains(&("anchored", dp, loc.clone())) || (gigantic(g, d)? && floral.contains(&("anchored", dp, other.to_string()))) {
                g.notify("message", json!({"msg": "${card_name} is not discarded but loose <ANCHORED> instead", "cardId": d}));
                nodes.push(json!({"action": "Loose", "args": {"cardId": d, "type": "anchored"}}));
                continue;
            }
        }
        nodes.push(ft::action("Discard", json!({"cardId": d}), php::arr()));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// `$this->getCtxArg('pId') ?? $card->getPlayer()->getId()`, then `getCtxArg('player') ?? ...`
fn ascend_player(g: &mut Game, s: &Se) -> R<Value> {
    let mut p = s.ctx_arg(g, "pId");
    if p.is_null() {
        let c = s.card()?;
        p = json!(player_of(g, c)?);
    }
    let pl = s.ctx_arg(g, "player");
    Ok(if pl.is_null() { p } else { pl })
}

// PHP: Actions/SpecialEffect.php:1824
fn ascend(g: &mut Game, s: &mut Se) -> R<Out> {
    let pv = ascend_player(g, s)?;
    let mut expedition = s.ctx_arg(g, "expedition");
    let o = players::get_by_value(g, &pv);
    let op = req_player(o)?;
    let resupply_if_ascended = ext::has_resupply_if_ascended(g, op)?;
    let boost_if_ascended = ext::has_boost_if_ascended(g, op)?;
    // manage my expedition
    if php::loose_eq(&expedition, &json!("source")) {
        let c = s.card()?;
        expedition = json!(card::location(g, c)?);
    }
    // Source may have left play mid-sequence (e.g. returned to hand): no invalid ascend meeple
    if !php::in_list(&expedition, &STORMS) {
        return Ok(Out::Break);
    }
    let exp = php::strval(&expedition);
    if !player::is_ascended(g, op, &exp)? {
        let m = meeples::single_create(g, &json!({"player_id": pv, "location": exp, "nbr": 1, "type": "ascend"}))?;
        g.notify("ascend", json!({"meepleId": m, "player_id": op, "cardId": s.card, "expedition": exp}));
    } else {
        if resupply_if_ascended {
            s.insert(g, &ft::action("Resupply", php::arr(), php::arr()))?;
        }
        if boost_if_ascended {
            for x in player::get_played_cards(g, op, &Value::Null)? {
                if card::is(g, x, "boostIfAscended")? {
                    s.insert(g, &gain(json!(x), "boost", json!(1)))?;
                }
            }
        }
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1864
fn ascend_on_leave(g: &mut Game, s: &mut Se) -> R<Out> {
    let pv = ascend_player(g, s)?;
    let o = players::get_by_value(g, &pv);
    let expedition = s.ctx_arg(g, "expedition");
    // Check event in case of leaving expedition
    let event = action::get_event_recursive(g, s.ctx)?;
    let src = php::get(&event, "from").clone();
    let real = if php::loose_eq(&expedition, &json!("oppositeSource")) {
        if php::loose_eq(&src, &json!("stormLeft")) {
            json!("stormRight")
        } else {
            json!("stormLeft")
        }
    } else {
        src
    };
    let op = req_player(o)?;
    if !player::is_ascended(g, op, &php::strval(&real))? {
        let m = meeples::single_create(g, &json!({"player_id": pv, "location": real, "nbr": 1, "type": "ascend"}))?;
        g.notify("ascend", json!({"meepleId": m, "player_id": op, "cardId": s.card, "expedition": real}));
    }
    Ok(Out::Break)
}

/// `allCharacterFleeting` / `allOtherCharacterFleeting` (the source excluded)
// PHP: Actions/SpecialEffect.php:1902
fn all_character_fleeting(g: &mut Game, s: &mut Se, exclude_source: bool) -> R<Out> {
    let mut nodes = vec![];
    for p in players::get_all(g) {
        for x in player::get_played_cards(g, p, &Value::Null)? {
            if !char_without(g, x, "fleeting")? || (exclude_source && php::loose_eq(&json!(x), &s.source_id(g)?)) {
                continue;
            }
            nodes.push(gain(json!(x), "fleeting", json!(1)));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1930
fn all_pass(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    let skipped = globals::get(g, "skippedPlayers")?;
    let c = s.card()?;
    let first = card::pid(g, c)?;
    for p in players::get_turn_order(g, Some(first))? {
        if !php::isset(&skipped, &p.to_string()) {
            nodes.push(ft::action("EndAfternoon", php::arr(), json!({"pId": p})));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2053
fn defect(g: &mut Game, s: &mut Se) -> R<Out> {
    let d = s.get_card(g)?;
    // TODO (PHP): multiplayer
    let dp = player_of(g, d)?;
    let mut new_pid = json!(players::get_next_id(g, dp)?);
    let mut extra = card::get(g, d, "extraDatas")?;
    if php::isset(&s.args, "takeControl") && php::loose_eq(php::get(&s.args, "takeControl"), &json!(true)) {
        let c = s.card()?;
        new_pid = json!(card::pid(g, c)?);
    }
    if !php::isset(&extra, "pId") {
        php::set(&mut extra, "pId", json!(card::pid(g, d)?));
    }
    if php::isset(&s.args, "moveToMe") && php::loose_eq(php::get(&s.args, "moveToMe"), &json!(true)) && !php::in_array(&json!("landmark"), &card::get_subtypes(g, d)?) {
        let c = s.card()?;
        let loc = card::location(g, c)?;
        card::set_location(g, d, &loc)?;
    }
    card::set_property(g, d, "extraDatas", extra, true)?;
    card::set_pid(g, d, &new_pid)?;
    g.notify("defect", json!({"cardId": d, "player_id": card::pid(g, d)?, "sourceId": s.card}));
    let np = player_of(g, d)?;
    let loc = card::location(g, d)?;
    let args = json!({
        "cardId": d,
        "playCard": true,
        "cardType": card::get(g, d, "type")?,
        "additionalType": card::get_additional_type(g, d)?,
        "cardSubtypes": card::get_subtypes(g, d)?,
        "from": loc,
        "to": loc,
        "locationPId": card::pid(g, d)?,
        "token": card::is_token(g, d)?,
        "defect": true,
    });
    s.after_listeners(g, np, args, true, Some("MoveCard"))?;
    Ok(Out::Break)
}

/// Sakarabru's fury
// PHP: Actions/SpecialEffect.php:2084
fn discard_all_and_backward(g: &mut Game, s: &mut Se) -> R<Out> {
    let pv = s.arg(g, "player")?;
    let p = req_player(players::get_by_value(g, &pv))?;
    let expedition = s.arg(g, "expedition")?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if php::loose_ne(&ctype(g, x)?, &json!("character")) {
            continue;
        }
        if php::loose_eq(&json!(card::location(g, x)?), &expedition) || (php::in_list(&expedition, &STORMS) && gigantic(g, x)?) {
            nodes.push(ft::action("Discard", json!({"cardId": x}), json!({"sourceId": s.source_id(g)?})));
        }
    }
    nodes.push(ft::action("MoveExpedition", json!({"n": -1, "forceExpedition": [p, expedition]}), php::arr()));
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2321
fn copy_gift(g: &mut Game, s: &mut Se) -> R<Out> {
    let event = action::get_event_recursive(g, s.ctx)?;
    let a = php::get(&event, "action").clone();
    if php::loose_eq(&a, &json!("InvokeToken")) {
        let c = s.card()?;
        let tl = if php::loose_eq(php::get(&event, "to"), &json!("landmark")) { json!(["landmark"]) } else { json!(STORMS) };
        s.insert(g, &ft::action("InvokeToken", json!({"pId": "source", "tokenType": php::get(&event, "invoked"), "targetLocation": tl, "targetPlayer": "me"}), json!({"sourceId": c})))?;
    } else if php::loose_eq(&a, &json!("Draw")) {
        let c = s.card()?;
        s.insert(g, &ft::action("Draw", json!({"n": 1, "players": "me"}), json!({"sourceId": c})))?;
    } else if php::loose_eq(&a, &json!("Resupply")) {
        let c = s.card()?;
        s.insert(g, &ft::action("Resupply", json!({"n": 1, "exhausted": php::get(&event, "exhausted")}), json!({"sourceId": c})))?;
    }
    Ok(Out::Break)
}

/// `return []` when the opponent has no target: takeAction then calls the missing
/// actSpecialEffect method (an error, as in PHP)
// PHP: Actions/SpecialEffect.php:2350
fn sacrifice_highest_character(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let opp = req_player(players::get_next(g, p)?)?;
    let targets: Vec<&str> = if s.effect == "sacrificeHighestCharacterPermanent" { vec!["character", "permanent"] } else { vec!["character"] };
    let mut max_cost = json!(0);
    let mut max_ids: Vec<CardId> = vec![];
    for o in player::get_played_cards(g, opp, &Value::Null)? {
        if !type_in(g, o, &targets)? {
            continue;
        }
        let cost = if card::has_token(g, o, "fleeting")? { card::get(g, o, "costReserve")? } else { card::get(g, o, "costHand")? };
        if php::lt(&max_cost, &cost) {
            max_cost = cost;
            max_ids = vec![o];
        } else if php::loose_eq(&max_cost, &cost) {
            max_ids.push(o);
        }
    }
    if max_ids.is_empty() {
        return Ok(Out::Done(Some(php::arr())));
    } else if max_ids.len() == 1 {
        s.insert(g, &ft::action("Discard", json!({"cardId": max_ids[0], "desc": "sacrifice"}), json!({"pId": opp, "sourceId": c})))?;
    } else {
        s.insert(
            g,
            &ft::action(
                "Target",
                json!({"effect": ft::action("Discard", json!({"cardId": max_ids[0], "desc": "sacrifice"}), php::arr()), "cards": max_ids, "targetType": targets}),
                json!({"pId": opp, "sourceId": c}),
            ),
        )?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2403
fn tiktok(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for p in players_from_active(g)? {
        let c = s.card()?;
        let exhaust = ft::seq(vec![ft::action("Exhaust", json!({"cardId": "EFFECT"}), json!({"sourceId": c}))]);
        nodes.push(json!({
            "type": "xor",
            "pId": p,
            "sourceId": c,
            "childs": [
                ft::action("Target", json!({"targetType": ["permanent"], "targetPlayer": "me", "isNotTapped": true, "sourceId": c, "effect": exhaust}), json!({"sourceId": c})),
                ft::action("Target", json!({"targetType": ["spell", "character", "permanent"], "targetPlayer": "me", "isNotTapped": true, "targetLocation": ["reserve"], "effect": exhaust}), json!({"sourceId": c})),
            ],
        }));
    }
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

/// `$nodes` is never initialised in PHP (empty() of an undefined variable is true)
// PHP: Actions/SpecialEffect.php:2479
fn invoke_recruit_on_ascended_expeditions(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut nodes = vec![];
    for storm in STORMS {
        let c = s.card()?;
        let p = player_of(g, c)?;
        if player::is_ascended(g, p, storm)? {
            let pid = card::pid(g, c)?;
            nodes.push(ft::action("InvokeToken", json!({"pId": pid, "tokenType": "OD_Common_OrdisRecruit", "targetLocation": [storm]}), json!({"sourceId": c})));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// #238924: Uniques with effect Sap Duende on target
// PHP: Actions/SpecialEffect.php:2512
fn swap_boosts_with_source(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut picked = s.ctx_arg(g, "cardId");
    if picked == json!("me") || picked == json!("EFFECT") {
        picked = Value::Null;
    }
    if !picked.is_null() {
        s.insert(g, &ft::action("BoostExchange", json!({"cardId": picked}), json!({"sourceId": s.source_id(g)?})))?;
    }
    Ok(Out::Break)
}

/// #206754: inserted at runtime with the opponent pId so LeaveExpedition's tagTree cannot stamp
/// the controller onto the nested Resupply
// PHP: Actions/SpecialEffect.php:2523
fn target_opponent_optional_exhausted_resupply(g: &mut Game, s: &mut Se) -> R<Out> {
    let n = php::coalesce(php::get(&s.args, "n"), &json!(1)).clone();
    let active = req_player(players::get_active(g))?;
    let opp = players::get_next_id(g, active)?;
    let mut childs = vec![];
    let mut i = 0;
    while php::lt(&json!(i), &n) {
        childs.push(ft::action("Resupply", json!({"exhausted": true}), json!({"pId": opp, "sourceId": s.source_id(g)?})));
        i += 1;
    }
    s.insert(g, &json!({"type": "seq", "optional": true, "pId": opp, "childs": childs}))?;
    Ok(Out::Break)
}
