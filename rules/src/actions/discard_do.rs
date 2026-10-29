//! Actions/DiscardDo.php: the player discards up to n cards, then the effect runs with
//! `n = count`.

use crate::core::engine;
use crate::core::node;
use crate::framework::ST_DISCARD_DO;
use crate::game::{CardId, Game, PId};
use crate::helpers::{collection, ft};
use crate::managers::{cards, meeples, players};
use crate::models::action::{get_arg, get_ctx_arg, get_source, get_source_id, insert_as_child, param_req, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DiscardDo;

/// `in_array($needle, $haystack)` where PHP throws a TypeError on a non-array haystack
pub(crate) fn in_arr(needle: &Value, haystack: &Value) -> R<bool> {
    if !php::is_arr(haystack) {
        return Err(PhpError::new(format!("TypeError: in_array(): Argument #2 ($haystack) must be of type array, {} given", php::type_name(haystack))));
    }
    Ok(php::in_array(needle, haystack))
}

/// `$c->hasToken($statuses)` with any value
pub(crate) fn has_token_v(g: &Game, c: CardId, token: &Value) -> R<bool> {
    Ok(meeples::count_meeples(g, &format!("card-{}", c), token)? > 0)
}

/// `foreach ($effects as $effect) if (!empty($c->{'getEffect'.$effect}())) $found = true;`
pub(crate) fn has_one_effect(g: &mut Game, c: CardId, effects: &Value) -> R<bool> {
    let mut found = false;
    for e in php::vals(effects) {
        let v = card::get(g, c, &format!("effect{}", php::strval(e)))?;
        if !php::empty(&v) {
            found = true;
        }
    }
    Ok(found)
}

/// `Engine::buildTree($this->getCtxArg('effect'))->getDescription()`
pub(crate) fn effect_desc(g: &mut Game, ctx: Ctx) -> R<Value> {
    let effect = get_ctx_arg(g, ctx, "effect");
    let t = engine::build_tree(g, &effect)?;
    node::get_description(g, t)
}

/// `getTargetableCards($player)`
// PHP: Actions/DiscardDo.php:65
fn get_targetable_cards(a: &DiscardDo, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<Vec<CardId>> {
    let pid = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut pids = json!(players::get_all(g));
    let target_player = get_arg(a, g, ctx, "targetPlayer")?;
    if php::loose_eq(&target_player, &json!("me")) {
        pids = json!([pid]);
    }
    if php::loose_eq(&target_player, &json!("opponent")) {
        pids = php::array_diff(&pids, &json!([pid]));
    }

    let mut max_hand_cost = get_arg(a, g, ctx, "maxHandCost")?;
    if !max_hand_cost.is_i64() && php::loose_eq(&max_hand_cost, &json!("controlledCharacter")) {
        max_hand_cost = json!(player::get_played_cards(g, pid, &json!("character"))?.len() + player::get_played_cards(g, pid, &json!("token"))?.len());
    }

    // What cards ?
    let target_type = get_arg(a, g, ctx, "targetType")?;
    let mut target_location = get_arg(a, g, ctx, "targetLocation")?;
    if php::loose_eq(&target_location, &json!(["source"])) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        target_location = json!([card::location(g, s)?]);
    } else if php::loose_eq(&target_location, &json!(["opposite"])) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        target_location = json!([if card::location(g, s)? == "stormRight" { "stormLeft" } else { "stormRight" }]);
    }

    let arg_cards = get_arg(a, g, ctx, "cards")?;
    let cs = if !php::empty(&arg_cards) {
        let mut out = vec![];
        for c in cards::get_many(g, &arg_cards, true)? {
            let loc_ok = in_arr(&json!(card::location(g, c)?), &target_location)?;
            let keep = loc_ok
                && (in_arr(&card::get(g, c, "type")?, &target_type)? || php::count_strict(&php::array_intersect(&target_type, &card::get_additional_type(g, c)?))? > 0);
            if keep {
                out.push(c);
            }
        }
        out
    } else {
        cards::get_filtered(g, &pids, &target_location, &target_type, true)?
    };

    let exclude_self = get_arg(a, g, ctx, "excludeSelf")?;
    let source_id = get_source_id(g, ctx)?;
    let sub_type = get_arg(a, g, ctx, "subType")?;

    // Which criteria ?
    let mut out = vec![];
    for c in cs {
        if php::truthy(&exclude_self) && php::loose_eq(&json!(c), &source_id) {
            continue;
        }
        let hand_cost = card::get(g, c, "costHand")?;
        let reserve_cost = card::get(g, c, "costReserve")?;
        let statuses = get_arg(a, g, ctx, "statuses")?;
        let effects = get_arg(a, g, ctx, "hasEffects")?;
        if php::loose_ne(&effects, &json!("disabled")) && !has_one_effect(g, c, &effects)? {
            continue;
        }
        if php::loose_ne(&sub_type, &json!("disabled")) && !in_arr(&sub_type, &card::get_subtypes(g, c)?)? {
            continue;
        }
        let cost_check = php::le(&get_arg(a, g, ctx, "minHandCost")?, &hand_cost)
            && php::le(&hand_cost, &max_hand_cost)
            && php::le(&get_arg(a, g, ctx, "minReserveCost")?, &reserve_cost)
            && php::le(&reserve_cost, &get_arg(a, g, ctx, "maxReserveCost")?);
        let keep = if php::loose_eq(&statuses, &json!("disabled")) || php::loose_eq(&card::get(g, c, "type")?, &json!("permanent")) {
            cost_check
        } else {
            cost_check && has_token_v(g, c, &statuses)?
        };
        if keep {
            out.push(c);
        }
    }
    Ok(out)
}

/// `argsDiscardDo()`
// PHP: Actions/DiscardDo.php:148
fn args_discard_do(a: &DiscardDo, g: &mut Game, ctx: Ctx) -> R<Value> {
    let player = players::get_active(g);
    let cs = get_targetable_cards(a, g, ctx, player)?;
    Ok(json!({
        "n": get_arg(a, g, ctx, "n")?,
        "cardIds": collection::ids(&cs),
        "upTo": get_arg(a, g, ctx, "upTo")?,
        "description": a.get_description(g, ctx)?,
        "totalCost": get_arg(a, g, ctx, "totalCost")?,
        "manaOrbs": php::loose_eq(&get_arg(a, g, ctx, "targetLocation")?, &json!(["mana"])),
        "targetCosts": [],
        "effect_desc": effect_desc(g, ctx)?,
    }))
}

/// The sanity checks of `actTarget` (Target and DiscardDo)
// PHP: Actions/Target.php:510, Actions/DiscardDo.php:171
pub(crate) fn check_selection(card_ids: &Value, args: &Value) -> R<()> {
    if !php::empty(&php::array_diff(card_ids, php::get(args, "cardIds"))) {
        return Err(PhpError::new("You cannot target these cards. Should not happen"));
    }
    let count = json!(php::count_strict(card_ids)?);
    let available = json!(php::count(php::get(args, "cardIds")));
    let n = php::get(args, "n");
    if php::gt(&count, n) {
        return Err(PhpError::new("You selected too many cards. Should not happen"));
    }
    if !php::truthy(php::get(args, "upTo")) && ((php::ge(&available, n) && php::lt(&count, n)) || (php::lt(&available, n) && php::loose_ne(&count, &available))) {
        return Err(PhpError::new("You havent selected enough cards. Should not happen"));
    }
    Ok(())
}

/// `actTarget($cardIds)`
// PHP: Actions/DiscardDo.php:165
fn act_target(a: &DiscardDo, g: &mut Game, ctx: Ctx, card_ids: &Value) -> R<Value> {
    let args = args_discard_do(a, g, ctx)?;
    let _total_cost = get_arg(a, g, ctx, "totalCost")?;
    check_selection(card_ids, &args)?;

    let is_sacrifice = get_arg(a, g, ctx, "sacrifice")?;
    cards::get_many(g, card_ids, true)?;
    let source_id = get_source_id(g, ctx)?;
    let mut nodes = vec![];
    for cid in php::vals(card_ids) {
        if php::truthy(&is_sacrifice) {
            nodes.push(ft::action("Discard", json!({"cardId": cid, "desc": "sacrifice"}), json!({"sourceId": source_id})));
        } else {
            nodes.push(ft::action("Discard", json!({"cardId": cid}), json!({"sourceId": source_id})));
        }
    }
    let mut effect = get_arg(a, g, ctx, "effect")?;
    php::set(php::get_mut(&mut effect, "args"), "n", json!(php::count(card_ids)));
    php::set(&mut effect, "sourceId", source_id);
    insert_as_child(g, ctx, &ft::seq(vec![ft::seq(nodes), effect]))?;
    Ok(json!([card_ids]))
}

impl ActionClass for DiscardDo {
    fn name(&self) -> &'static str {
        "DiscardDo"
    }

    // PHP: Actions/DiscardDo.php:39
    fn default_args(&self) -> Value {
        json!({
            "upTo": true,
            "targetPlayer": "me",
            "targetType": ["character", "token", "spell", "permanent"],
            "targetLocation": ["reserve"],
            "maxReserveCost": 1000,
            "maxHandCost": 1000,
            "minReserveCost": 0,
            "minHandCost": 0,
            "n": 99,
            "statuses": "disabled",
            "excludeSelf": false,
            "totalCost": 1000,
            "hasEffects": "disabled",
            "cards": [],
            "discardRemaining": false,
            "subType": "disabled",
            "effect": null,
            "sacrifice": false,
        })
    }

    // PHP: Actions/DiscardDo.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD_DO))
    }

    // PHP: Actions/DiscardDo.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let msg = if php::loose_eq(&get_arg(self, g, ctx, "targetLocation")?, &cards::in_play()) {
            "Discard any number of cards to ${effect_desc}"
        } else {
            "Discard any number of cards from your reserve to ${effect_desc}"
        };
        Ok(json!({"log": msg, "args": {"effect_desc": effect_desc(g, ctx)?}}))
    }

    // PHP: Actions/DiscardDo.php:60
    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/DiscardDo.php:148
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_discard_do(self, g, ctx)?))
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttarget" => {
                let ids = param_req(args, 0, "cardIds", "actTarget")?;
                act_target(self, g, ctx, &ids)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\DiscardDo::{}()", method))),
        }
    }
}
