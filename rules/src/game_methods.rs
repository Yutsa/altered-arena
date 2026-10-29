//! altered.game.php: custom turn orders, phase listeners, method dispatch by name (state
//! actions, state args, client actions)

use crate::core::engine;
use crate::core::globals;
use crate::framework::{self, ST_PRE_END_OF_GAME};
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::php;
use crate::states;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `initCustomTurnOrder($key, $order, $callback, $endCallback, $loop, $autoNext, $args)`
// PHP: altered.game.php:154
#[allow(clippy::too_many_arguments)]
pub fn init_custom_turn_order(g: &mut Game, key: &str, order: Option<Vec<PId>>, callback: Value, end_callback: Value, looping: bool, auto_next: bool, args: Value) -> R<()> {
    let mut orders = globals::get(g, "customTurnOrders")?;
    let order = match order {
        Some(o) => o,
        None => players::get_turn_order(g, None)?,
    };
    php::set(
        &mut orders,
        key,
        json!({"order": order, "index": -1, "callback": callback, "args": args, "endCallback": end_callback, "loop": looping}),
    );
    globals::set(g, "customTurnOrders", orders)?;
    if auto_next {
        next_player_custom_order(g, key)?;
    }
    Ok(())
}

/// `initCustomDefaultTurnOrder($key, $callback, $endCallback, $loop, $autoNext)`
// PHP: altered.game.php:172
pub fn init_custom_default_turn_order(g: &mut Game, key: &str, callback: Value, end_callback: Value, looping: bool, auto_next: bool) -> R<()> {
    init_custom_turn_order(g, key, None, callback, end_callback, looping, auto_next, json!([]))
}

/// `nextPlayerCustomOrder($key)`
// PHP: altered.game.php:177
pub fn next_player_custom_order(g: &mut Game, key: &str) -> R<()> {
    let mut orders = globals::get(g, "customTurnOrders")?;
    if !php::isset(&orders, key) {
        return Err(PhpError::new(format!("Asking for the next player of a custom turn order not initialized : {}", key)));
    }
    let o = php::get(&orders, key).clone();
    let n = php::count_strict(php::get(&o, "order"))? as i64;
    let mut i = php::intval(php::get(&o, "index")) + 1;
    if i == n && php::truthy(php::get(&o, "loop")) {
        i = 0;
    }
    php::set(php::get_mut(&mut orders, key), "index", json!(i));
    globals::set(g, "customTurnOrders", orders)?;
    if i < n {
        framework::jump(g, framework::ST_GENERIC_NEXT_PLAYER)?;
        let p = php::geti(php::get(&o, "order"), i).clone();
        framework::change_active_player(g, &p);
        jump_to_or_call(g, php::get(&o, "callback"), php::get(&o, "args"))?;
    } else {
        end_custom_order(g, key)?;
    }
    Ok(())
}

/// `endCustomOrder($key)`
// PHP: altered.game.php:202
pub fn end_custom_order(g: &mut Game, key: &str) -> R<()> {
    let mut orders = globals::get(g, "customTurnOrders")?;
    if !php::isset(&orders, key) {
        return Err(PhpError::new(format!("Asking for ending a custom turn order not initialized : {}", key)));
    }
    let o = php::get(&orders, key).clone();
    let n = php::count_strict(php::get(&o, "order"))?;
    php::set(php::get_mut(&mut orders, key), "index", json!(n));
    globals::set(g, "customTurnOrders", orders)?;
    jump_to_or_call(g, php::get(&o, "endCallback"), &json!([]))
}

/// `jumpToOrCall($mixed, $args)`: a state id, or a method name (case-insensitive) called with
/// `$args`
// PHP: altered.game.php:216
pub fn jump_to_or_call(g: &mut Game, mixed: &Value, args: &Value) -> R<()> {
    if mixed.is_i64() && framework::state_def(php::intval(mixed)).is_some() {
        return framework::jump_to_state(g, mixed);
    }
    let name = php::strval(mixed);
    if method_exists(&name) {
        return call_method(g, &name, args);
    }
    Err(PhpError::new(format!("Failing to jumpToOrCall  : {}", name)))
}

/// Game methods callable by name (state actions and turn order callbacks), lowercase
const METHODS: &[&str] = &[
    "stgamesetup", "stgameend", "stdecksetup", "stfirstday", "stnewday", "stnewdayinit", "stcheckmorning", "initnewday", "stnewdaydraw",
    "stafternewday", "stbeforeassignment", "stassignment", "stprebeforedusk", "stbeforedusk", "stdusk", "stadvancedusk", "stafterdusk",
    "stbeforenight", "stnightcleanup", "stafternightcleanup", "stprenight", "stnight", "stendofnight", "statomicaction", "stresolvestack",
    "stresolvechoice", "stconfirmturn", "stpreendofgame", "genericplayerchecklisteners",
];

pub fn method_exists(name: &str) -> bool {
    METHODS.contains(&name.to_lowercase().as_str())
}

/// `$this->$method($args)` (PHP method names are case-insensitive; methods without parameters
/// ignore the argument)
pub fn call_method(g: &mut Game, name: &str, args: &Value) -> R<()> {
    match name.to_lowercase().as_str() {
        "stgamesetup" => framework::next_state(g, ""),
        "stgameend" => Ok(()),
        "stdecksetup" => states::setup::st_deck_setup(g),
        "stfirstday" => states::new_day::st_first_day(g),
        "stnewday" => states::new_day::st_new_day(g),
        "stnewdayinit" => states::new_day::st_new_day_init(g),
        "stcheckmorning" => states::new_day::st_check_morning(g),
        "initnewday" => states::new_day::init_new_day(g),
        "stnewdaydraw" => states::new_day::st_new_day_draw(g),
        "stafternewday" => states::new_day::st_after_new_day(g),
        "stbeforeassignment" => states::turn::st_before_assignment(g),
        "stassignment" => states::turn::st_assignment(g),
        "stprebeforedusk" => states::turn::st_pre_before_dusk(g),
        "stbeforedusk" => states::turn::st_before_dusk(g),
        "stdusk" => states::turn::st_dusk(g),
        "stadvancedusk" => states::turn::st_advance_dusk(g),
        "stafterdusk" => states::turn::st_after_dusk(g),
        "stbeforenight" => states::turn::st_before_night(g),
        "stnightcleanup" => states::turn::st_night_cleanup(g),
        "stafternightcleanup" => states::turn::st_after_night_cleanup(g),
        "stprenight" => states::turn::st_pre_night(g),
        "stnight" => states::turn::st_night(g),
        "stendofnight" => states::turn::st_end_of_night(g),
        "statomicaction" => states::engine::st_atomic_action(g),
        "stresolvestack" => Ok(()),
        "stresolvechoice" => Ok(()),
        "stconfirmturn" => states::engine::st_confirm_turn(g),
        "stpreendofgame" => states::end_game::st_pre_end_of_game(g, false),
        "genericplayerchecklisteners" => generic_player_check_listeners(g, args),
        _ => Err(PhpError::new(format!("Error: Call to undefined method altered::{}()", name))),
    }
}

/// The `args` method of a state
pub fn call_args(g: &mut Game, name: &str) -> R<Value> {
    match name {
        "argsAtomicAction" => states::engine::args_atomic_action(g),
        "argsResolveChoice" => states::engine::args_resolve_choice(g),
        "argsConfirmTurn" => states::engine::args_confirm_turn(g),
        "argsImpossibleAction" => states::engine::args_impossible_action(g),
        "argsPrecoDeckSelection" => states::setup::args_preco_deck_selection(g),
        "argsFirstDayManaSelection" => states::new_day::args_first_day_mana_selection(g),
        _ => Err(PhpError::new(format!("Error: Call to undefined method altered::{}()", name))),
    }
}

/// `checkCardListeners($typeEvent, $endCallback, $event, $order)`: a custom order over the
/// players, each resolving their own phase triggers
// PHP: altered.game.php:236
pub fn check_card_listeners(g: &mut Game, type_event: &str, end_callback: Value, event: Value, order: Option<Vec<PId>>) -> R<()> {
    let mut event = if event.is_null() { php::arr() } else { event };
    php::set(&mut event, "type", json!(type_event));
    php::set(&mut event, "method", json!(type_event));
    php::set(&mut event, "phase", json!(true));
    init_custom_turn_order(g, type_event, order, json!("genericPlayerCheckListeners"), end_callback, false, true, event)
}

/// `genericPlayerCheckListeners($event)`: the active player's phase reactions (with the
/// `array_push` bug of BeforeNight / EndOfNight)
// PHP: altered.game.php:246
pub fn generic_player_check_listeners(g: &mut Game, event: &Value) -> R<()> {
    let pid = players::get_active_id(g);
    let mut event = event.clone();
    php::set(&mut event, "pId", json!(pid));
    let mut reaction = cards::get_reaction(g, &event, true, true)?;
    let ty = php::get(&event, "type").clone();
    for (phase, global) in [("BeforeNight", "afterRest"), ("EndOfNight", "afterNightCleanup")] {
        if ty == json!(phase) {
            let list = php::coalesce(php::get(&globals::get(g, global)?, &pid.to_string()), &php::arr()).clone();
            if !php::empty(&list) {
                if reaction.is_null() {
                    reaction = list;
                } else {
                    // $reaction = array_push($reaction, ...$list): the new count
                    let n = php::count(&reaction) + php::count(&list);
                    reaction = json!(n);
                }
            }
        }
    }
    if reaction.is_null() {
        let t = php::strval(&ty);
        return next_player_custom_order(g, &t);
    }
    let n = php::count_strict(&reaction)?;
    let tree = json!({"type": "parallel", "childs": reaction, "noIndependent": n > 1});
    engine::setup(g, &tree, json!({"order": ty}))?;
    engine::proceed(g, false, false)
}

/// `localCheckAction($actionName, $bThrowException)`
// PHP: altered.game.php:402
pub fn local_check_action(g: &mut Game, action: &str, throw: bool) -> R<()> {
    let doable = framework::check_action(g, action, false)?;
    if !doable && throw {
        return Err(PhpError::new(format!("This game action is impossible right now{}", action)));
    }
    Ok(())
}

/// `actChangePreference($pref, $value)` (not logged)
pub fn act_change_preference(g: &mut Game, pref: i64, value: &Value) -> R<()> {
    let pid = g.current_player_id();
    for p in g.prefs.iter_mut() {
        if p.player_id == pid && p.pref_id == pref {
            p.value = if value.is_null() { None } else { Some(php::strval(value)) };
        }
    }
    Ok(())
}

/// `actPassOptionalAction($auto)`
pub fn act_pass_optional_action(g: &mut Game, auto: bool) -> R<()> {
    states::engine::act_pass_optional_action(g, auto)
}

/// `zombieTurn($state, $activePlayer)`: the game ends (the rest of the PHP method is dead code)
// PHP: altered.game.php:305
pub fn zombie_turn(g: &mut Game, active: PId) -> R<()> {
    let next = players::get_next_id(g, active)?;
    if let Some(p) = g.players.get_mut(&next) {
        p.score = 1;
    }
    globals::set(g, "zombie", json!(true))?;
    framework::jump(g, ST_PRE_END_OF_GAME)
}
