//! States/EngineTrait.php: the states and client actions of the engine

use crate::core::engine;
use crate::core::globals;
use crate::core::node::{self, NodeKind};
use crate::framework;
use crate::game::Game;
use crate::game_methods;
use crate::helpers::log;
use crate::managers::{actions, cards, players};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

fn next_unresolved(g: &mut Game) -> R<usize> {
    engine::get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getAction() on null"))
}

/// `addCommonArgs(&$args)`: `_private['active']` becomes the active player's entry
// PHP: States/EngineTrait.php:17
pub fn add_common_args(g: &mut Game, args: &mut Value) -> R<()> {
    let ec = globals::get_int(g, "engineChoices")?;
    php::set(args, "previousEngineChoices", json!(ec));
    php::set(args, "previousSteps", json!(log::get_undoable_steps(g)));
    if !php::truthy(php::get(args, "automaticAction")) {
        let private = php::get(args, "_private").clone();
        if php::isset(&private, "active") {
            let mut p = private.clone();
            let active = php::get(&p, "active").clone();
            php::set(&mut p, &players::get_active_id(g).to_string(), active);
            php::unset(&mut p, "active");
            php::set(args, "_private", p);
        }
    }
    Ok(())
}

/// `getCurrentAtomicAction()`
pub fn get_current_atomic_action(g: &mut Game) -> R<Value> {
    let n = next_unresolved(g)?;
    Ok(node::get_action(g, n))
}

/// `argsAtomicAction()`
// PHP: States/EngineTrait.php:45
pub fn args_atomic_action(g: &mut Game) -> R<Value> {
    let player = players::get_active(g);
    let action = get_current_atomic_action(g)?;
    let n = next_unresolved(g)?;
    let mut args = actions::get_args(g, &action, n)?;
    let auto = actions::get(&action)?.is_automatic(g, Some(n), player)?;
    php::set(&mut args, "automaticAction", json!(auto));
    add_args_anytime_action(g, &mut args)?;
    let sid = node::get_source_id(g, n);
    if !php::isset(&args, "source") && !sid.is_null() {
        php::set(&mut args, "sourceId", sid.clone());
        let c = cards::get(g, &sid)?;
        let name = card::get(g, c, "name")?;
        php::set(&mut args, "source", name);
    }
    let source = node::get_source(g, n);
    if !php::isset(&args, "source") && !source.is_null() {
        php::set(&mut args, "source", source);
    }
    let sk = globals::get(g, "skippedPlayers")?;
    php::set(&mut args, "skippedPlayers", sk);
    Ok(args)
}

/// `addArgsAnytimeAction(&$args, $action)`: the anytime action list is always empty
// PHP: States/EngineTrait.php:72
pub fn add_args_anytime_action(g: &mut Game, args: &mut Value) -> R<()> {
    add_common_args(g, args)?;
    if php::truthy(php::get(args, "automaticAction")) {
        return Ok(());
    }
    php::set(args, "anytimeActions", json!([]));
    Ok(())
}

/// `actAnytimeAction($choiceId)`: there are never anytime actions
pub fn act_anytime_action(g: &mut Game, choice: &Value) -> R<()> {
    let (_, args) = framework::state_with_args(g)?;
    if !php::isset(php::get(&args, "anytimeActions"), &php::key_of(choice)) {
        return Err(PhpError::new("You can't take this anytime action"));
    }
    Ok(())
}

/// `actTakeAtomicAction($actionName, $args)`
// PHP: States/EngineTrait.php:120
pub fn act_take_atomic_action(g: &mut Game, action_name: &str, args: &Value) -> R<()> {
    game_methods::local_check_action(g, action_name, true)?;
    let action = get_current_atomic_action(g)?;
    let ctx = next_unresolved(g)?;
    actions::take_action(g, &action, action_name, args, ctx, false)
}

/// `actPassOptionalAction($auto)`
// PHP: States/EngineTrait.php:132
pub fn act_pass_optional_action(g: &mut Game, auto: bool) -> R<()> {
    if auto {
        framework::check_possible_action(g, "actPassOptionalAction")?;
    } else {
        game_methods::local_check_action(g, "actPassOptionalAction", true)?;
    }
    let action = get_current_atomic_action(g)?;
    let n = next_unresolved(g)?;
    actions::pass(g, &action, n, true)
}

/// `stAtomicAction()`
// PHP: States/EngineTrait.php:148
pub fn st_atomic_action(g: &mut Game) -> R<()> {
    let action = get_current_atomic_action(g)?;
    let n = next_unresolved(g)?;
    actions::st_action(g, &action, n)
}

/// `argsResolveChoice()`
// PHP: States/EngineTrait.php:160
pub fn args_resolve_choice(g: &mut Game) -> R<Value> {
    let player = players::get_active(g);
    let n = next_unresolved(g)?;
    let base = php::coalesce(&node::get_args(g, n), &php::arr()).clone();
    let choices = engine::get_next_choice(g, player, false)?;
    let all = engine::get_next_choice(g, player, true)?;
    let mut args = php::array_merge(&[&base, &json!({"choices": choices, "allChoices": all})]);
    if node::kind(g, n) == NodeKind::Xor {
        php::set(&mut args, "descSuffix", json!("xor"));
    }
    let sid = node::get_source_id(g, n);
    if !php::isset(&args, "source") && !sid.is_null() {
        php::set(&mut args, "sourceId", sid.clone());
        let c = cards::get(g, &sid)?;
        let name = card::get(g, c, "name")?;
        php::set(&mut args, "source", name);
    }
    if node::kind(g, n) == NodeKind::Or && php::isset(&args, "n") {
        let remaining = node::or_get_remaining_choices(g, n)?;
        php::set(&mut args, "nRemaining", json!(remaining));
        if !php::isset(&args, "descSuffix") {
            php::set(&mut args, "descSuffix", json!("remaining"));
        }
    }
    add_args_anytime_action(g, &mut args)?;
    Ok(args)
}

/// `actChooseAction($choiceId)`
// PHP: States/EngineTrait.php:185
pub fn act_choose_action(g: &mut Game, choice: i64) -> R<()> {
    let player = players::get_active(g);
    engine::choose_node(g, player, choice, false)
}

/// `argsImpossibleAction()`
pub fn args_impossible_action(g: &mut Game) -> R<Value> {
    let n = next_unresolved(g)?;
    let d = node::get_description(g, n)?;
    let mut args = json!({"desc": d});
    add_args_anytime_action(g, &mut args)?;
    Ok(args)
}

/// `argsConfirmTurn()`
// PHP: States/EngineTrait.php:210
pub fn args_confirm_turn(g: &mut Game) -> R<Value> {
    let ec = globals::get_int(g, "engineChoices")?;
    let mut data = json!({"previousEngineChoices": ec, "previousSteps": log::get_undoable_steps(g), "automaticAction": false});
    add_args_anytime_action(g, &mut data)?;
    Ok(data)
}

/// `stConfirmTurn()`: auto-confirms when the player's preferences say so
// PHP: States/EngineTrait.php:221
pub fn st_confirm_turn(g: &mut Game) -> R<()> {
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getPref() on null"))?;
    let pref = crate::models::player::get_pref(g, p, 103);
    let undo_pref = crate::models::player::get_pref(g, p, 152);
    if php::loose_eq(&pref, &json!(0)) || php::loose_eq(&undo_pref, &json!(2)) {
        act_confirm_turn(g, true)?;
    }
    Ok(())
}

/// `actConfirmTurn($auto)`
pub fn act_confirm_turn(g: &mut Game, auto: bool) -> R<()> {
    if !auto {
        game_methods::local_check_action(g, "actConfirmTurn", true)?;
    }
    engine::confirm(g)
}

/// `actConfirmPartialTurn()`
pub fn act_confirm_partial_turn(g: &mut Game) -> R<()> {
    game_methods::local_check_action(g, "actConfirmPartialTurn", true)?;
    engine::confirm_partial_turn(g)
}

/// `actRestart()`
// PHP: States/EngineTrait.php:244
pub fn act_restart(g: &mut Game) -> R<()> {
    game_methods::local_check_action(g, "actRestart", true)?;
    if !globals::is(g, "undo")? {
        return Err(PhpError::new("Undo is disabled"));
    }
    if globals::get_int(g, "engineChoices")? < 1 {
        return Err(PhpError::new("No choice to undo"));
    }
    engine::restart(g)
}

/// `actUndoToStep($stepId)`
pub fn act_undo_to_step(g: &mut Game, step: i64) -> R<()> {
    game_methods::local_check_action(g, "actRestart", true)?;
    if !globals::is(g, "undo")? {
        return Err(PhpError::new("Undo is disabled"));
    }
    engine::undo_to_step(g, step)
}
