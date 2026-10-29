//! Actions/Discard.php: moves cards to `destination` (discard, reserve, hand, mana, top of deck;
//! tokens are destroyed) with the sacrifice / sabotage / Jinn / protection rules, firing the
//! Discard listeners with `cardsToListen`.

use crate::actions::target::storm_shims;
use crate::core::{engine, globals};
use crate::framework::ST_DISCARD;
use crate::game::{CardId, Game, PId};
use crate::game_methods;
use crate::helpers::{collection, ft};
use crate::managers::{cards, players};
use crate::models::action::{
    base_get_player, check_after_listeners, get_arg, get_event, get_source, get_source_id, insert_as_child, param, param_req, push_after_finishing_childs, resolve_action,
    ActionClass, Ctx,
};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Discard;

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

fn player_req(p: Option<PId>, method: &str) -> R<PId> {
    p.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// PHP 8 `$a + $b` on scalars: a non-numeric string is a TypeError
pub(crate) fn num_add(a: &Value, b: &Value) -> R<Value> {
    let num = |v: &Value| -> R<Value> {
        match v {
            Value::Null => Ok(json!(0)),
            Value::Bool(x) => Ok(json!(*x as i64)),
            Value::Number(_) => Ok(v.clone()),
            Value::String(s) if php::is_numeric_str(s) => Ok(if s.contains(['.', 'e', 'E']) { json!(php::floatval(v)) } else { json!(php::intval(v)) }),
            _ => Err(PhpError::new(format!("TypeError: Unsupported operand types: {} + {}", php::type_name(a), php::type_name(b)))),
        }
    };
    let (x, y) = (num(a)?, num(b)?);
    if x.is_i64() && y.is_i64() {
        Ok(json!(x.as_i64().unwrap() + y.as_i64().unwrap()))
    } else {
        Ok(json!(php::floatval(&x) + php::floatval(&y)))
    }
}

/// `isSacrifice()`
// PHP: Actions/Discard.php:73
fn is_sacrifice(a: &Discard, g: &Game, ctx: Ctx) -> R<bool> {
    Ok(php::loose_eq(&get_arg(a, g, ctx, "desc")?, &json!("sacrifice")))
}

/// `isSabotage()`
// PHP: Actions/Discard.php:78
fn is_sabotage(a: &Discard, g: &Game, ctx: Ctx) -> R<bool> {
    Ok(php::loose_eq(&get_arg(a, g, ctx, "desc")?, &json!("sabotage")))
}

/// `getDiscardControllerId()`
// PHP: Actions/Discard.php:83
fn get_discard_controller_id(a: &Discard, g: &mut Game, ctx: Ctx) -> R<Value> {
    // "Each player discards their hand/reserve" (Loki...): each player discards their own cards
    if php::in_list(&get_arg(a, g, ctx, "special")?, &["allHand", "allHandReserve", "allReserve"]) && !get_arg(a, g, ctx, "pId")?.is_null() {
        return get_arg(a, g, ctx, "pId");
    }
    if let Some(s) = get_source(g, ctx)? {
        return Ok(json!(card::pid(g, s)?));
    }
    Ok(json!(players::get_active_id(g)))
}

/// `cannotFulfillDiscard($player)`
// PHP: Actions/Discard.php:56
fn cannot_fulfill_discard(a: &Discard, g: &mut Game, ctx: Ctx) -> R<bool> {
    if php::loose_ne(&get_arg(a, g, ctx, "special")?, &json!("")) {
        return Ok(false);
    }
    let args = args_discard(a, g, ctx)?;
    let available = php::coalesce(php::get(php::get(php::get(&args, "_private"), "active"), "cards"), &php::arr()).clone();
    let n = num_add(php::get(&args, "n"), php::coalesce(php::get(&args, "nLandmarks"), &json!(0)))?;
    if php::truthy(&get_arg(a, g, ctx, "upTo")?) {
        return Ok(php::count_strict(&available)? == 0);
    }
    Ok(php::lt(&json!(php::count_strict(&available)?), &n))
}

/// `argsDiscard()`
// PHP: Actions/Discard.php:163
fn args_discard(a: &Discard, g: &mut Game, ctx: Ctx) -> R<Value> {
    let p = a.get_player(g, ctx)?;
    let card_id = get_arg(a, g, ctx, "cardId")?;
    let source = get_arg(a, g, ctx, "source")?;
    let card_ids: Value = if !card_id.is_null() {
        let ids = if php::is_arr(&card_id) { card_id.clone() } else { json!([card_id]) };
        // EFFECT targets the event card when available, else the source
        let event_card_id = php::get(&get_event(g, ctx)?, "cardId").clone();
        let source_id = get_source_id(g, ctx)?;
        let mut out = vec![];
        for c in php::vals(&ids) {
            let r = if php::loose_eq(c, &json!("me")) || php::loose_eq(c, &json!("source")) {
                source_id.clone()
            } else if php::loose_eq(c, &json!("EFFECT")) {
                if event_card_id.is_null() {
                    source_id.clone()
                } else {
                    event_card_id.clone()
                }
            } else if php::loose_eq(c, &json!("event")) {
                event_card_id.clone()
            } else {
                c.clone()
            };
            // Remove unresolved placeholders
            if !r.is_null() {
                out.push(r);
            }
        }
        Value::Array(out)
    } else if php::loose_eq(&source, &json!("hand")) {
        let p = player_req(p, "getHand")?;
        collection::ids(&player::get_hand(g, p, &Value::Null)?)
    } else if php::loose_eq(&source, &json!("reserve")) {
        let p = player_req(p, "getReserveCards")?;
        collection::ids(&player::get_reserve_cards(g, p)?)
    } else {
        let p = player_req(p, "getPlayedCards")?;
        collection::ids(&player::get_played_cards(g, p, &Value::Null)?)
    };
    let suffix = if a.is_optional(g, ctx, p)? { "CanPass" } else { "" };
    Ok(json!({
        "n": get_arg(a, g, ctx, "n")?,
        "source": source,
        "destination": get_arg(a, g, ctx, "destination")?,
        "descSuffix": suffix,
        "upTo": get_arg(a, g, ctx, "upTo")?,
        "_private": {"active": {"cards": card_ids}},
    }))
}

/// `actDiscard($cardIds, $automatic)`
// PHP: Actions/Discard.php:273
fn act_discard(a: &Discard, g: &mut Game, ctx: Ctx, card_ids: &Value, automatic: bool) -> R<Value> {
    let p = a.get_player(g, ctx)?;
    let args = args_discard(a, g, ctx)?;
    let pargs = php::get(php::get(&args, "_private"), "active").clone();

    // Sanity checks (bypass if automatic)
    if !automatic {
        let n = num_add(php::get(&args, "n"), php::coalesce(php::get(&args, "nLandmarks"), &json!(0)))?;
        let up_to = php::truthy(php::get(&args, "upTo"));
        let count = json!(php::count_strict(card_ids)?);
        if (!up_to && php::loose_ne(&count, &n)) || (up_to && php::gt(&count, &n)) {
            return Err(PhpError::new("You must select the correct number of cards. Should not happen"));
        }
        // `+` on lists: only the first list's indexes count
        let valid = php::union(
            &php::union(php::coalesce(php::get(&pargs, "cards"), &php::arr()), php::coalesce(php::get(&pargs, "reserveCards"), &php::arr())),
            php::coalesce(php::get(&pargs, "landmarkCards"), &php::arr()),
        );
        if !php::empty(&php::array_diff(card_ids, &valid)) {
            return Err(PhpError::new("You selected a card that should not be discarded. Should not happen"));
        }
    }

    // Discard them
    let mut cs: Vec<CardId> = cards::get_many(g, card_ids, true)?;
    let mut deleted_tokens: Vec<CardId> = vec![];
    let mut deleted_meeples: Vec<i64> = vec![];
    let mut cards_to_listen: Vec<CardId> = vec![];
    let mut original_location = String::new();
    let destination0 = php::get(&args, "destination").clone();

    for c in &cs {
        let loc = json!(card::location(g, *c)?);
        // the cards being discarded (but in Landmark or Storms) react
        if php::loose_eq(&destination0, &json!("discard")) && php::in_list(&loc, &["landmark", "stormLeft", "stormRight"]) {
            cards_to_listen.push(*c);
        } else if !php::in_list(&destination0, &["discard", "landmark", "stormLeft", "stormRight"]) {
            // only the cards not going to discard or triggering classical listener
            cards_to_listen.push(*c);
        } else if php::loose_eq(&destination0, &json!("discard")) && php::loose_eq(&loc, &json!("hand")) && !php::empty(php::get(&card::get(g, *c, "effectPassive")?, "Discard")) {
            // Hand cards are not in the global listener pool; add discard-reactive cards explicitly.
            cards_to_listen.push(*c);
        }
    }

    let ids_snapshot = cs.clone();
    for c in ids_snapshot {
        let new_cid = c;
        let pid = card::pid(g, c)?;
        let mut destination = php::get(&args, "destination").clone();
        let has_fleeting = card::has_token(g, c, "fleeting")?;
        let is_token = card::is_token(g, c)?;

        // We discard a card that has fleeting and should go to reserve
        if php::in_list(&destination, &["reserve", "discard"]) && has_fleeting {
            destination = json!("discard");
        }
        // Save information about original location
        original_location = card::location(g, c)?;
        let in_storms = STORMS.contains(&original_location.as_str());

        // Jinn's effect (ask question to put in mana instead of discarding)
        if (card::is(g, c, "leaveExpeditionToMana")?
            || card::is(g, c, "leaveExpeditionToManaOrDraw")?
            || (card::is(g, c, "leaveExpeditionBoostedToMana")? && card::count_token(g, c, "boost")? > 0))
            && in_storms
            && !php::truthy(&get_arg(a, g, ctx, "force")?)
        {
            let mut nodes = vec![ft::action("Discard", json!({"cardId": c, "destination": "mana", "tapped": true, "force": true}), json!({"pId": pid}))];
            let tapped = php::coalesce(&get_arg(a, g, ctx, "tapped")?, &json!(false)).clone();
            let mut new_nodes = if php::loose_eq(&destination, &json!("topOfDeck")) {
                ft::action("Discard", json!({"cardId": c, "destination": "topOfDeck", "tapped": tapped, "force": true}), json!({"pId": pid}))
            } else {
                let seasoned = card::is_seasoned(g, c)?;
                ft::action(
                    "Discard",
                    json!({"cardId": c, "destination": php::get(&args, "destination"), "tapped": tapped, "force": true, "seasoned": seasoned}),
                    json!({"pId": pid}),
                )
            };
            if card::is(g, c, "leaveExpeditionToManaOrDraw")? {
                new_nodes = ft::seq(vec![new_nodes, ft::action("Draw", json!({"players": "me"}), php::arr())]);
            }
            nodes.push(new_nodes);
            let mut to_add = ft::xor(nodes);
            php::set(&mut to_add, "pId", json!(pid));
            php::set(&mut to_add, "sourceId", json!(c));
            insert_as_child(g, ctx, &to_add)?;
            cs.retain(|x| *x != c);
            continue;
        }

        if php::loose_eq(&destination, &json!("reserve")) && in_storms && card::is(g, c, "leaveExpeditionDefect")? {
            let mut to_add = ft::seq(vec![
                ft::gain(json!(c), "fleeting", json!(1), json!(99)),
                ft::action("SpecialEffect", json!({"effect": "defect", "cardId": c}), json!({"sourceId": c})),
            ]);
            php::set(&mut to_add, "pId", json!(pid));
            insert_as_child(g, ctx, &to_add)?;
            cs.retain(|x| *x != c);
            continue;
        }

        let ty = card::get(g, c, "type")?;
        // Floral Tent
        if globals::is(g, "dayPhase")? && in_storms && php::in_list(&ty, &["token", "character"]) {
            let owner = player_req(card::get_player(g, c)?, "hasProtectAnchoredInExpedition")?;
            let gig = php::truthy(&card::is_gigantic(g, c, false)?);
            if storm_shims::has_protect_anchored_in_expedition(g, owner, &original_location, gig)? && card::has_token(g, c, "anchored")? {
                cs.retain(|x| *x != c);
                g.notify("message", json!({"log": "${card_name} is not discarded but loose <ANCHORED> instead", "cardId": c}));
                insert_as_child(g, ctx, &json!({"action": "Loose", "args": {"cardId": c, "type": "anchored"}}))?;
                continue;
            }
        }

        // Floral tent bravos
        if globals::is(g, "dayPhase")? && in_storms && php::in_list(&ty, &["token", "character"]) {
            let owner = player_req(card::get_player(g, c)?, "hasProtectBoostedInExpedition")?;
            let gig = php::truthy(&card::is_gigantic(g, c, false)?);
            if storm_shims::has_protect_boosted_in_expedition(g, owner, &original_location, gig)? && card::has_token(g, c, "boost")? {
                cs.retain(|x| *x != c);
                g.notify("message", json!({"log": "${card_name} is not discarded but loose <BOOST> instead", "cardId": c}));
                insert_as_child(g, ctx, &json!({"action": "Loose", "args": {"cardId": c, "type": "boost", "n": 99}}))?;
                continue;
            }
        }

        // Special case of MoonlightJellyFish
        if is_sacrifice(a, g, ctx)? {
            if has_fleeting && card::is(g, c, "sacrificeAndFleetingDraw")? {
                // Sacrifice a fleeting
                insert_as_child(g, ctx, &json!({"action": "Draw", "args": {"players": "me"}}))?;
            } else if !has_fleeting && card::is(g, c, "sacrificeAndNotFleetingGoToReserve")? {
                // Sacrifice a non fleeting: move the card from discard as afterFinishing
                push_after_finishing_childs(g, &json!([{"action": "Discard", "args": {"destination": "reserve", "cardId": c}}]))?;
            }
        }

        // If the card discarded (to hand/mana/reserve) is the source of another node, we need to authorize listening
        if php::in_list(&json!(original_location), &["stormLeft", "stormRight", "landmark"]) {
            engine::force_listening_node(g, &json!(c), None)?;
        }

        if card::is_token(g, c)? {
            // Destroy the card if token
            deleted_tokens.push(c);
            let m = card::discard_to(g, c, "destroy", &php::arr(), false, &php::arr())?;
            deleted_meeples.extend(m);
            // X Marks the spot
            if php::truthy(&get_arg(a, g, ctx, "readyIfCostLower")?) && php::gt(&get_arg(a, g, ctx, "position")?, &card::get(g, c, "costHand")?) {
                let sid = get_source_id(g, ctx)?;
                insert_as_child(g, ctx, &ft::action("Ready", json!({"cardId": "mana"}), json!({"sourceId": sid})))?;
            }
        } else {
            // Otherwise move the card
            let season = if php::truthy(&get_arg(a, g, ctx, "seasoned")?) { json!([c]) } else { php::arr() };
            let m = card::discard_to(g, c, &php::strval(&destination), &season, false, &php::arr())?;
            deleted_meeples.extend(m);
            if php::loose_eq(&destination, &json!("topOfDeck")) {
                let owner = player_req(card::get_player(g, c)?, "getId")?;
                cards::insert_on_top(g, &json!(c), &format!("deck-{}", owner))?;
                let position = get_arg(a, g, ctx, "position")?;
                if !position.is_null() {
                    let owner = player_req(card::get_player(g, c)?, "getId")?;
                    cards::move_at_position(g, c, &format!("deck-{}", owner), php::intval(&position))?;
                    // X Marks the spot
                    if php::truthy(&get_arg(a, g, ctx, "readyIfCostLower")?) && php::gt(&get_arg(a, g, ctx, "position")?, &card::get(g, c, "costHand")?) {
                        let sid = get_source_id(g, ctx)?;
                        insert_as_child(g, ctx, &ft::action("Ready", json!({"cardId": "mana"}), json!({"sourceId": sid})))?;
                    }
                }
            }
            // Do we need to tap the card ? (LY_Rare_MightyJinn)
            if php::truthy(&get_arg(a, g, ctx, "tapped")?) {
                card::set_property(g, c, "tapped", json!(true), true)?;
            }
        }

        if original_location == "hand" || original_location == "reserve" {
            let mut flags = json!({"discardFromHandOrReserve": true});
            if original_location == "hand" {
                php::set(&mut flags, "discardFromHand", json!(true));
            }
            let mut activated = globals::get(g, "abilityActivatedThisTurn")?;
            let k = pid.to_string();
            let merged = php::array_merge(&[php::coalesce(php::get(&activated, &k), &php::arr()), &flags]);
            php::set(&mut activated, &k, merged);
            globals::set(g, "abilityActivatedThisTurn", activated)?;
        }

        let gigantic = in_storms && php::truthy(&card::is_gigantic(g, c, false)?);
        let sacrifice = is_sacrifice(a, g, ctx)?;
        let sabotage = is_sabotage(a, g, ctx)? || (php::loose_ne(&get_discard_controller_id(a, g, ctx)?, &json!(pid)) && (original_location == "hand" || original_location == "reserve"));
        let controller = get_discard_controller_id(a, g, ctx)?;
        let source_id = get_source_id(g, ctx)?;
        let pp = player_req(p, "getId")?;
        check_after_listeners(
            a,
            g,
            ctx,
            pp,
            &json!({
                "discardCard": true,
                "cardsToListen": cards_to_listen,
                "cardId": new_cid,
                "token": is_token,
                "from": original_location,
                "to": destination,
                "gigantic": gigantic,
                "sacrifice": sacrifice,
                "sabotage": sabotage,
                "controller": controller,
                "sourceId": source_id,
                "pId": pid,
                "cardPId": pid,
            }),
            true,
            None,
        )?;
    }

    if !deleted_meeples.is_empty() {
        g.notify("silentKill", json!({"ids": deleted_meeples}));
    }
    if !cs.is_empty() {
        g.notify(
            "discard",
            json!({"player_id": p, "cardIds": cs, "destination": php::get(&args, "destination"), "tokens": deleted_tokens, "originalLocation": original_location}),
        );
    }

    // Resolve action if automatic since we are bypassing usual flow with return
    if automatic {
        resolve_action(a, g, ctx, json!([card_ids]), json!(false))?;
    }
    Ok(Value::Null)
}

impl ActionClass for Discard {
    fn name(&self) -> &'static str {
        "Discard"
    }

    // PHP: Actions/Discard.php:26
    fn default_args(&self) -> Value {
        json!({
            "upTo": false,
            "destination": "discard",
            "n": 1,
            "cardId": null,
            "source": "",
            "nLandmarks": 0,
            "special": "",
            "tapped": false,
            "pId": null,
            "canPass": false,
            "force": false,
            "position": null,
            "readyIfCostLower": false,
            "from": null,
            "seasoned": false,
            "desc": "",
        })
    }

    // PHP: Actions/Discard.php:21
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD))
    }

    // PHP: Actions/Discard.php:98
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let location = get_arg(self, g, ctx, "destination")?;
        let mut msg = match php::strval(&location).as_str() {
            "discard" => "Discard ${card}",
            "topOfDeck" => "put ${card} on top of it's owner deck",
            "hand" => "Return to hand ${card}",
            "reserve" => "put it in reserve",
            _ => "discard to ${location} ${card}",
        };
        let card_id = get_arg(self, g, ctx, "cardId")?;
        if is_sacrifice(self, g, ctx)? {
            msg = "Sacrifice ${card}";
        } else if is_sabotage(self, g, ctx)? {
            msg = "sabotage";
        } else if php::loose_eq(&location, &json!("reserve")) && php::loose_eq(&card_id, &json!("EFFECT")) {
            msg = "Send back discarded card to Reserve";
        }

        // Card (if any): None = '', Some(Got)
        let got: Option<cards::Got> = if php::loose_eq(&card_id, &json!("me")) {
            let sid = get_source_id(g, ctx)?;
            if php::loose_eq(&sid, &Value::Null) {
                None
            } else {
                Some(cards::get_raw(g, &sid, true)?)
            }
        } else if php::loose_eq(&card_id, &json!("EFFECT")) {
            let event_card_id = php::get(&get_event(g, ctx)?, "cardId").clone();
            let sid = get_source_id(g, ctx)?;
            if !event_card_id.is_null() {
                Some(cards::get_raw(g, &event_card_id, false)?)
            } else if !sid.is_null() {
                Some(cards::get_raw(g, &sid, false)?)
            } else {
                None
            }
        } else if php::loose_eq(&card_id, &json!("event")) {
            let event_card_id = php::get(&get_event(g, ctx)?, "cardId").clone();
            Some(cards::get_raw(g, &event_card_id, true)?)
        } else if !card_id.is_null() {
            Some(cards::get_raw(g, &card_id, false)?)
        } else {
            None
        };
        let card_v = match got {
            None => json!(""),
            Some(cards::Got::Many(_)) => json!(" multiple cards"),
            Some(cards::Got::One(c)) => card::get(g, c, "name")?,
        };
        Ok(json!({"log": msg, "args": {"location": location, "card": card_v, "i18n": ["location"]}}))
    }

    // PHP: Actions/Discard.php:152
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let from = get_arg(self, g, ctx, "from")?;
        if from.is_null() {
            return Ok(true);
        }
        let c = cards::get(g, &get_arg(self, g, ctx, "cardId")?)?;
        Ok(php::loose_eq(&json!(card::location(g, c)?), &from))
    }

    // PHP: Actions/Discard.php:46
    fn is_optional(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(php::truthy(&get_arg(self, g, ctx, "canPass")?))
    }

    // PHP: Actions/Discard.php:51
    fn can_pass_action(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        Ok(self.is_optional(g, ctx, player)? || cannot_fulfill_discard(self, g, ctx)?)
    }

    // PHP: Actions/Discard.php:157
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let pid = get_arg(self, g, ctx, "pId")?;
        if pid.is_null() {
            base_get_player(g, ctx)
        } else {
            Ok(players::get_by_value(g, &pid))
        }
    }

    // PHP: Actions/Discard.php:163
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_discard(self, g, ctx)?))
    }

    // PHP: Actions/Discard.php:220
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let _force = get_arg(self, g, ctx, "force")?;
        // Nothing automatic if player can pass
        if php::truthy(&get_arg(self, g, ctx, "canPass")?) {
            return Ok(None);
        }
        if cannot_fulfill_discard(self, g, ctx)? {
            game_methods::act_pass_optional_action(g, true)?;
            return Ok(None);
        }

        let mut card_ids = Value::Null;
        let args = args_discard(self, g, ctx)?;
        let card_id = get_arg(self, g, ctx, "cardId")?;
        let special = get_arg(self, g, ctx, "special")?;
        if !card_id.is_null() {
            if php::loose_eq(&card_id, &json!("event")) {
                card_ids = json!([php::get(&get_event(g, ctx)?, "cardId")]);
            } else {
                card_ids = php::get(php::get(php::get(&args, "_private"), "active"), "cards").clone();
            }
        } else if php::loose_eq(&special, &json!("allHand")) {
            // Discard all hand (Loki)
            let p = player_req(self.get_player(g, ctx)?, "getHand")?;
            card_ids = collection::ids(&player::get_hand(g, p, &Value::Null)?);
        } else if php::loose_eq(&special, &json!("allHandReserve")) {
            // Discard all hand and reserve (Rare Loki)
            let p = player_req(self.get_player(g, ctx)?, "getHand")?;
            let hand = player::get_hand(g, p, &Value::Null)?;
            let p = player_req(self.get_player(g, ctx)?, "getReserveCards")?;
            let reserve = player::get_reserve_cards(g, p)?;
            card_ids = collection::ids(&collection::merge(&hand, &reserve));
        } else if php::loose_eq(&special, &json!("allReserve")) {
            let p = player_req(self.get_player(g, ctx)?, "getReserveCards")?;
            card_ids = collection::ids(&player::get_reserve_cards(g, p)?);
        } else if php::loose_eq(&get_arg(self, g, ctx, "n")?, &json!("me")) {
            // Discard myself
            card_ids = json!([get_source_id(g, ctx)?]);
        }

        // Call actDiscard instead of returning values to bypass the checks
        if !card_ids.is_null() {
            act_discard(self, g, ctx, &card_ids, true)?;
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actdiscard" => {
                let ids = param_req(args, 0, "cardIds", "actDiscard")?;
                let automatic = param(args, 1, "automatic").map(|v| php::truthy(&v)).unwrap_or(false);
                act_discard(self, g, ctx, &ids, automatic)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Discard::{}()", method))),
        }
    }
}
