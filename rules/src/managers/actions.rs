//! Managers/Actions.php: the atomic action dispatcher

use crate::actions as a;
use crate::core::engine;
use crate::core::node::{self, NodeId, PASS};
use crate::framework;
use crate::game::{Game, PId};
use crate::game_methods;
use crate::helpers::log;
use crate::managers::players;
use crate::models::action::{ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `Actions::$classes` (Managers/Actions.php:15)
pub const CLASSES: &[&str] = &[
    "ChooseAssignment", "Discard", "NightCleanup", "Gain", "Target", "Loose", "SpellCleanup", "InvokeToken", "ActivateCard", "Draw",
    "SpecialEffect", "CheckCondition", "AfterYou", "RollDie", "Resupply", "MoveExpedition", "UseCounter", "ActivateEffect", "Tap", "PlayCard",
    "MoveCard", "Pay", "DrawMana", "BlockExpedition", "TargetPlayer", "DiscardDo", "TargetExpedition", "Exhaust", "Ready", "Exchange",
    "Spend", "BoostExchange", "EndAfternoon", "MarkRegion", "MoveRegionMarker", "InteruptReveal", "Shuffle", "DiscardFromDeck", "CompleteFeat",
];

/// `Actions::get($actionId)`: the action class (`in_array` is loose, so the name must match)
// PHP: Managers/Actions.php:57
pub fn get(action_id: &Value) -> R<&'static dyn ActionClass> {
    let name = match action_id {
        Value::String(s) if CLASSES.contains(&s.as_str()) => s.as_str(),
        _ => {
            return Err(PhpError::new(format!(
                "Trying to get an atomic action not defined in Actions.php : {}",
                php::strval(action_id)
            )))
        }
    };
    get_by_name(name)
}

/// The action class of a name
pub fn get_by_name(name: &str) -> R<&'static dyn ActionClass> {
    Ok(match name {
        "ActivateCard" => &a::activate_card::ActivateCard,
        "ActivateEffect" => &a::activate_effect::ActivateEffect,
        "AfterYou" => &a::after_you::AfterYou,
        "BlockExpedition" => &a::block_expedition::BlockExpedition,
        "BoostExchange" => &a::boost_exchange::BoostExchange,
        "CheckCondition" => &a::check_condition::CheckCondition,
        "ChooseAssignment" => &a::choose_assignment::ChooseAssignment,
        "CompleteFeat" => &a::complete_feat::CompleteFeat,
        "Discard" => &a::discard::Discard,
        "DiscardDo" => &a::discard_do::DiscardDo,
        "DiscardFromDeck" => &a::discard_from_deck::DiscardFromDeck,
        "Draw" => &a::draw::Draw,
        "DrawMana" => &a::draw_mana::DrawMana,
        "EndAfternoon" => &a::end_afternoon::EndAfternoon,
        "Exchange" => &a::exchange::Exchange,
        "Exhaust" => &a::exhaust::Exhaust,
        "Gain" => &a::gain::Gain,
        "InteruptReveal" => &a::interupt_reveal::InteruptReveal,
        "InvokeToken" => &a::invoke_token::InvokeToken,
        "Loose" => &a::loose::Loose,
        "MarkRegion" => &a::mark_region::MarkRegion,
        "MoveCard" => &a::move_card::MoveCard,
        "MoveExpedition" => &a::move_expedition::MoveExpedition,
        "MoveRegionMarker" => &a::move_region_marker::MoveRegionMarker,
        "NightCleanup" => &a::night_cleanup::NightCleanup,
        "Pay" => &a::pay::Pay,
        "PlayCard" => &a::play_card::PlayCard,
        "Ready" => &a::ready::Ready,
        "Resupply" => &a::resupply::Resupply,
        "RollDie" => &a::roll_die::RollDie,
        "Shuffle" => &a::shuffle::Shuffle,
        "SpecialEffect" => &a::special_effect::SpecialEffect,
        "SpellCleanup" => &a::spell_cleanup::SpellCleanup,
        "Spend" => &a::spend::Spend,
        "Tap" => &a::tap::Tap,
        "Target" => &a::target::Target,
        "TargetExpedition" => &a::target_expedition::TargetExpedition,
        "TargetPlayer" => &a::target_player::TargetPlayer,
        "UseCounter" => &a::use_counter::UseCounter,
        _ => return Err(PhpError::new(format!("Trying to get an atomic action not defined in Actions.php : {}", name))),
    })
}

/// `Actions::isDoable($actionId, $ctx, $player)`
pub fn is_doable(g: &mut Game, action_id: &Value, ctx: Ctx, player: Option<PId>) -> R<bool> {
    get(action_id)?.is_doable(g, ctx, player)
}

/// `Actions::isOptional($actionId, $ctx, $player)`
pub fn is_optional(g: &mut Game, action_id: &Value, ctx: Ctx, player: Option<PId>) -> R<bool> {
    get(action_id)?.is_optional(g, ctx, player)
}

/// `Actions::getState($actionId, $ctx)`
pub fn get_state(g: &mut Game, action_id: &Value, ctx: Ctx) -> R<Value> {
    get(action_id)?.get_state(g, ctx)
}

/// `Actions::getArgs($actionId, $ctx)`: `args<Class>()` merged with `optionalAction`
// PHP: Managers/Actions.php:95
pub fn get_args(g: &mut Game, action_id: &Value, ctx: NodeId) -> R<Value> {
    let a = get(action_id)?;
    let args = a.args(g, Some(ctx))?.unwrap_or_else(php::arr);
    let player = players::get_active(g);
    let opt = a.is_optional(g, Some(ctx), player)? || node::is_optional(g, ctx, player)?;
    Ok(php::array_merge(&[&args, &json!({"optionalAction": opt})]))
}

/// `Actions::takeAction($actionId, $actionName, $args, &$ctx, $automatic)`: runs the act method,
/// then resolves the ctx (engineChoices++) and proceeds, whatever the act method did
// PHP: Managers/Actions.php:104
pub fn take_action(g: &mut Game, action_id: &Value, action_name: &str, args: &Value, ctx: NodeId, automatic: bool) -> R<()> {
    let player = players::get_active(g);
    if !is_doable(g, action_id, Some(ctx), player)? && !is_optional(g, action_id, Some(ctx), player)? {
        let id = php::ucfirst(&php::strval(action_id).to_lowercase());
        return Err(PhpError::new(format!(
            "Attempting to take an action ({}) that is not possible. Either another card erroneously flagged this action as possible, or this action was possible until another card interfered.",
            id
        )));
    }
    if !automatic {
        game_methods::local_check_action(g, action_name, true)?;
        let step = log::step(g)?;
        g.notify("newUndoableStep", json!({"stepId": step}));
    } else {
        framework::check_possible_action(g, action_name)?;
    }
    let a = get(action_id)?;
    let result = a.act(g, Some(ctx), action_name, args)?;
    let _automatic = node::is_automatic(g, ctx, player)?;
    let checkpoint = if result.is_null() { json!(false) } else { result };
    engine::resolve_action(g, json!({"actionName": action_name, "args": args}), &checkpoint, Some(ctx))?;
    engine::proceed(g, false, false)
}

/// `Actions::stAction($actionId, $ctx)`: auto-pass when not doable, else `st<Class>()`, whose
/// result (if not null) is the argument list of `act<Class>`
// PHP: Managers/Actions.php:133
pub fn st_action(g: &mut Game, action_id: &Value, ctx: NodeId) -> R<()> {
    let player = players::get_active(g);
    if !is_doable(g, action_id, Some(ctx), player)? {
        game_methods::act_pass_optional_action(g, true)?;
        return Ok(());
    }
    let a = get(action_id)?;
    if let Some(result) = a.st(g, Some(ctx))? {
        let name = format!("act{}", a.name());
        take_action(g, action_id, &name, &result, ctx, true)?;
    }
    Ok(())
}

/// `Actions::stPreAction($actionId, $ctx)`: `stPre<Class>()`, then a checkpoint if the node is
/// irreversible; returns the interrupt flag
// PHP: Managers/Actions.php:164
pub fn st_pre_action(g: &mut Game, action_id: &Value, ctx: NodeId) -> R<Value> {
    let a = get(action_id)?;
    if a.has_st_pre() {
        let interrupt = a.st_pre(g, Some(ctx))?;
        let p = node::get_pid(g, ctx)?;
        let pp = if p.is_null() { players::get_active(g) } else { node::player_of(g, &p) };
        if node::is_irreversible(g, ctx, pp)? {
            engine::checkpoint(g)?;
        }
        return Ok(interrupt);
    }
    Ok(Value::Null)
}

/// `Actions::pass($actionId, $ctx, $proceed)`
// PHP: Managers/Actions.php:177
pub fn pass(g: &mut Game, action_id: &Value, ctx: NodeId, proceed: bool) -> R<()> {
    let player = players::get_active(g);
    let a = get(action_id)?;
    if !node::is_optional(g, ctx, player)? && !a.can_pass_action(g, Some(ctx), player)? {
        return Err(PhpError::new("This action is not optional"));
    }
    if a.has_act_pass() {
        a.act_pass(g, Some(ctx))?;
    } else {
        engine::resolve(g, json!(PASS))?;
    }
    if proceed {
        engine::proceed(g, false, false)?;
    }
    Ok(())
}
