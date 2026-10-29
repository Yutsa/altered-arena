//! Core/Engine.php: the flow engine (setup, boot, proceed, choices, resolution, tree mutations)

use crate::core::globals;
use crate::core::node::{self, NodeId, NodeKind, PASS};
use crate::framework::{self, *};
use crate::game::{Game, PId};
use crate::game_methods;
use crate::helpers::log;
use crate::managers::{actions, cards, players};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// The root of `Engine::$tree`
pub fn root(g: &Game) -> R<NodeId> {
    g.tree.root.ok_or_else(|| PhpError::new("Error: Call to a member function on null (Engine::$tree)"))
}

/// `Engine::boot()`: the tree from the `engine` global, wrapped into a SEQ if needed
// PHP: Core/Engine.php:21
pub fn boot(g: &mut Game) -> R<()> {
    let t = globals::get(g, "engine")?;
    g.tree = node::Tree::default();
    let r = node::build_tree(g, &t)?;
    g.tree.root = Some(r);
    ensure_seq_root_node(g)
}

/// `Engine::save()`
// PHP: Core/Engine.php:32
pub fn save(g: &mut Game) -> R<()> {
    let r = root(g)?;
    let t = node::to_array(g, r);
    globals::set(g, "engine", t)?;
    Ok(())
}

/// `Engine::ensureSeqRootNode()`
// PHP: Core/Engine.php:41
pub fn ensure_seq_root_node(g: &mut Game) -> R<()> {
    let r = root(g)?;
    if node::kind(g, r) != NodeKind::Seq {
        let n = node::new_node(g, NodeKind::Seq, Default::default(), vec![r]);
        g.tree.root = Some(n);
        save(g)?;
    }
    Ok(())
}

/// `Engine::setup($t, $callback)`
// PHP: Core/Engine.php:53
pub fn setup(g: &mut Game, t: &Value, callback: Value) -> R<()> {
    let r = node::build_tree(g, t)?;
    g.tree.root = Some(r);
    save(g)?;
    globals::set(g, "callbackEngineResolved", callback)?;
    globals::set(g, "engineChoices", json!(0))?;
    globals::set(g, "engineAutomatic", json!(0))?;
    log::enable(g);
    log::start_engine(g)?;
    Ok(())
}

/// `Engine::buildTree($t)`
pub fn build_tree(g: &mut Game, t: &Value) -> R<NodeId> {
    node::build_tree(g, t)
}

/// `Engine::getNextUnresolved()`
pub fn get_next_unresolved(g: &mut Game) -> R<Option<NodeId>> {
    let r = root(g)?;
    node::get_next_unresolved(g, r)
}

/// `Engine::getUndoableMandatoryNode($player)`
pub fn get_undoable_mandatory_node(g: &mut Game, player: Option<PId>) -> R<Option<NodeId>> {
    let r = root(g)?;
    node::get_undoable_mandatory_node(g, r, player)
}

fn pref(g: &mut Game, player: Option<PId>, pref: i64) -> R<Value> {
    let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getPref() on null"))?;
    Ok(crate::models::player::get_pref(g, p, pref))
}

/// `Engine::proceed($confirmedPartial, $isUndo)`
// PHP: Core/Engine.php:102
pub fn proceed(g: &mut Game, confirmed_partial: bool, is_undo: bool) -> R<()> {
    let node_opt = get_next_unresolved(g)?;
    let mut player = players::get_active(g);

    let Some(mut nd) = node_opt else {
        let skipped = globals::get(g, "skippedPlayers")?;
        let day_phase = globals::get(g, "dayPhase")?;
        let played = globals::get(g, "playedCards")?;
        let active_pid = globals::get(g, "activePId")?;
        if (day_phase == json!(true) && (php::loose_ne(&played, &json!(0)) || php::in_array(&active_pid, &skipped))) || day_phase == json!(false) {
            let undo = globals::is(g, "undo")?;
            if globals::get_int(g, "engineChoices")? == 0 || !undo || (undo && php::loose_eq(&pref(g, player, 152)?, &json!(2))) {
                confirm(g)?;
            } else {
                framework::jump(g, ST_CONFIRM_TURN)?;
            }
            return Ok(());
        }
        globals::set(g, "playedForFree", json!(false))?;
        let apid = globals::get(g, "activePId")?;
        insert_at_root(g, &json!({"childs": [{"action": "ChooseAssignment", "pId": apid}]}), true)?;
        return proceed(g, false, false);
    };

    let old_pid = json!(g.active_player_id());
    let mut pid = node::get_pid(g, nd)?;
    if php::loose_eq(&pid, &json!("all")) {
        framework::jump(g, ST_RESOLVE_STACK)?;
        framework::set_all_players_multiactive(g);
        log::checkpoint(g)?;
        globals::set(g, "engineChoices", json!(0))?;
        globals::set(g, "engineAutomatic", json!(0))?;
        proceed_to_state(g, nd, is_undo)?;
        return Ok(());
    } else if php::loose_eq(&pid, &json!("nextPlayer")) {
        let a = players::get_active_id(g);
        pid = json!(players::get_next_id(g, a)?);
    } else if php::loose_eq(&pid, &json!("active")) {
        pid = old_pid.clone();
    } else if php::loose_eq(&pid, &json!("source")) {
        let sid = node::get_source_id(g, nd);
        let c = cards::get(g, &sid)?;
        pid = json!(g.card(c)?.pid);
    }

    // Partial confirmation before handing the flow to another player
    let undo = globals::is(g, "undo")?;
    if undo && (php::loose_eq(&pref(g, player, 152)?, &json!(1)) || php::loose_eq(&pref(g, player, 104)?, &json!(2))) && php::loose_ne(&pid, &Value::Null) && php::loose_ne(&old_pid, &pid) {
        let pp = node::player_of(g, &pid);
        if !node::is_independent(g, nd, pp)? && globals::get_int(g, "engineChoices")? != 0 && !confirmed_partial {
            framework::jump(g, ST_CONFIRM_PARTIAL_TURN)?;
            return Ok(());
        }
    }

    player = node::player_of(g, &pid);
    if php::loose_ne(&pid, &Value::Null) && php::loose_ne(&old_pid, &pid) {
        framework::jump(g, ST_RESOLVE_STACK)?;
        framework::change_active_player(g, &pid);
    }
    if pid.is_null() {
        player = players::get_active(g);
    }
    if confirmed_partial || (php::loose_ne(&pid, &Value::Null) && php::loose_ne(&old_pid, &pid)) {
        log::enable(g);
        log::checkpoint(g)?;
        globals::set(g, "engineChoices", json!(0))?;
    }

    if node::kind(g, nd) == NodeKind::Or {
        let args = node::get_args(g, nd);
        if php::isset(&args, "canReuse") && php::loose_eq(php::get(&args, "canReuse"), &json!(true)) {
            let mut new_node = node::to_array(g, nd);
            if !php::isset(&new_node, "backupNode") {
                let c = php::get(&new_node, "childs").clone();
                php::set(&mut new_node, "backupNode", c);
            } else {
                let b = php::get(&new_node, "backupNode").clone();
                php::set(&mut new_node, "childs", b);
                php::unset(&mut new_node, "choice");
            }
            let built = node::build_tree(g, &new_node)?;
            node::replace(g, nd, built)?;
            save(g)?;
            nd = get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getChoices() on null"))?;
        }
    }

    let choices = node::get_choices(g, nd, player, false)?;
    let all_choices = node::get_choices(g, nd, player, true)?;
    if php::count(&all_choices) > 0 && node::get_type(g, nd) != "leaf" {
        let keys = php::keys_str(&choices);
        let id = keys.first().cloned();
        let auto = if let Some(id) = &id {
            let c = php::get(&choices, id).clone();
            let undo = globals::is(g, "undo")?;
            php::count(&choices) == 1
                && php::count(&all_choices) == 1
                && php::keys_str(&all_choices) == keys
                && ((!undo || (undo && php::loose_eq(&pref(g, player, 152)?, &json!(2))))
                    || ((!php::truthy(php::get(&c, "irreversibleAction")) && !php::truthy(php::get(&c, "optionalAction")))
                        || (globals::get_int(g, "engineChoices")? == 0 && !php::truthy(php::get(&c, "optionalAction")))))
                && !php::truthy(php::get(&c, "forceManualChoice"))
        } else {
            false
        };
        if auto {
            let id = php::intval(&php::key_value(&id.unwrap()));
            choose_node(g, player, id, true)?;
        } else {
            framework::jump(g, ST_RESOLVE_CHOICE)?;
        }
    } else {
        proceed_to_state(g, nd, is_undo)?;
    }
    Ok(())
}

/// `Engine::proceedToState($node, $isUndo)`
// PHP: Core/Engine.php:265
pub fn proceed_to_state(g: &mut Game, nd: NodeId, is_undo: bool) -> R<()> {
    let state = node::get_state(g, nd)?;
    let action = node::get_action(g, nd);
    let run_pre = if !is_undo {
        true
    } else {
        let p = node::get_pid(g, nd)?;
        let pp = if p.is_null() { players::get_active(g) } else { node::player_of(g, &p) };
        !node::is_irreversible(g, nd, pp)?
    };
    if run_pre && php::loose_ne(&node::get_flag(g, nd), &json!("preActionDone")) {
        let interrupt = actions::st_pre_action(g, &action, nd)?;
        node::flag_st_pre_action(g, nd);
        save(g)?;
        if interrupt == json!(true) {
            proceed(g, false, false)?;
            return Ok(());
        }
    }
    framework::jump_to_state(g, &state)
}

/// `Engine::getNextChoice($player, $displayAllChoices)`
pub fn get_next_choice(g: &mut Game, player: Option<PId>, display_all: bool) -> R<Value> {
    let n = get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getChoices() on null"))?;
    node::get_choices(g, n, player, display_all)
}

/// `Engine::chooseNode($player, $nodeId, $auto)`
// PHP: Core/Engine.php:295
pub fn choose_node(g: &mut Game, player: Option<PId>, node_id: i64, auto: bool) -> R<()> {
    let nd = get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getChoices() on null"))?;
    let args = node::get_choices(g, nd, player, false)?;
    if !php::isset(&args, &node_id.to_string()) {
        return Err(PhpError::new("This choice is not possible"));
    }
    if !auto {
        globals::inc(g, "engineChoices", 1)?;
        log::step(g)?;
    }

    if node_id == PASS {
        if node::kind(g, nd) == NodeKind::Parallel {
            let mut other: Value = Value::Null;
            let player = match player {
                Some(p) => Some(p),
                None => players::get_active(g),
            };
            let pid = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            for c in node::childs(g, nd) {
                if node::is_resolved(g, c)? {
                    continue;
                }
                let cp = node::get_pid(g, c)?;
                if php::loose_eq(&cp, &json!(pid)) || cp.is_null() {
                    node::resolve(g, c, json!([PASS]));
                } else if node::is_doable(g, c, player)? {
                    other = node::get_pid(g, c)?;
                }
            }
            if !other.is_null() {
                node::set_info(g, nd, "pId", other);
            } else {
                node::resolve(g, nd, json!([PASS]));
            }
            save(g)?;
            proceed(g, false, false)?;
            return Ok(());
        }
        resolve(g, json!(PASS))?;
        save(g)?;
        proceed(g, false, false)?;
        return Ok(());
    }
    let can_reuse = php::truthy(php::coalesce(php::get(&node::get_args(g, nd), "canReuse"), &json!(false)));
    let ch = *node::childs(g, nd).get(node_id as usize).ok_or_else(|| PhpError::new("Error: Call to a member function isResolved() on null"))?;
    if node::is_resolved(g, ch)? && !can_reuse {
        return Err(PhpError::new("Node is already resolved"));
    }
    if can_reuse {
        node::unresolve_action(g, ch);
        for c in node::childs(g, ch) {
            node::unresolve_action(g, c);
        }
    }
    let cp = node::get_pid(g, ch)?;
    let pid = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"));
    let cp_or = if cp.is_null() { json!(pid.clone()?) } else { cp.clone() };
    let other_optional = if php::loose_ne(&cp_or, &json!(pid.clone()?)) {
        let cpp = node::get_pid(g, ch)?;
        let pp = node::player_of(g, &cpp);
        node::is_optional(g, ch, pp)?
    } else {
        false
    };
    if other_optional {
        let cpp = node::get_pid(g, ch)?;
        node::set_info(g, nd, "pId", cpp);
    } else {
        node::choose(g, nd, node_id, auto)?;
    }
    save(g)?;
    proceed(g, false, false)
}

/// `Engine::resolve($args)`
// PHP: Core/Engine.php:372
pub fn resolve(g: &mut Game, args: Value) -> R<()> {
    let nd = get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function resolve() on null"))?;
    node::resolve(g, nd, args);
    save(g)
}

/// `Engine::resolveAction($args, $checkpoint, &$node)`
// PHP: Core/Engine.php:382
pub fn resolve_action(g: &mut Game, args: Value, checkpoint_flag: &Value, nd: Option<NodeId>) -> R<()> {
    let nd = match nd {
        Some(n) => n,
        None => get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function isReUsable() on null"))?,
    };
    if !node::is_reusable(g, nd) {
        node::resolve_action(g, nd, args.clone());
        if node::is_resolving_parent(g, nd) {
            let p = node::parent(g, nd).ok_or_else(|| PhpError::new("Error: Call to a member function resolve() on null"))?;
            node::resolve(g, p, json!([]));
        }
    }
    save(g)?;
    if !php::isset(&args, "automatic") || php::get(&args, "automatic") == &json!(false) {
        globals::inc(g, "engineChoices", 1)?;
    } else {
        globals::inc(g, "engineAutomatic", 1)?;
    }
    if php::truthy(checkpoint_flag) {
        checkpoint(g)?;
    }
    Ok(())
}

/// `Engine::checkpoint()`
// PHP: Core/Engine.php:406
pub fn checkpoint(g: &mut Game) -> R<()> {
    let player = players::get_active(g);
    let n = get_undoable_mandatory_node(g, player)?;
    if let Some(n) = n {
        let p = node::get_pid(g, n)?;
        let pid = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        if php::loose_eq(&p, &json!(pid)) {
            return Err(PhpError::new("You can't take an irreversible action if there is a mandatory undoable action pending"));
        }
    }
    globals::set(g, "engineChoices", json!(0))?;
    log::checkpoint(g)?;
    Ok(())
}

/// `Engine::insertAtRoot($t, $last)`
// PHP: Core/Engine.php:425
pub fn insert_at_root(g: &mut Game, t: &Value, last: bool) -> R<NodeId> {
    ensure_seq_root_node(g)?;
    let n = node::build_tree(g, t)?;
    let r = root(g)?;
    if last {
        node::push_child(g, r, n);
    } else {
        node::unshift_child(g, r, n);
    }
    save(g)?;
    Ok(n)
}

/// `Engine::getAfterFinishingNode()`
// PHP: Core/Engine.php:441
pub fn get_after_finishing_node(g: &mut Game) -> R<NodeId> {
    ensure_seq_root_node(g)?;
    let r = root(g)?;
    for c in node::childs(g, r) {
        if php::loose_eq(&node::get_flag(g, c), &json!("afterFinishing")) && !node::is_resolved(g, c)? {
            if !g.tree.nodes[c].infos.contains_key("activePId") || node::info(g, c, "activePId").is_null() {
                let a = players::get_active_id(g);
                node::set_info(g, c, "activePId", json!(a));
                save(g)?;
            }
            return Ok(c);
        }
    }
    insert_at_root(g, &json!({"type": "parallel", "flag": "afterFinishing", "childs": []}), true)
}

/// `Engine::pushAfterFinishingChilds($childs)`
// PHP: Core/Engine.php:467
pub fn push_after_finishing_childs(g: &mut Game, childs: &Value) -> R<()> {
    if php::empty(childs) {
        return Ok(());
    }
    let n = get_after_finishing_node(g)?;
    for c in php::vals(childs) {
        let b = node::build_tree(g, c)?;
        node::push_child(g, n, b);
    }
    save(g)
}

/// `Engine::insertAsChild($t, &$node)`: returns the node the flow was inserted under
// PHP: Core/Engine.php:483
pub fn insert_as_child(g: &mut Game, t: &Value, nd: Option<NodeId>) -> R<Option<NodeId>> {
    if t.is_null() {
        return Ok(nd);
    }
    let mut nd = match nd {
        Some(n) => n,
        None => get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getType() on null"))?,
    };
    if node::get_type(g, nd) == "leaf" {
        let mut new_node = node::to_array(g, nd);
        php::set(&mut new_node, "type", json!("seq"));
        let b = node::build_tree(g, &new_node)?;
        nd = node::replace(g, nd, b)?;
    }
    let c = node::build_tree(g, t)?;
    node::push_child(g, nd, c);
    save(g)?;
    Ok(Some(nd))
}

/// `Engine::insertOrUpdateParallelChilds($childs, &$node)`
// PHP: Core/Engine.php:511
pub fn insert_or_update_parallel_childs(g: &mut Game, childs: &Value, nd: Option<NodeId>) -> R<Option<NodeId>> {
    if php::empty(childs) {
        return Ok(nd);
    }
    let mut nd = match nd {
        Some(n) => n,
        None => get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getType() on null"))?,
    };
    if node::get_type(g, nd) == "seq" {
        for c in node::childs(g, nd) {
            if node::get_type(g, c) == "parallel" {
                for nc in php::vals(childs) {
                    let b = node::build_tree(g, nc)?;
                    node::push_child(g, c, b);
                }
                save(g)?;
                return Ok(Some(nd));
            }
        }
        let b = node::build_tree(g, &json!({"type": "parallel", "childs": childs}))?;
        node::push_child(g, nd, b);
    } else {
        if node::get_type(g, nd) == "leaf" {
            let mut new_node = node::to_array(g, nd);
            php::set(&mut new_node, "type", json!("parallel"));
            let b = node::build_tree(g, &new_node)?;
            nd = node::replace(g, nd, b)?;
        }
        for nc in php::vals(childs) {
            let b = node::build_tree(g, nc)?;
            node::push_child(g, nd, b);
        }
        save(g)?;
    }
    Ok(Some(nd))
}

/// `Engine::updateParallelChilds($attributes, &$node)`
// PHP: Core/Engine.php:557
pub fn update_parallel_childs(g: &mut Game, attrs: &Value, nd: Option<NodeId>) -> R<()> {
    if php::empty(attrs) {
        return Ok(());
    }
    let nd = match nd {
        Some(n) => n,
        None => match get_next_unresolved(g)? {
            Some(n) => n,
            None => return Ok(()),
        },
    };
    if node::get_type(g, nd) == "parallel" {
        for (k, v) in php::entries(attrs) {
            node::set_info(g, nd, &k, v.clone());
        }
        save(g)?;
    } else if node::get_type(g, nd) == "seq" {
        for c in node::childs(g, nd) {
            update_parallel_childs(g, attrs, Some(c))?;
        }
    }
    Ok(())
}

/// `Engine::updateAfterFinishingChilds($attributes)`
// PHP: Core/Engine.php:584
pub fn update_after_finishing_childs(g: &mut Game, attrs: &Value) -> R<()> {
    if php::empty(attrs) {
        return Ok(());
    }
    let n = get_after_finishing_node(g)?;
    for (k, v) in php::entries(attrs) {
        node::set_info(g, n, &k, v.clone());
    }
    save(g)
}

/// `Engine::updateBrotherArgs($args)` (no caller in the engine; brothers compared by identity)
// PHP: Core/Engine.php:597
pub fn update_brother_args(g: &mut Game, args: &Value) -> R<()> {
    let mut args: Vec<Value> = php::vals(args).into_iter().cloned().collect();
    let nd = get_next_unresolved(g)?.ok_or_else(|| PhpError::new("Error: Call to a member function getParent() on null"))?;
    let p = node::parent(g, nd).ok_or_else(|| PhpError::new("Error: Call to a member function getChilds() on null"))?;
    for b in node::childs(g, p) {
        if b == nd {
            continue;
        }
        let mut nb = node::to_array(g, b);
        if !args.is_empty() {
            let last = args.pop().unwrap();
            let merged = php::union(php::get(&nb, "args"), &last);
            php::set(&mut nb, "args", merged);
        }
        let built = node::build_tree(g, &nb)?;
        node::replace(g, b, built)?;
    }
    save(g)
}

/// `Engine::confirm()`: runs the callback of the finished flow
// PHP: Core/Engine.php:616
pub fn confirm(g: &mut Game) -> R<()> {
    if get_next_unresolved(g)?.is_some() {
        return Err(PhpError::new("You can't confirm an ongoing turn"));
    }
    let cb = globals::get(g, "callbackEngineResolved")?;
    if php::isset(&cb, "state") {
        framework::jump_to_state(g, php::get(&cb, "state"))?;
    } else if php::isset(&cb, "order") {
        let o = php::get(&cb, "order").clone();
        game_methods::next_player_custom_order(g, &php::key_of(&o))?;
    } else if php::isset(&cb, "method") {
        let m = php::strval(php::get(&cb, "method"));
        game_methods::call_method(g, &m, &Value::Null)?;
    }
    Ok(())
}

/// `Engine::confirmPartialTurn()`
// PHP: Core/Engine.php:636
pub fn confirm_partial_turn(g: &mut Game) -> R<()> {
    let Some(nd) = get_next_unresolved(g)? else {
        return Err(PhpError::new("You can't partial confirm an ended turn"));
    };
    let old = json!(g.active_player_id());
    let pid = node::get_pid(g, nd)?;
    if php::loose_eq(&old, &pid) {
        return Err(PhpError::new("You can't partial confirm for the same player"));
    }
    checkpoint(g)?;
    proceed(g, true, false)
}

/// `Engine::restart()`
// PHP: Core/Engine.php:660
pub fn restart(g: &mut Game) -> R<()> {
    log::undo_turn(g)?;
    globals::fetch(g);
    boot(g)?;
    proceed(g, false, true)
}

/// `Engine::undoToStep($stepId)`
// PHP: Core/Engine.php:673
pub fn undo_to_step(g: &mut Game, step_id: i64) -> R<()> {
    log::undo_to_step(g, step_id)?;
    globals::fetch(g);
    boot(g)?;
    proceed(g, false, true)
}

/// `Engine::hasPendingGroupedReaction($cardId, $gainCardId, $group)`
pub fn has_pending_grouped_reaction(g: &mut Game, card_id: &Value, gain_card_id: &Value, group: &Value) -> R<bool> {
    if g.tree.root.is_none() || card_id.is_null() || group.is_null() {
        return Ok(false);
    }
    let r = root(g)?;
    node::has_pending_grouped_reaction(g, r, card_id, gain_card_id, group)
}

/// `Engine::clearZombieNodes($pId)`
pub fn clear_zombie_nodes(g: &mut Game, pid: &Value) -> R<()> {
    let r = root(g)?;
    node::clear_zombie_nodes(g, r, pid)
}

/// `Engine::getResolvedActions($types)`
pub fn get_resolved_actions(g: &mut Game, types: &Value) -> R<Vec<NodeId>> {
    let r = root(g)?;
    Ok(node::get_resolved_actions(g, r, types))
}

/// `Engine::getLastResolvedAction($types)`
pub fn get_last_resolved_action(g: &mut Game, types: &Value) -> R<Option<NodeId>> {
    Ok(get_resolved_actions(g, types)?.last().copied())
}

/// `Engine::forceListeningNode($sourceId)`: adds the card to `event.cardsToListen` of every
/// unresolved ACTIVATE_CARD node about it
// PHP: Core/Engine.php:719
pub fn force_listening_node(g: &mut Game, source_id: &Value, nodes: Option<NodeId>) -> R<()> {
    let nd = match nodes {
        Some(n) => n,
        None => root(g)?,
    };
    let args = node::get_args(g, nd);
    if php::loose_eq(&node::get_action(g, nd), &json!("ActivateCard")) && php::loose_eq(php::coalesce(php::get(&args, "cardId"), &json!(-1)), source_id) {
        let mut args = args.clone();
        let event = php::get_mut(&mut args, "event");
        let ctl = php::get_mut(event, "cardsToListen");
        php::push(ctl, source_id.clone());
        node::set_info(g, nd, "args", args);
    }
    for c in node::childs(g, nd) {
        if node::is_resolved(g, c)? {
            continue;
        }
        force_listening_node(g, source_id, Some(c))?;
    }
    save(g)
}
