//! Actions/ChooseAssignment.php: the afternoon turn action (play a card, support, tap, pass).
//!
//! Everything is translated except statPlay (:221): stats are not kept.
//! `playCard` is also called by PlayCard::actPlayCard with the PlayCard node as ctx.

use crate::core::engine;
use crate::core::node::{self, NodeKind, PASS};
use crate::framework::ST_CHOOSE_ASSIGNMENT;
use crate::game::{CardId, Game, PId};
use crate::core::globals;
use crate::helpers::{ft, utils};
use crate::managers::{cards, meeples, players};
use crate::models::action::{check_after_listeners, check_immediate_listeners, get_arg, get_ctx_args, insert_as_child, param_req, push_after_finishing_childs, push_parallel_child, resolve_action, update_after_finishing_childs, ActionClass, Ctx};
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

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actplay" => {
                let card_id = param_req(args, 0, "cardId", "actPlay")?;
                let location = param_req(args, 1, "location", "actPlay")?;
                act_play(self, g, ctx, &card_id, &location)?;
                Ok(Value::Null)
            }
            "actsupport" => {
                let card_id = param_req(args, 0, "cardId", "actSupport")?;
                act_support(self, g, ctx, &card_id)?;
                Ok(Value::Null)
            }
            "acttap" => {
                let card_id = param_req(args, 0, "cardId", "actTap")?;
                act_tap(self, g, ctx, &card_id)?;
                Ok(Value::Null)
            }
            "actpass" => {
                act_pass(self, g, ctx)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\ChooseAssignment::{}()", method))),
        }
    }
}

/// `Actions::get(GAIN)->gain($player, $card, $resource, $amount, $source, $args)`: Gain.php is
/// translated by another port (port-effects); once merged, the body is
/// `crate::actions::gain::gain(g, None, Some(player), card, &json!(resource), &json!(amount), &crate::actions::gain::Source::Null, args)`
/// (tested so against the goldens)
// PHP: Actions/Gain.php:246
fn gain_gain(_g: &mut Game, _player: PId, _card: CardId, _resource: &str, _amount: i64, _source: Value, _args: Value) -> R<()> {
    Err(PhpError::nt("Actions/Gain.php:246 gain"))
}

fn active_player(g: &Game, method: &str) -> R<PId> {
    players::get_active(g).ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `actPlay($cardId, $location)`
// PHP: Actions/ChooseAssignment.php:248
fn act_play(a: &ChooseAssignment, g: &mut Game, ctx: Ctx, card_id: &Value, location: &Value) -> R<()> {
    let r = args_choose_assignment(a, g, ctx)?;
    let play = php::get(php::get(php::get(&r.args, "_private"), "active"), "play").clone();
    let locations = php::get(&play, &php::key_of(card_id)).clone();
    if locations.is_null() {
        return Err(PhpError::new("This card cannot be played. Should not happen"));
    }
    if !php::in_array(location, &locations) {
        return Err(PhpError::new("Invalid location to play a card. Should not happen"));
    }
    // `$locExploded[1] ?? '' == 'scout'` is `$locExploded[1] ?? ('' == 'scout')`
    let exploded = php::explode("_", &php::strval(location));
    let scout = exploded.get(1).is_some_and(|x| php::truthy(&json!(x)));
    let free = get_arg(a, g, ctx, "free")?;
    play_card(g, ctx, card_id, location, &free, &json!(true), &json!(0), &json!(true), scout, &json!(false), &Value::Null)
}

/// Appends a {J}/{H}/{R} effect to `$effects`, its siblings one by one when it is a PARALLEL
// PHP: Actions/ChooseAssignment.php:633
fn push_card_effect(effects: &mut Vec<Value>, effect: Value) {
    let mut effect = effect;
    if php::isset(&effect, "childs") && !php::isset(&effect, "type") {
        php::set(&mut effect, "type", json!("parallel"));
    }
    if php::isset(&effect, "type") && php::loose_eq(php::get(&effect, "type"), &json!("parallel")) {
        for child in php::vals(php::get(&effect, "childs")) {
            effects.push(child.clone());
        }
    } else {
        effects.push(effect);
    }
}

/// The repeated "next character boost" block: `$occur - 1` single boosts, then the rest
fn push_next_boosts(g: &mut Game, ctx: Ctx, c: CardId, to_boost: i64, occur: i64) -> R<()> {
    let mut to_boost = to_boost;
    let mut v = 0;
    while v < occur - 1 {
        push_parallel_child(g, ctx, &ft::gain(json!(c), "boost", json!(1), json!(99)))?;
        to_boost -= 1;
        v += 1;
    }
    if to_boost > 0 {
        push_parallel_child(g, ctx, &ft::gain(json!(c), "boost", json!(to_boost), json!(99)))?;
    }
    Ok(())
}

fn type_in(ty: &Value, list: &[&str]) -> bool {
    php::in_list(ty, list)
}

fn played_has_br30(g: &mut Game, pid: PId) -> R<bool> {
    for c in player::get_played_cards(g, pid, &Value::Null)? {
        let uid = card::get(g, c, "uid")?;
        if php::in_list(&uid, &["ALT_CORE_B_BR_30_R", "ALT_CORE_B_BR_30_C"]) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The cost-reduction alternative: XOR(play at full cost, the reduced play), then the node is
/// resolved
fn insert_cost_reduction(g: &mut Game, ctx: Ctx, flow: Value) -> R<()> {
    insert_as_child(g, ctx, &flow)?;
    resolve_action(&ChooseAssignment, g, ctx, json!(["CostReduction"]), json!(false))
}

fn effect_event(g: &mut Game, card_id: &Value, ty: &Value, at: &Value, from: &str, to: &str, played_free: &Value, put_and_not_played: &Value, steal: &Value) -> R<Value> {
    Ok(json!({
        "playCard": true,
        "cardId": card_id,
        "cardType": ty,
        "additionalType": at,
        "from": from,
        "to": to,
        "playedFree": played_free,
        "putAndNotPlayed": put_and_not_played,
        "additionalEffects": globals::get(g, "additionalEffect")?,
        "stealOwnership": steal,
    }))
}

/// `playCard($cardId, $location, $free, $effectHand, $newCost, $reallyPlayed, $scout,
/// $stealOwnership, $countsAsTurnPlay)` (the action object is a ChooseAssignment whose ctx is
/// `ctx`: the ChooseAssignment node, or the PlayCard node)
// PHP: Actions/ChooseAssignment.php:267
#[allow(clippy::too_many_arguments)]
pub fn play_card(
    g: &mut Game,
    ctx: Ctx,
    card_id: &Value,
    location: &Value,
    free: &Value,
    effect_hand: &Value,
    new_cost: &Value,
    really_played: &Value,
    scout: bool,
    steal_ownership: &Value,
    counts_as_turn_play: &Value,
) -> R<()> {
    let a = &ChooseAssignment;
    let p = active_player(g, "getId")?;
    let c = cards::get(g, card_id)?;
    let location = php::explode("_", &php::strval(location))[0].clone();
    let loc = location.as_str();
    let pkey = p.to_string();

    if card::pid(g, c)? != p {
        return Err(PhpError::new("You do not own this card. Should not happen"));
    }

    let cost: Value;
    if php::loose_eq(free, &json!(false)) {
        // Calculate cost
        let reserve_flip = php::truthy(&get_arg(a, g, ctx, "reserveFlipCost")?);
        let mut c_cost = card::get_cost(g, c, scout, reserve_flip, Some(loc))?;
        // Diocles Chariot racer Rare
        let limitation = card::get(g, c, "playLimitation")?;
        if php::loose_eq(&limitation, &json!("+3StartingRegion")) {
            if loc == "stormLeft" {
                let t = player::get_hero_token(g, p)?;
                if g.meeple(t)?.location == "storm-0" {
                    c_cost = json!(php::intval(&c_cost) + 3);
                }
            }
            if loc == "stormRight" {
                let t = player::get_companion_token(g, p)?;
                if g.meeple(t)?.location == "storm-7" {
                    c_cost = json!(php::intval(&c_cost) + 3);
                }
            }
        }
        // Lyra DJ
        if php::loose_eq(&card::get(g, c, "playLimitation")?, &json!("-2Contact")) && player::is_in_contact(g, p, Some(loc))? {
            c_cost = json!(php::intval(&c_cost) - 2);
        } else if php::loose_eq(&card::get(g, c, "playLimitation")?, &json!("-2Multi")) {
            let opp = players::get_next(g, p)?.ok_or_else(|| PhpError::new("Error: Call to a member function countCardsInLocation() on null"))?;
            if player::count_cards_in_location(g, opp, "stormLeft", &json!("character"))? > 0
                || player::count_cards_in_location(g, opp, "stormRight", &json!("character"))? > 0
            {
                c_cost = json!(php::intval(&c_cost) - 2);
            }
        }
        let mut cost_reduction = globals::get(g, "costReduction")?;
        let ty = card::get(g, c, "type")?;
        let at = card::get(g, c, "additionalType")?;
        let mine = php::coalesce(php::get(&cost_reduction, &pkey), &php::arr()).clone();
        for (cost_type, reduction_cost) in php::entries(&mine) {
            if !php::is_arr(reduction_cost) {
                continue;
            }
            let ct = php::key_value(&cost_type);
            if (php::loose_eq(&ty, &ct) || php::in_array(&ct, &at) || php::loose_eq(&ct, &json!("all"))) && php::is_arr(php::get(&cost_reduction, &pkey)) {
                php::unset(php::get_mut(&mut cost_reduction, &pkey), &cost_type);
            }
        }
        for subtype in php::vals(&card::get(g, c, "subtypes")?) {
            let k = php::key_of(subtype);
            if php::isset(php::get(&cost_reduction, &pkey), &k) {
                php::unset(php::get_mut(&mut cost_reduction, &pkey), &k);
            }
        }
        if php::isset(php::get(&cost_reduction, &pkey), "all") {
            php::unset(php::get_mut(&mut cost_reduction, &pkey), "all");
        }
        globals::set(g, "costReduction", cost_reduction)?;

        // management of CostReductionDiscard, discarding a card from reserve to reduce cost
        let crd = card::get_int(g, c, "costReductionDiscard")?;
        if crd > 0 {
            let reserve_count = player::get_reserve_cards(g, p)?.len();
            let in_reserve = card::location(g, c)? == "reserve";
            if (in_reserve && reserve_count > 1) || (!in_reserve && reserve_count > 0) {
                let flow = ft::xor(vec![
                    // next card played needs to be considered as a turn play, even if played for free
                    ft::action("PlayCard", json!({"cardId": card_id, "free": true, "location": loc, "cost": c_cost, "countsAsTurnPlay": true}), json!({})),
                    ft::seq(vec![
                        ft::action(
                            "Target",
                            json!({
                                "targetLocation": ["reserve"],
                                "targetPlayer": "me",
                                "excludeSelf": true,
                                "targetType": ["character", "token", "spell", "permanent"],
                                "effect": ft::action("Discard", json!([]), json!({})),
                            }),
                            json!({"sourceId": card_id}),
                        ),
                        ft::action(
                            "PlayCard",
                            json!({"cardId": card_id, "free": true, "cost": php::intval(&c_cost) - crd, "location": loc, "countsAsTurnPlay": true}),
                            json!({}),
                        ),
                    ]),
                ]);
                return insert_cost_reduction(g, ctx, flow);
            }
        } else if card::get_int(g, c, "costReductionSacrificePermanent")? > 0 {
            let mut nb_permanents = 0;
            for c2 in player::get_played_cards(g, p, &Value::Null)? {
                let t2 = card::get(g, c2, "type")?;
                if php::loose_eq(&t2, &json!("permanent")) || php::in_array(&json!("permanent"), &card::get(g, c2, "additionalType")?) {
                    nb_permanents += 1;
                }
            }
            if nb_permanents > 0 {
                let crsp = card::get_int(g, c, "costReductionSacrificePermanent")?;
                let flow = ft::xor(vec![
                    ft::action("PlayCard", json!({"cardId": card_id, "free": true, "location": loc, "cost": c_cost, "countsAsTurnPlay": true}), json!({})),
                    ft::seq(vec![
                        ft::action(
                            "Target",
                            json!({
                                "targetType": ["permanent"],
                                "targetPlayer": "me",
                                "effect": ft::action("Discard", json!({"desc": "sacrifice"}), json!({})),
                            }),
                            json!({"sourceId": card_id}),
                        ),
                        ft::action(
                            "PlayCard",
                            json!({"cardId": card_id, "free": true, "cost": php::intval(&c_cost) - crsp, "location": loc, "countsAsTurnPlay": true}),
                            json!({}),
                        ),
                    ]),
                ]);
                return insert_cost_reduction(g, ctx, flow);
            }
        } else if card::get_int(g, c, "costReductionLimitation")? > 0 {
            let crl = card::get_int(g, c, "costReductionLimitation")?;
            let flow = ft::xor(vec![
                ft::action("PlayCard", json!({"cardId": card_id, "free": true, "location": loc, "cost": c_cost, "countsAsTurnPlay": true}), json!({})),
                ft::action(
                    "PlayCard",
                    json!({"cardId": card_id, "free": true, "location": loc, "cost": php::intval(&c_cost) - crl, "limited": true, "countsAsTurnPlay": true}),
                    json!({}),
                ),
            ]);
            return insert_cost_reduction(g, ctx, flow);
        } else if card::get_int(g, c, "costReductionTap")? > 0 {
            let mut untapped = 0;
            for rc in player::get_reserve_cards(g, p)? {
                if !card::is_tapped(g, rc)? {
                    untapped += 1;
                }
            }
            let in_reserve = card::location(g, c)? == "reserve";
            if (in_reserve && untapped > 1) || (!in_reserve && untapped > 0) {
                let crt = card::get_int(g, c, "costReductionTap")?;
                let flow = ft::xor(vec![
                    ft::action("PlayCard", json!({"cardId": card_id, "free": true, "location": loc, "cost": c_cost, "countsAsTurnPlay": true}), json!({})),
                    ft::seq(vec![
                        ft::action(
                            "Target",
                            json!({
                                "targetLocation": ["reserve"],
                                "targetPlayer": "me",
                                "excludeSelf": true,
                                "isNotTapped": true,
                                "targetType": ["character", "token", "spell", "permanent"],
                                "effect": ft::action("Exhaust", json!([]), json!({})),
                            }),
                            json!({"sourceId": card_id}),
                        ),
                        // next card played needs to be considered as a turn play, even if played for free
                        ft::action(
                            "PlayCard",
                            json!({"cardId": card_id, "free": true, "cost": php::intval(&c_cost) - crt, "location": loc, "countsAsTurnPlay": true}),
                            json!({}),
                        ),
                    ]),
                ]);
                return insert_cost_reduction(g, ctx, flow);
            }
        }
        // Has to update cost here as cost is dynamic where it's played
        let crie = card::get_int(g, c, "costReductionIfEmpty")?;
        if crie > 0 && player::count_cards_in_location(g, p, loc, &json!(["token", "character"]))? == 0 && !player::has_gigantic(g, p)? {
            c_cost = json!(php::intval(&c_cost) - crie);
        }

        // Pay cost
        player::pay_mana(g, p, php::intval(&c_cost))?;
        cost = c_cost;
    } else if php::gt(new_cost, &json!(0)) {
        player::pay_mana(g, p, php::intval(new_cost))?;
        cost = new_cost.clone();
    } else {
        cost = json!(0);
    }
    // Effect free plays (Wayfarer, Coppelia, ...) use free=true with cost 0 and must not end the
    // Afternoon turn. Cost-reduction PLAY_CARD nodes pass countsAsTurnPlay=true.
    let counts = if counts_as_turn_play.is_null() {
        json!(!(php::loose_eq(free, &json!(true)) && php::loose_eq(&cost, &json!(0))))
    } else {
        counts_as_turn_play.clone()
    };
    if php::truthy(&counts) {
        globals::inc(g, "playedCards", 1)?;
    }

    // getUid() is never null (the magic getter defaults to '')
    let uid = card::get(g, c, "uid")?;
    if !uid.is_null() {
        let mut history = globals::get(g, "playedCardsHistory")?;
        let uk = php::key_of(&uid);
        let n = php::intval(php::coalesce(php::get(php::get(&history, &pkey), &uk), &json!(0))) + 1;
        php::set(php::get_mut(&mut history, &pkey), &uk, json!(n));
        globals::set(g, "playedCardsHistory", history)?;
    }

    let ty = card::get(g, c, "type")?;
    let at = card::get(g, c, "additionalType")?;
    let is_spellish = php::loose_eq(&ty, &json!("spell")) || php::in_array(&json!("spell"), &at);
    if (is_spellish && globals::is(g, "nextSpellIsFree")?) || (php::loose_eq(free, &json!(true)) && php::loose_eq(&cost, &json!(0))) {
        globals::set(g, "playedForFree", json!(true))?;
    }
    // Move card
    let from_location = card::location(g, c)?;
    card::set_location(g, c, loc)?;
    card::set_property(g, c, "tapped", json!(false), true)?;
    card::set_property(g, c, "revealed", json!(false), true)?;

    let mut turn_cards = globals::get(g, "turnCards")?;
    let n = php::intval(php::coalesce(php::get(&turn_cards, &pkey), &json!(0))) + 1;
    php::set(&mut turn_cards, &pkey, json!(n));
    globals::set(g, "turnCards", turn_cards)?;

    // if card has boosts (from incorrect passive effect), we remove them
    let meeple_ids: Vec<i64> = if from_location == "hand" {
        let ids = meeples::get_in_location(g, &format!("card-{}", c), None)?;
        if !ids.is_empty() {
            meeples::delete(g, &json!(ids))?;
        }
        ids
    } else {
        vec![]
    };

    let new_state = cards::get_next_played_state(g)? + 1;
    card::set_state(g, c, new_state)?;

    // notification
    if !meeple_ids.is_empty() {
        g.notify("silentKill", json!({"tokens": meeple_ids}));
    }
    g.notify("playCard", json!({"player_id": p, "cardId": c, "mana_cost": cost, "location": loc, "fromLocation": from_location}));

    // When does this happens ???? (DISCARD is the action name 'Discard': never a location)
    if loc == "Discard" {
        let deleted = card::discard(g, c, &php::arr(), &php::arr())?;
        g.notify("silentKill", json!({"tokens": deleted}));
    }
    // if played from reserve, it gains fleeting
    else if from_location == "reserve" && !php::in_array(&json!("landmark"), &card::get(g, c, "subtypes")?) {
        gain_gain(g, p, c, "fleeting", 1, Value::Null, json!({"type": "fleeting"}))?;
    } else {
        let hero = player::get_hero(g, p)?.ok_or_else(|| PhpError::new("Error: Call to a member function isAllSpell1Fleeting() on null"))?;
        if card::is(g, hero, "allSpell1Fleeting")? && php::loose_eq(&card::get(g, c, "type")?, &json!("spell")) && card::get_int(g, c, "costHand")? <= 1 {
            gain_gain(g, p, c, "fleeting", 1, Value::Null, json!({"type": "fleeting"}))?;
        }
    }

    if php::loose_eq(&globals::get(g, "nextCharacterFleeting")?, &json!(true)) {
        gain_gain(g, p, c, "fleeting", 1, Value::Null, json!({"type": "fleeting"}))?;
        globals::set(g, "nextCharacterFleeting", json!(false))?;
    }

    // should we boost the card
    let ty = card::get(g, c, "type")?;
    if type_in(&ty, &["character", "token"]) && globals::get_int(g, "nextCharacterBoost")? > 0 {
        let to_boost = globals::get_int(g, "nextCharacterBoost")?;
        let occur = globals::get_int(g, "nextCharacterBoostOccurence")?;
        push_next_boosts(g, ctx, c, to_boost, occur)?;
        globals::set(g, "nextCharacterBoost", json!(0))?;
        globals::set(g, "nextCharacterBoostOccurence", json!(0))?;
    }

    // Sound the Howl
    let ty = card::get(g, c, "type")?;
    if type_in(&ty, &["character", "token"]) && php::in_array(&json!("animal"), &card::get(g, c, "subtypes")?) && globals::get_int(g, "nextAnimalBoost")? > 0 {
        let to_boost = globals::get_int(g, "nextAnimalBoost")?;
        let occur = globals::get_int(g, "nextAnimalBoostOccurence")?;
        push_next_boosts(g, ctx, c, to_boost, occur)?;
        globals::set(g, "nextAnimalBoost", json!(0))?;
        globals::set(g, "nextAnimalBoostOccurence", json!(0))?;
    }

    if from_location == "reserve" && php::loose_eq(&card::get(g, c, "type")?, &json!("character")) && globals::get_int(g, "nextReserveCharacterBoost")? != 0 {
        let n = globals::get_int(g, "nextReserveCharacterBoost")?;
        push_parallel_child(g, ctx, &ft::gain(json!(c), "boost", json!(n), json!(99)))?;
        globals::set(g, "nextReserveCharacterBoost", json!(0))?;
    }
    // The undergrowth
    let other_location = if loc == "stormLeft" {
        "stormRight"
    } else if loc == "stormRight" {
        "stormLeft"
    } else {
        "none"
    };
    let undergrowth = globals::get_int(g, "nextCharacterBoostV")? > 0
        && php::loose_eq(&card::get(g, c, "type")?, &json!("character"))
        && (player::is_in_biome(g, p, loc, "forest", true)?
            || (php::truthy(&card::is_gigantic(g, c, false)?) && player::is_in_biome(g, p, other_location, "forest", true)?));
    if undergrowth {
        let to_boost = globals::get_int(g, "nextCharacterBoostV")?;
        let occur = globals::get_int(g, "nextCharacterBoostOccurence")?;
        push_next_boosts(g, ctx, c, to_boost, occur)?;
        globals::set(g, "nextCharacterBoostOccurence", json!(0))?;
        globals::set(g, "nextCharacterBoostV", json!(0))?;
    }

    // should we anchor the character?
    if php::loose_eq(&globals::get(g, "nextCharacterCost3Anchored")?, &json!(true))
        && type_in(&card::get(g, c, "type")?, &["character", "token"])
        && card::get_int(g, c, "costHand")? <= 3
    {
        push_parallel_child(g, ctx, &ft::gain(json!(c), "anchored", json!(1), json!(99)))?;
        globals::set(g, "nextCharacterCost3Anchored", json!(false))?;
    }

    // This global is declared as 'obj', so games started before it existed default to [],
    // which must not be interpreted as "the next character is asleep".
    let asleep_data = globals::get(g, "nextCharacterAsleep")?;
    let asleep_value = if php::is_arr(&asleep_data) {
        php::truthy(php::coalesce(php::get(&asleep_data, "value"), &json!(false)))
    } else {
        php::truthy(&asleep_data)
    };
    if asleep_value && type_in(&card::get(g, c, "type")?, &["character", "token"]) {
        let gain_node = ft::gain(json!(c), "asleep", json!(1), json!(99));
        if php::is_arr(&asleep_data) && php::truthy(php::coalesce(php::get(&asleep_data, "optional"), &json!(false))) {
            push_parallel_child(g, ctx, &json!({"type": "seq", "optional": true, "childs": [gain_node]}))?;
        } else {
            push_parallel_child(g, ctx, &gain_node)?;
        }
        globals::set(g, "nextCharacterAsleep", json!(false))?;
    }

    if php::loose_eq(&globals::get(g, "nextCharacterAnchored")?, &json!(true)) && type_in(&card::get(g, c, "type")?, &["character", "token"]) {
        push_parallel_child(g, ctx, &ft::gain(json!(c), "anchored", json!(1), json!(99)))?;
        globals::set(g, "nextCharacterAnchored", json!(false))?;
    }
    // LIMBO is used for Romantic Encounter / Phoibos, where hand cost shall be used for comparison
    let base_cost3 = php::loose_eq(&globals::get(g, "nextCharacterBaseCost3Anchored")?, &json!(true))
        && type_in(&card::get(g, c, "type")?, &["character", "token"])
        && ((card::get_int(g, c, "costHand")? <= 3 && (from_location == "hand" || from_location == "limbo"))
            || (from_location == "reserve" && card::get_int(g, c, "costReserve")? <= 3));
    if base_cost3 {
        push_parallel_child(g, ctx, &ft::gain(json!(c), "anchored", json!(1), json!(99)))?;
        globals::set(g, "nextCharacterBaseCost3Anchored", json!(false))?;
    }

    let played_free = json!(php::loose_eq(&cost, &json!(0)));
    let put_and_not_played = json!(!php::truthy(effect_hand));
    let has_card_effects;
    let triggers = if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) {
        let gig = php::truthy(&card::is_gigantic(g, c, false)?);
        !players::has_opponent_blocking_power(g, p, loc, gig)?
    } else {
        true
    };
    if triggers {
        let mut effects: Vec<Value> = vec![];

        if scout {
            effects.push(ft::action("Discard", json!({"cardId": c, "from": loc, "destination": "reserve"}), json!({})));
        }

        // insert effect flow
        let effect = if get_arg(a, g, ctx, "limited")? == json!(true) {
            card::get(g, c, "effectPlayedLimited")?
        } else {
            card::get(g, c, "effectPlayed")?
        };
        if !php::empty(&effect) {
            // Ticket 210043: multi-{J} effects arrive as a Parallel of siblings, pushed one by one
            push_card_effect(&mut effects, effect);
        }

        if php::truthy(effect_hand) && (from_location == "hand" || from_location == "limbo") {
            let effect = card::get(g, c, "effectHand")?;
            if !php::empty(&effect) {
                push_card_effect(&mut effects, effect);
            }
        }

        if from_location == "reserve" {
            let effect = card::get(g, c, "effectReserve")?;
            if !php::empty(&effect) {
                push_card_effect(&mut effects, effect);
            }
        }

        // if it's a spell, effect are resolved immediately
        let ty = card::get(g, c, "type")?;
        let at = card::get(g, c, "additionalType")?;
        if php::loose_eq(&ty, &json!("spell")) || php::in_array(&json!("spell"), &at) {
            let st = card::get(g, c, "subtypes")?;
            if from_location == "hand" && php::loose_eq(&globals::get(g, "removeFleetingIfSpellPlayedHand")?, &json!(true)) {
                effects.push(ft::loose(json!(c), "fleeting", json!(1)));
                globals::set(g, "removeFleetingIfSpellPlayedHand", json!(false))?;
            } else if php::loose_eq(&globals::get(g, "removeFleetingSpellPlayed")?, &json!(true)) {
                effects.push(ft::loose(json!(c), "fleeting", json!(1)));
                globals::set(g, "removeFleetingSpellPlayed", json!(false))?;
            } else if (php::in_array(&json!("artist"), &st) || php::in_array(&json!("song"), &st)) && globals::is(g, "removeFleetingSongArtistPlayed")? {
                effects.push(ft::loose(json!(c), "fleeting", json!(1)));
                globals::set(g, "removeFleetingSongArtistPlayed", json!(false))?;
            }
            if php::loose_eq(&ty, &json!("spell")) {
                let card_pid = json!(card::pid(g, c)?);
                let spell_action = if !effects.is_empty() {
                    let tagged = utils::tag_tree(&json!({"childs": effects}), &json!({"sourceId": c}), false);
                    let tagged = utils::tag_pid(&tagged, &card_pid);
                    // this event lists 'from' before 'additionalType'
                    let event = json!({
                        "playCard": true,
                        "cardId": card_id,
                        "cardType": ty,
                        "from": from_location,
                        "additionalType": at,
                        "to": loc,
                        "playedFree": played_free,
                        "putAndNotPlayed": put_and_not_played,
                        "additionalEffects": globals::get(g, "additionalEffect")?,
                        "stealOwnership": steal_ownership,
                    });
                    ft::seq(vec![ft::par(vec![tagged]), json!({"action": "SpellCleanup", "args": {"cardId": c, "event": event}, "pId": p})])
                } else {
                    let event = effect_event(g, card_id, &ty, &at, &from_location, loc, &played_free, &put_and_not_played, steal_ownership)?;
                    json!({"action": "SpellCleanup", "args": {"cardId": c, "event": event}, "pId": p})
                };
                insert_as_child(g, ctx, &spell_action)?;
                effects = vec![];
            }
            if php::in_array(&json!("spell"), &at) {
                engine::resolve_action(g, php::arr(), &json!(false), None)?;
            }
        } else {
            // resolving current node as some things are inserted before and after
            engine::resolve_action(g, php::arr(), &json!(false), None)?;
        }

        let add_effects_v = globals::get(g, "additionalEffect")?;
        if !php::loose_eq(&add_effects_v, &php::arr()) {
            process_additional_effects(g, c, p, loc, &from_location, add_effects_v, &mut effects)?;
        }

        let mut expeditions_boosts = globals::get(g, "nextCharacterInExpeditionBoost")?;
        let mut boost_expedition = loc;
        let opposite_expedition = if loc == "stormLeft" { "stormRight" } else { "stormLeft" };
        if !php::isset(php::get(&expeditions_boosts, &pkey), loc)
            && php::truthy(&card::is_gigantic(g, c, false)?)
            && php::isset(php::get(&expeditions_boosts, &pkey), opposite_expedition)
        {
            boost_expedition = opposite_expedition;
        }
        if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) && php::isset(php::get(&expeditions_boosts, &pkey), boost_expedition) {
            let n = php::get(php::get(&expeditions_boosts, &pkey), boost_expedition).clone();
            effects.push(ft::gain(json!(c), "boost", n, json!(99)));
            php::unset(php::get_mut(&mut expeditions_boosts, &pkey), boost_expedition);
            globals::set(g, "nextCharacterInExpeditionBoost", expeditions_boosts)?;
        }

        if !effects.is_empty() {
            // setting a default PId for the effects
            let card_pid = json!(card::pid(g, c)?);
            for eff in effects.iter_mut() {
                if php::isset(eff, "pId") {
                    continue;
                }
                php::set(eff, "pId", card_pid.clone());
                if php::isset(eff, "childs") {
                    let childs = php::get_mut(eff, "childs");
                    for k in php::keys_str(childs) {
                        let child = php::get_mut(childs, &k);
                        if php::isset(child, "pId") {
                            continue;
                        }
                        php::set(child, "pId", card_pid.clone());
                    }
                }
            }
            let tagged = utils::tag_tree(&json!({"childs": effects}), &json!({"sourceId": c}), false);
            let cl = json!(card::location(g, c)?);
            let tagged = utils::update_tree(&tagged, &json!(["dioclesLocation"]), &cl, &["expedition"]);
            // Snapshot expedition so delayed ASCEND still works if the source leaves play (e.g. Tengu rush + bounce)
            let tagged = utils::update_tree(&tagged, &json!("source"), &cl, &["expedition"]);
            push_after_finishing_childs(g, php::get(&tagged, "childs"))?;
            if card::get_int(g, c, "rarity")? == 2 {
                update_after_finishing_childs(g, &json!({"noIndependent": true}))?;
            }
        }
        // Track if the played card has its own effects that were pushed
        has_card_effects = !effects.is_empty();
    } else {
        has_card_effects = false;
        g.notify("message", json!({"msg": "Effects are not triggered, due to an effect in the opponent's expedition"}));
    }

    // #171615: "Use of Sleight of Hand on Scribbling Starfish"
    if !php::loose_eq(&card::get(g, c, "type")?, &json!("spell")) {
        let ty = card::get(g, c, "type")?;
        let at = card::get(g, c, "additionalType")?;
        let token = json!(card::is_token(g, c)?);
        let event = json!({
            "playCard": true,
            "cardId": card_id,
            "cardType": ty,
            "additionalType": at,
            "from": from_location,
            "to": loc,
            "playedFree": played_free,
            "putAndNotPlayed": put_and_not_played,
            "additionalEffects": globals::get(g, "additionalEffect")?,
            "token": token,
            "stealOwnership": steal_ownership,
        });
        check_immediate_listeners(a, g, p, &event)?;

        // Count children before adding passives
        let after_finishing = engine::get_after_finishing_node(g)?;
        let child_count_before = node::childs(g, after_finishing).len();

        let event = after_event(g, c, card_id, &from_location, really_played, p, loc, &played_free, &put_and_not_played, steal_ownership)?;
        check_after_listeners(a, g, ctx, p, &event, true, None)?;

        // If the card has its own effects AND passives from other cards were added, force manual
        // choice for effect ordering
        let child_count_after = node::childs(g, after_finishing).len();
        if has_card_effects && child_count_after > child_count_before {
            update_after_finishing_childs(g, &json!({"noIndependent": true}))?;
        }

        if php::in_list(&card::get(g, c, "uid")?, &["ALT_ALIZE_B_BR_45_C", "ALT_ALIZE_B_BR_45_R1", "ALT_ALIZE_B_BR_45_R2"]) {
            let event = after_event(g, c, card_id, &from_location, really_played, p, loc, &played_free, &put_and_not_played, steal_ownership)?;
            check_after_listeners(a, g, ctx, p, &event, true, Some("EatMeEnergyBars"))?;
        }
    }

    // statPlay: stats are not kept
    let mut base_stat0 = false;
    for v in php::vals(&card::get_biomes(g, c, false, false)?) {
        if php::loose_eq(v, &json!(0)) {
            base_stat0 = true;
        }
    }

    let ty = card::get(g, c, "type")?;
    let char_or_token = type_in(&ty, &["character", "token"]);
    let st = card::get(g, c, "subtypes")?;
    if char_or_token && globals::is(g, "removeFleetingCharacterPlayed")? {
        engine::insert_at_root(g, &ft::loose(json!(c), "fleeting", json!(1)), true)?;
        globals::set(g, "removeFleetingCharacterPlayed", json!(false))?;
    } else if char_or_token && globals::is(g, "removeFleetingCharacterStat0Played")? && base_stat0 {
        engine::insert_at_root(g, &ft::loose(json!(c), "fleeting", json!(1)), true)?;
        globals::set(g, "removeFleetingCharacterStat0Played", json!(false))?;
    } else if (php::in_array(&json!("artist"), &st) || php::in_array(&json!("song"), &st)) && globals::is(g, "removeFleetingSongArtistPlayed")? {
        engine::insert_at_root(g, &ft::loose(json!(c), "fleeting", json!(1)), true)?;
        globals::set(g, "removeFleetingSongArtistPlayed", json!(false))?;
    }
    Ok(())
}

/// The event of checkAfterListeners in playCard
#[allow(clippy::too_many_arguments)]
fn after_event(g: &mut Game, c: CardId, card_id: &Value, from: &str, really_played: &Value, p: PId, to: &str, played_free: &Value, put_and_not_played: &Value, steal: &Value) -> R<Value> {
    let ty = card::get(g, c, "type")?;
    let at = card::get(g, c, "additionalType")?;
    let gig = card::is_gigantic(g, c, false)?;
    let token = json!(card::is_token(g, c)?);
    Ok(json!({
        "playCard": true,
        "cardId": card_id,
        "cardType": ty,
        "additionalType": at,
        "from": from,
        "reallyPlayed": really_played,
        "locationPId": p,
        "to": to,
        "gigantic": gig,
        "playedFree": played_free,
        "putAndNotPlayed": put_and_not_played,
        "additionalEffects": globals::get(g, "additionalEffect")?,
        "token": token,
        "stealOwnership": steal,
    }))
}

fn limit_of(e: &Value) -> Value {
    php::coalesce(php::get(e, "limit"), &json!(1000)).clone()
}

fn same_group(e: &Value, x: &Value) -> bool {
    php::loose_eq(php::get(e, "type"), php::get(x, "type"))
        && php::loose_eq(php::get(e, "from"), php::get(x, "from"))
        && php::loose_eq(php::get(e, "effect"), php::get(x, "effect"))
        && php::loose_eq(&limit_of(e), &limit_of(x))
}

/// The additional effects block of playCard (`Globals::getAdditionalEffect()`)
// PHP: Actions/ChooseAssignment.php:735
fn process_additional_effects(g: &mut Game, c: CardId, p: PId, loc: &str, from_location: &str, add_effects_v: Value, effects: &mut Vec<Value>) -> R<()> {
    let mut add_effects = add_effects_v;

    // Pre-pass: determine if effect signatures have an INFTY limit
    let mut infinite_sigs: Vec<String> = vec![];
    for e in php::vals(&add_effects) {
        if php::loose_eq(&limit_of(e), &json!(1000)) {
            let sig = format!("{}|{}|{}", php::strval(php::get(e, "effect")), php::strval(php::get(e, "type")), php::strval(php::get(e, "from")));
            infinite_sigs.push(sig);
        }
    }

    let mut processed: Vec<String> = vec![];

    // foreach by reference: entries unset during the loop are not visited
    for i in php::keys_str(&add_effects) {
        if !php::key_exists(&add_effects, &i) {
            continue;
        }
        let add_effect = php::get(&add_effects, &i).clone();
        if !php::loose_eq(php::get(&add_effect, "type"), &card::get(g, c, "type")?) {
            continue;
        }
        if !php::loose_eq(php::get(&add_effect, "from"), &json!(from_location)) {
            continue;
        }
        let effect_type = php::strval(php::get(&add_effect, "effect"));
        let limit = limit_of(&add_effect);
        let signature = format!("{}|{}|{}|{}", effect_type, php::strval(php::get(&add_effect, "type")), php::strval(php::get(&add_effect, "from")), php::strval(&limit));

        // If this is a limit=1 effect but an INFTY effect exists for the same group, skip it
        let group_sig = format!("{}|{}|{}", effect_type, php::strval(php::get(&add_effect, "type")), php::strval(php::get(&add_effect, "from")));
        if php::loose_eq(&limit, &json!(1)) && infinite_sigs.contains(&group_sig) {
            php::unset(&mut add_effects, &i);
            continue;
        }

        // Skip if we already processed an effect with this signature
        if processed.contains(&signature) {
            php::unset(&mut add_effects, &i);
            continue;
        }

        let new_effect = card::get(g, c, &format!("effect{}", php::ucfirst(&effect_type)))?;

        if php::loose_eq(&new_effect, &php::arr()) {
            continue;
        }

        if php::isset(&add_effect, "to") && !php::get(&add_effect, "to").is_null() {
            if php::loose_eq(php::get(&add_effect, "to"), &json!("sourceLocation")) {
                let source = cards::get(g, php::get(&add_effect, "sourceId"))?;
                if loc != card::location(g, source)? {
                    continue;
                }
            } else if php::loose_ne(php::get(&add_effect, "to"), &json!(loc)) {
                continue;
            }
        }

        let match_count = php::vals(&add_effects).into_iter().filter(|e| same_group(e, &add_effect)).count();
        let boost = php::coalesce(php::get(&add_effect, "boost"), &json!(0)).clone();

        if php::loose_eq(&limit, &json!(1)) {
            // Unique cards merge multiple {R} trigrams as NODE_PARALLEL (with type set). Flatten
            // those childs so limit=1 wraps them in XOR (choose one), not run all.
            let mut list: Vec<Value> = if php::isset(&new_effect, "childs")
                && (!php::isset(&new_effect, "type") || php::loose_eq(php::get(&new_effect, "type"), &json!("parallel")))
            {
                php::vals(php::get(&new_effect, "childs")).into_iter().cloned().collect()
            } else {
                vec![new_effect]
            };

            if effect_type == "reserve" && played_has_br30(g, p)? {
                list.push(ft::gain(json!(c), "boost", json!(1), json!(99)));
            }

            // Boost from Fab Lab Unit rare (etc.) must run after the chosen {R}, not as an XOR option
            let mut after_effects = vec![];
            if php::gt(&boost, &json!(0)) {
                after_effects.push(ft::gain(json!(c), "boost", boost.clone(), json!(99)));
            }

            let choice = if match_count > 1 {
                json!({
                    "type": "or",
                    "args": {"n": match_count},
                    "pId": p,
                    "sourceId": php::get(&add_effect, "sourceId"),
                    "childs": list,
                })
            } else {
                ft::xor(list)
            };

            if after_effects.is_empty() {
                effects.push(choice);
            } else {
                let mut seq = vec![choice];
                seq.extend(after_effects);
                effects.push(ft::seq(seq));
            }
        } else {
            let mut list: Vec<Value> = vec![];
            if !php::empty(&new_effect) {
                list.push(new_effect);
            }

            if effect_type == "reserve" && played_has_br30(g, p)? {
                list.push(ft::gain(json!(c), "boost", json!(1), json!(99)));
            }

            if list.is_empty() && php::gt(&boost, &json!(0)) {
                effects.push(ft::gain(json!(c), "boost", boost.clone(), json!(99)));
            } else {
                if php::gt(&boost, &json!(0)) {
                    list.push(ft::gain(json!(c), "boost", boost.clone(), json!(99)));
                }
                effects.push(ft::seq(list));
            }
        }

        // Mark as processed and remove all matching entries
        processed.push(signature);
        for j in php::keys_str(&add_effects) {
            let e = php::get(&add_effects, &j).clone();
            if same_group(&e, &add_effect) {
                php::unset(&mut add_effects, &j);
            }
        }
    }
    globals::set(g, "additionalEffect", add_effects)?;
    Ok(())
}

/// `actSupport($cardId)`
// PHP: Actions/ChooseAssignment.php:1035
fn act_support(a: &ChooseAssignment, g: &mut Game, ctx: Ctx, card_id: &Value) -> R<()> {
    let p = active_player(g, "getId")?;
    let r = args_choose_assignment(a, g, ctx)?;
    let support = php::get(php::get(php::get(&r.args, "_private"), "active"), "support").clone();

    if !php::in_array(card_id, &support) {
        return Err(PhpError::new("This card cannot be played as support. Should not happen"));
    }

    let c = cards::get(g, card_id)?;
    cards::discard(g, card_id, "discard", None)?;
    g.notify("supportEffect", json!({"player_id": p, "cardId": c}));
    // statPlay: stats are not kept
    record_ability(g, p, json!({"discard": true, "discardFromHandOrReserve": true}), "discard")?;

    let effect = card::get(g, c, "effectSupport")?;
    if !php::empty(&effect) {
        let effect = utils::tag_tree(&effect, &json!({"sourceId": c}), false);
        insert_as_child(g, ctx, &effect)?;
    }

    let ty = card::get(g, c, "type")?;
    let at = card::get(g, c, "additionalType")?;
    let token = card::is_token(g, c)?;
    let event = json!({
        "cardId": card_id,
        "discardCard": true,
        "playCard": false,
        "cardType": ty,
        "additionalType": at,
        "from": "reserve",
        "to": "discard",
        "isSupport": true,
        "token": token,
        "sourceId": card_id,
        "controller": p,
        "pId": p,
    });
    check_after_listeners(a, g, ctx, p, &event, true, Some("Discard"))
}

/// The abilityActivatedThisTurn / Count / TypeCount bookkeeping of actSupport and actTap
fn record_ability(g: &mut Game, p: PId, flags: Value, kind: &str) -> R<()> {
    let pkey = p.to_string();
    let mut activated = globals::get(g, "abilityActivatedThisTurn")?;
    let merged = php::array_merge(&[php::coalesce(php::get(&activated, &pkey), &php::arr()), &flags]);
    php::set(&mut activated, &pkey, merged);
    globals::set(g, "abilityActivatedThisTurn", activated)?;
    let mut count = globals::get(g, "abilityActivatedThisTurnCount")?;
    let n = php::intval(php::coalesce(php::get(&count, &pkey), &json!(0))) + 1;
    php::set(&mut count, &pkey, json!(n));
    globals::set(g, "abilityActivatedThisTurnCount", count)?;
    let mut type_count = globals::get(g, "abilityActivatedThisTurnTypeCount")?;
    let cur = php::coalesce(php::get(&type_count, &pkey), &php::arr()).clone();
    php::set(&mut type_count, &pkey, cur);
    let n = php::intval(php::coalesce(php::get(php::get(&type_count, &pkey), kind), &json!(0))) + 1;
    php::set(php::get_mut(&mut type_count, &pkey), kind, json!(n));
    globals::set(g, "abilityActivatedThisTurnTypeCount", type_count)?;
    Ok(())
}

/// `actTap($cardId)`
// PHP: Actions/ChooseAssignment.php:1093
fn act_tap(a: &ChooseAssignment, g: &mut Game, ctx: Ctx, card_id: &Value) -> R<()> {
    let p = active_player(g, "getId")?;
    let r = args_choose_assignment(a, g, ctx)?;
    let tap = php::get(php::get(php::get(&r.args, "_private"), "active"), "tap").clone();

    if !php::in_array(card_id, &tap) {
        return Err(PhpError::new("This card cannot be tapped. Should not happen"));
    }
    let c = cards::get(g, card_id)?;
    card::set_property(g, c, "tapped", json!(true), true)?;
    g.notify("tapEffect", json!({"player_id": p, "cardId": c}));
    record_ability(g, p, json!({"tap": true}), "tap")?;

    let effect = card::get(g, c, "effectTap")?;
    if !php::empty(&effect) {
        let effect = utils::tag_tree(&effect, &json!({"sourceId": c}), false);
        insert_as_child(g, ctx, &effect)?;
    }

    let loc = card::location(g, c)?;
    check_after_listeners(a, g, ctx, p, &json!({"cardId": c, "cardLocation": loc, "sourceId": c}), true, Some("Exhaust"))
}

/// `actPass()`
// PHP: Actions/ChooseAssignment.php:1140
fn act_pass(a: &ChooseAssignment, g: &mut Game, ctx: Ctx) -> R<()> {
    let p = active_player(g, "getId")?;
    let mut skipped = globals::get(g, "skippedPlayers")?;
    if php::empty(&skipped) {
        globals::set(g, "firstPass", json!(p))?;
    }
    php::push(&mut skipped, json!(p));
    globals::set(g, "skippedPlayers", skipped)?;
    g.notify("passTurn", json!({"player_id": p}));
    check_after_listeners(a, g, ctx, p, &json!(["pass"]), true, Some("EndTurn"))
}
