//! Actions/ChooseAssignment.php: the afternoon turn action (play a card, support, tap, pass).
//!
//! Translated: getDescription, $args, getExplicitFreeCostLimits, cardMeetsExplicitFreeCostLimits,
//! argsChooseAssignment, isOptional, isDoable, actPassChooseAssignment.
//! Not translated yet (return `PhpError::nt`): statPlay (:221), actPlay (:248), playCard (:267),
//! actSupport (:1035), actTap (:1093), actPass (:1140).

use crate::core::engine;
use crate::core::node::{self, NodeKind, PASS};
use crate::framework::ST_CHOOSE_ASSIGNMENT;
use crate::game::{CardId, Game, PId};
use crate::managers::players;
use crate::models::action::{get_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct ChooseAssignment;

/// What argsChooseAssignment returns, and whether `play` is a Collection (an array when 'play'
/// is not an authorized action: `->toArray()` on it is a fatal error)
pub struct AssignmentArgs {
    pub args: Value,
    pub play_is_collection: bool,
}

/// `getExplicitFreeCostLimits()`
// PHP: Actions/ChooseAssignment.php:54
fn get_explicit_free_cost_limits(g: &Game, ctx: Ctx) -> Value {
    let ctx_args = get_ctx_args(g, ctx);
    let mut limits = php::arr();
    for k in ["maxHandCost", "minHandCost", "maxBaseCost", "minBaseCost"] {
        if php::key_exists(&ctx_args, k) {
            php::set(&mut limits, k, php::get(&ctx_args, k).clone());
        }
    }
    limits
}

/// `cardMeetsExplicitFreeCostLimits($card, $limits)`
// PHP: Actions/ChooseAssignment.php:67
fn card_meets_explicit_free_cost_limits(g: &mut Game, c: CardId, limits: &Value) -> R<bool> {
    let hand = card::get(g, c, "costHand")?;
    let base = if card::location(g, c)? == "reserve" { card::get(g, c, "costReserve")? } else { card::get(g, c, "costHand")? };
    if php::isset(limits, "minHandCost") && php::lt(&hand, php::get(limits, "minHandCost")) {
        return Ok(false);
    }
    if php::isset(limits, "maxHandCost") && php::gt(&hand, php::get(limits, "maxHandCost")) {
        return Ok(false);
    }
    if php::isset(limits, "minBaseCost") && php::lt(&base, php::get(limits, "minBaseCost")) {
        return Ok(false);
    }
    if php::isset(limits, "maxBaseCost") && php::gt(&base, php::get(limits, "maxBaseCost")) {
        return Ok(false);
    }
    Ok(true)
}

fn matches_sub_type(g: &mut Game, c: CardId, sub_type: &Value) -> R<bool> {
    if sub_type == &json!("disabled") {
        return Ok(true);
    }
    let st = card::get(g, c, "subtypes")?;
    if !php::is_arr(sub_type) && php::in_array(sub_type, &st) {
        return Ok(true);
    }
    Ok(php::is_arr(sub_type) && php::count(&php::array_intersect(sub_type, &st)) > 0)
}

/// `argsChooseAssignment()`
// PHP: Actions/ChooseAssignment.php:87
pub fn args_choose_assignment(a: &dyn ActionClass, g: &mut Game, ctx: Ctx) -> R<AssignmentArgs> {
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getHand() on null"))?;
    let hand = player::get_hand(g, p, &Value::Null)?;
    let reserve = player::get_reserve_cards(g, p)?;
    let mut play = php::arr();
    let mut play_is_collection = false;
    let mut support = php::arr();
    let mut tap = php::arr();
    let mut toto = Value::Null;
    let types = get_arg(a, g, ctx, "types")?;
    let actions = get_arg(a, g, ctx, "actions")?;
    let limits = get_explicit_free_cost_limits(g, ctx);
    let reserve_flip = php::truthy(&get_arg(a, g, ctx, "reserveFlipCost")?);
    let free = php::truthy(&get_arg(a, g, ctx, "free")?);
    let forced = get_arg(a, g, ctx, "forcedLocation")?;
    let sub_type = get_arg(a, g, ctx, "subType")?;

    if php::in_array(&json!("play"), &actions) {
        play_is_collection = true;
        let mut merged = hand.clone();
        for c in &reserve {
            if !merged.contains(c) {
                merged.push(*c);
            }
        }
        let mut playable: Vec<CardId> = vec![];
        for c in merged {
            let ty = card::get(g, c, "type")?;
            let at = card::get(g, c, "additionalType")?;
            let type_ok = php::in_array(&ty, &types) || php::count(&php::array_intersect(&types, &at)) > 0;
            if !type_ok || !matches_sub_type(g, c, &sub_type)? {
                continue;
            }
            let ok = if !free {
                card::can_be_played(g, c, p, false, reserve_flip)?
            } else {
                let orbs = card::get(g, c, "minManaOrbs")?;
                let total = player::get_total_mana(g, p)? as i64;
                php::le(&orbs, &json!(total)) && !card::is_exhausted_reserve_play_blocked(g, c, p)? && card_meets_explicit_free_cost_limits(g, c, &limits)?
            };
            if ok {
                playable.push(c);
            }
        }
        for c in playable {
            let locs = card::get_playable_location(g, c, p, &forced, free)?;
            if !php::empty(&locs) {
                php::set(&mut play, &c.to_string(), locs);
            }
        }
        let mut scouts = php::arr();
        for c in hand {
            let meets_free = free && card_meets_explicit_free_cost_limits(g, c, &limits)?;
            let scout = card::get(g, c, "scout")?;
            let ok = php::gt(&scout, &json!(0))
                && php::in_array(&card::get(g, c, "type")?, &types)
                && matches_sub_type(g, c, &sub_type)?
                && ((!free && card::can_be_played(g, c, p, true, false)?) || meets_free);
            if ok {
                let locs = card::get_scoutable_locations(g, c, p, &forced)?;
                php::set(&mut scouts, &c.to_string(), locs);
            }
        }
        for (k, locs) in php::entries(&scouts) {
            let cur = php::coalesce(php::get(&play, &k), &php::arr()).clone();
            php::set(&mut play, &k, php::array_merge(&[&cur, locs]));
        }
        toto = scouts;
    }

    if php::in_array(&json!("support"), &actions) {
        let mut ids = vec![];
        for c in &reserve {
            if !php::empty(&card::get(g, *c, "effectSupport")?) && !php::empty(&card::get(g, *c, "supportDesc")?) && !card::is_tapped(g, *c)? {
                ids.push(json!(c));
            }
        }
        support = Value::Array(ids);
    }

    if php::in_array(&json!("tap"), &actions) {
        let mut list = player::get_played_cards(g, p, &Value::Null)?;
        for c in player::get_hero_collection(g, p)? {
            if !list.contains(&c) {
                list.push(c);
            }
        }
        let mut ids = vec![];
        for c in list {
            let mut effect_tap = card::get(g, c, "effectTap")?;
            if php::empty(&effect_tap) {
                continue;
            }
            php::set(&mut effect_tap, "sourceId", json!(c));
            if !card::is_tapped(g, c)? {
                let t = engine::build_tree(g, &effect_tap)?;
                if node::is_doable(g, t, Some(p))? {
                    ids.push(json!(c));
                }
            }
        }
        tap = Value::Array(ids);
    }
    let additional = php::count_strict(&get_arg(a, g, ctx, "actions")?)? != 3;
    let mut acts = json!({"play": play, "support": support, "tap": tap});
    if !toto.is_null() {
        php::set(&mut acts, "toto", toto);
    }
    Ok(AssignmentArgs {
        args: json!({"_private": {"active": acts}, "additionalAction": additional, "descSuffix": if additional { "additional" } else { "" }}),
        play_is_collection,
    })
}

fn play_empty(r: &AssignmentArgs) -> R<bool> {
    if !r.play_is_collection {
        return Err(PhpError::new("Error: Call to a member function toArray() on array"));
    }
    Ok(php::empty(php::get(php::get(php::get(&r.args, "_private"), "active"), "play")))
}

impl ActionClass for ChooseAssignment {
    fn name(&self) -> &'static str {
        "ChooseAssignment"
    }

    // PHP: Actions/ChooseAssignment.php:34
    fn default_args(&self) -> Value {
        json!({
            "types": ["permanent", "spell", "character"],
            "actions": ["play", "support", "tap"],
            "maxHandCost": 1000,
            "minHandCost": 0,
            "free": false,
            "maxBaseCost": 1000,
            "minBaseCost": 0,
            "limited": false,
            "forcedLocation": null,
            "mandatory": false,
            "reserveFlipCost": false,
            "subType": "disabled",
        })
    }

    // PHP: Actions/ChooseAssignment.php:20
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_CHOOSE_ASSIGNMENT))
    }

    // PHP: Actions/ChooseAssignment.php:25
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        if php::count_strict(&get_arg(self, g, ctx, "actions")?)? == 3 {
            Ok(json!("Choose an assignment"))
        } else {
            Ok(json!("Play a card"))
        }
    }

    // PHP: Actions/ChooseAssignment.php:87
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_choose_assignment(self, g, ctx)?.args))
    }

    // PHP: Actions/ChooseAssignment.php:188
    fn is_optional(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getOptional() on null"))?;
        if php::loose_eq(&json!(node::get_optional(g, n)), &json!(true)) {
            return Ok(true);
        }
        if php::count_strict(&get_arg(self, g, ctx, "actions")?)? == 3 {
            return Ok(false);
        }
        let r = args_choose_assignment(self, g, ctx)?;
        if !play_empty(&r)? {
            return Ok(false);
        }
        Ok(!php::truthy(&get_arg(self, g, ctx, "mandatory")?))
    }

    // PHP: Actions/ChooseAssignment.php:197
    fn has_act_pass(&self) -> bool {
        true
    }

    fn act_pass(&self, g: &mut Game, ctx: Ctx) -> R<()> {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getParent() on null"))?;
        let parent = node::parent(g, n);
        node::resolve(g, n, json!([PASS]));
        if php::truthy(&get_arg(self, g, ctx, "mandatory")?) {
            if let Some(p) = parent {
                if node::kind(g, p) == NodeKind::Seq {
                    node::resolve(g, p, json!([PASS]));
                }
            }
        }
        engine::save(g)
    }

    // PHP: Actions/ChooseAssignment.php:208
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        if self.is_optional(g, ctx, player)? || !php::truthy(&get_arg(self, g, ctx, "mandatory")?) {
            return Ok(true);
        }
        if php::count_strict(&get_arg(self, g, ctx, "actions")?)? != 3 {
            let r = args_choose_assignment(self, g, ctx)?;
            if !play_empty(&r)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actplay" => Err(PhpError::nt("Actions/ChooseAssignment.php:248 actPlay")),
            "actsupport" => Err(PhpError::nt("Actions/ChooseAssignment.php:1035 actSupport")),
            "acttap" => Err(PhpError::nt("Actions/ChooseAssignment.php:1093 actTap")),
            "actpass" => Err(PhpError::nt("Actions/ChooseAssignment.php:1140 actPass")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\ChooseAssignment::{}()", method))),
        }
    }
}
