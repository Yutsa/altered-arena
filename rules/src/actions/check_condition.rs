//! Actions/CheckCondition.php: evaluates `condition(s)` with `Conditions::check`; met → pushes
//! `effect` as a parallel child, not met → pushes `oppositeEffect` or resolves `['notMet']`.
//! Effects that need a deck count as not met when the deck is empty.

use crate::core::{engine, node};
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::conditions;
use crate::managers::{cards, players};
use crate::models::action::{self, get_arg, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct CheckCondition;

/// `isFlowEffect($effect)`
// PHP: Actions/CheckCondition.php:26
fn is_flow_effect(effect: &Value) -> bool {
    php::is_arr(effect) && (php::isset(effect, "action") || php::isset(effect, "type") || php::isset(effect, "childs"))
}

/// `getConditions()`
// PHP: Actions/CheckCondition.php:31
fn get_conditions(a: &CheckCondition, g: &Game, ctx: Ctx) -> R<Value> {
    let conditions = get_ctx_arg(g, ctx, "conditions");
    if !conditions.is_null() {
        return Ok(conditions);
    }
    let cond = get_arg(a, g, ctx, "condition")?;
    Ok(if php::is_arr(&cond) { cond } else { json!([cond]) })
}

/// `effectRequiresDeck($effect)`: DRAW_MANA, or a SpecialEffect that reveals from the deck,
/// anywhere in the flow
// PHP: Actions/CheckCondition.php:90
fn effect_requires_deck(effect: &Value) -> bool {
    if !php::is_arr(effect) {
        return false;
    }
    if php::strict_eq(php::get(effect, "action"), &json!("DrawMana")) {
        return true;
    }
    if php::strict_eq(php::get(effect, "action"), &json!("SpecialEffect")) {
        if let Value::String(se) = php::get(php::get(effect, "args"), "effect") {
            return [
                "revealTop",
                "drawReveal",
                "MindApotheosis",
                "RunesTestamentLook4",
                "boostedRevealBaseStat",
                "boostedRevealArtistSong",
                "boostedRevealRobotPermanent",
                "RomanticEncounter",
                "AuraqKibble",
            ]
            .contains(&se.as_str());
        }
    }
    for child in php::vals(php::get(effect, "childs")) {
        if effect_requires_deck(child) {
            return true;
        }
    }
    let inner = php::get(php::get(effect, "args"), "effect");
    if !inner.is_null() && php::is_arr(inner) {
        return effect_requires_deck(inner);
    }
    false
}

/// `getDeckPlayer($player)`: the source's controller, else the player
// PHP: Actions/CheckCondition.php:133
fn get_deck_player(g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<Option<PId>> {
    if let Some(s) = action::get_source(g, ctx)? {
        return card::get_player(g, s);
    }
    Ok(player)
}

/// `checkCondition($player)`
// PHP: Actions/CheckCondition.php:143
fn check_condition(a: &CheckCondition, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
    let source = action::get_source(g, ctx)?;
    let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut event = json!({"pId": p});
    if php::truthy(&get_arg(a, g, ctx, "previousEvent")?) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))?;
        event = php::get(&node::to_array(g, n), "event").clone();
    }
    let c: Option<CardId> = match source {
        Some(s) => Some(s),
        None => player::get_hero(g, p)?,
    };
    let ctx_args = get_ctx_args(g, ctx);
    if php::isset(&ctx_args, "cardFrom") {
        php::set(&mut event, "cardFrom", php::get(&ctx_args, "cardFrom").clone());
    }
    if php::isset(&ctx_args, "wasGigantic") {
        php::set(&mut event, "wasGigantic", php::get(&ctx_args, "wasGigantic").clone());
    }
    if !conditions::check(g, &ctx_args, c, &event)? {
        return Ok(false);
    }

    let effect = get_arg(a, g, ctx, "effect")?;
    if !php::truthy(&get_arg(a, g, ctx, "ignoreDeck")?) && effect_requires_deck(&effect) {
        let dp = get_deck_player(g, ctx, player)?.ok_or_else(|| PhpError::new("Error: Call to a member function hasDeckCards() on null"))?;
        if !player::has_deck_cards(g, dp)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `!is_null($this->getArg('oppositeEffect')) && $this->getArg('oppositeEffect') != 'OPPOSITE'`
fn has_opposite_effect(a: &CheckCondition, g: &Game, ctx: Ctx) -> R<bool> {
    let o = get_arg(a, g, ctx, "oppositeEffect")?;
    Ok(!o.is_null() && php::loose_ne(&get_arg(a, g, ctx, "oppositeEffect")?, &json!("OPPOSITE")))
}

impl ActionClass for CheckCondition {
    fn name(&self) -> &'static str {
        "CheckCondition"
    }

    fn default_args(&self) -> Value {
        json!({"condition": null, "effect": null, "oppositeEffect": null, "previousEvent": false, "ignoreDeck": false})
    }

    // PHP: Actions/CheckCondition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_CHECK_CONDITION))
    }

    // PHP: Actions/CheckCondition.php:42
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let conditions = get_conditions(self, g, ctx)?;
        let desc = get_ctx_arg(g, ctx, "description");
        for condition in php::vals(&conditions) {
            if php::loose_eq(condition, &json!("hasBoost:4:LTE")) {
                return Ok(json!({"log": "Check if there are less than 4 <BOOST>", "args": []}));
            }
        }
        if php::loose_eq(&desc, &Value::Null) {
            let effect = get_ctx_arg(g, ctx, "effect");
            if !is_flow_effect(&effect) {
                return Ok(json!({"log": "if valid condition", "args": []}));
            }
            let flow = engine::build_tree(g, &effect)?;
            let action1 = node::get_description(g, flow)?;
            let args = json!({"action0": {"log": "if valid condition", "args": []}, "action1": action1});
            return Ok(json!({"log": "${action0}: ${action1}", "args": args}));
        }
        Ok(desc)
    }

    // PHP: Actions/CheckCondition.php:68
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let played = cards::get_played_cards(g, &Value::Null, &Value::Null)?;
        let conditions = get_conditions(self, g, ctx)?;
        for c in played {
            let block = card::get(g, c, "blockAutomaticAction")?;
            if php::isset(&block, "CheckCondition") {
                let b = php::get(&block, "CheckCondition");
                for (v, n) in [(b, 1), (&conditions, 2)] {
                    if !php::is_arr(v) {
                        return Err(PhpError::new(format!("TypeError: array_intersect(): Argument #{} must be of type array, {} given", n, php::type_name(v))));
                    }
                }
                if !php::empty(&php::array_intersect(b, &conditions)) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    // PHP: Actions/CheckCondition.php:85
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        Ok(check_condition(self, g, ctx, player)? || has_opposite_effect(self, g, ctx)?)
    }

    // PHP: Actions/CheckCondition.php:170
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = players::get_active(g);

        let mut nd = get_arg(self, g, ctx, "effect")?;

        if !check_condition(self, g, ctx, player)? {
            if has_opposite_effect(self, g, ctx)? {
                nd = get_arg(self, g, ctx, "oppositeEffect")?;
            } else {
                action::resolve_action(self, g, ctx, json!(["notMet"]), Value::Null)?;
                return Ok(None);
            }
        }

        // effect may be null (or a stray placeholder string): condition met, nothing to run
        if !is_flow_effect(&nd) {
            action::resolve_action(self, g, ctx, json!(["met"]), Value::Null)?;
            return Ok(None);
        }

        if php::isset(&nd, "childs") {
            let mut childs = php::get(&nd, "childs").clone();
            for k in php::keys_str(&childs) {
                let sid = action::get_source_id(g, ctx)?;
                php::set(php::get_mut(&mut childs, &k), "sourceId", sid);
            }
            php::set(&mut nd, "childs", childs);
        }
        let sid = action::get_source_id(g, ctx)?;
        php::set(&mut nd, "sourceId", sid);
        action::push_parallel_child(g, ctx, &nd)?;
        action::resolve_action(self, g, ctx, json!(["met"]), Value::Null)?;
        Ok(None)
    }
}
