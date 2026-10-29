//! Models/Action.php: the base class of the atomic actions.
//!
//! An action class is a unit struct implementing `ActionClass`; the trait's default methods are
//! the base class methods (overridden by the action modules exactly where the PHP class
//! overrides them). The non-virtual base methods are the free functions below, taking the
//! action and its ctx (the engine node, `None` when PHP passes no ctx).

use crate::core::engine;
use crate::core::node::{self, NodeId};
use crate::game::{CardId, Game, PId};
use crate::managers::{cards, players};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `$this->ctx`: the node of the action (PHP keeps it by reference; node ids follow replacements)
pub type Ctx = Option<NodeId>;

pub trait ActionClass: Sync {
    /// `getClassName()`
    fn name(&self) -> &'static str;

    /// `protected $args`: default values of the args (`getArg`)
    fn default_args(&self) -> Value {
        php::arr()
    }

    /// `getState()` (Models/Action.php:73)
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(Value::Null)
    }

    /// `getDescription()` (Models/Action.php:62): `$this->description`, '' by default
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(""))
    }

    /// `isDoable($player)` (Models/Action.php:27)
    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    /// `isOptional($player)` (Models/Action.php:32)
    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    /// `canPassAction($player)` (Models/Action.php:37)
    fn can_pass_action(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        self.is_optional(g, ctx, player)
    }

    /// `isIndependent($player)` (Models/Action.php:42)
    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    /// `isAutomatic($player)` (Models/Action.php:47)
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    /// `isIrreversible($player)` (Models/Action.php:52)
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    /// `isMandatory()` (Models/Action.php:57)
    fn is_mandatory(&self, _g: &mut Game, _ctx: Ctx) -> R<bool> {
        Ok(false)
    }

    /// `getPlayer()` (Models/Action.php:67): `Players::get(ctx->getPId() ?? activeId)`
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        base_get_player(g, ctx)
    }

    /// `args<Class>()`; `None` when the class has no such method
    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Ok(None)
    }

    /// `st<Class>()`; `None` when the method does not exist or returns null, otherwise the
    /// arguments of `act<Class>`
    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Ok(None)
    }

    /// Whether the class has a `stPre<Class>()` method
    fn has_st_pre(&self) -> bool {
        false
    }

    /// `stPre<Class>()`: its return value (`true` interrupts)
    fn st_pre(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(Value::Null)
    }

    /// Whether the class has an `actPass<Class>()` method
    fn has_act_pass(&self) -> bool {
        false
    }

    /// `actPass<Class>()`
    fn act_pass(&self, _g: &mut Game, _ctx: Ctx) -> R<()> {
        Ok(())
    }

    /// A player act method (`act...`, called by `Actions::takeAction` with the client's
    /// arguments, a list or a map of named arguments); returns its return value (null for void)
    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\{}::{}()", self.name(), method)))
    }
}

/// The base `getPlayer()`
// PHP: Models/Action.php:67
pub fn base_get_player(g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
    let pid = match ctx {
        Some(n) => node::get_pid(g, n)?,
        None => return Err(PhpError::new("Error: Call to a member function getPId() on null")),
    };
    let pid = if pid.is_null() { json!(players::get_active_id(g)) } else { pid };
    Ok(players::get_by_value(g, &pid))
}

/// A positional or named argument of an act method (PHP 8.1 spreads string keys as named
/// arguments)
pub fn param(args: &Value, pos: usize, name: &str) -> Option<Value> {
    match args {
        Value::Array(a) => a.get(pos).cloned(),
        Value::Object(o) => o.get(name).cloned().or_else(|| o.get(&pos.to_string()).cloned()),
        _ => None,
    }
}

/// A required argument of an act method (ArgumentCountError when missing)
pub fn param_req(args: &Value, pos: usize, name: &str, method: &str) -> R<Value> {
    param(args, pos, name).ok_or_else(|| PhpError::new(format!("ArgumentCountError: Too few arguments to function {}()", method)))
}

/// `getCtxArgs()`
// PHP: Models/Action.php:86
pub fn get_ctx_args(g: &Game, ctx: Ctx) -> Value {
    match ctx {
        None => php::arr(),
        Some(n) => {
            let a = node::get_args(g, n);
            if a.is_null() {
                php::arr()
            } else {
                a
            }
        }
    }
}

/// `getCtxArg($v)`
pub fn get_ctx_arg(g: &Game, ctx: Ctx, k: &str) -> Value {
    // getCtxArgs()[$k], read in place
    match ctx.and_then(|n| g.tree.nodes[n].infos.get("args")) {
        Some(a) if !a.is_null() => php::get(a, k).clone(),
        _ => Value::Null,
    }
}

thread_local! {
    /// `default_args()` of each action class (constant data), built once per thread
    static DEFAULT_ARGS: std::cell::RefCell<std::collections::HashMap<&'static str, std::rc::Rc<Value>>> = Default::default();
}

fn default_args_of(a: &dyn ActionClass) -> std::rc::Rc<Value> {
    DEFAULT_ARGS.with(|m| m.borrow_mut().entry(a.name()).or_insert_with(|| std::rc::Rc::new(a.default_args())).clone())
}

/// `getArg($v)`: the ctx arg, else the class default, else an exception
// PHP: Models/Action.php:102
pub fn get_arg(a: &dyn ActionClass, g: &Game, ctx: Ctx, k: &str) -> R<Value> {
    let t = get_ctx_arg(g, ctx, k);
    if !t.is_null() {
        return Ok(t);
    }
    let d = default_args_of(a);
    let d = &*d;
    if php::key_exists(d, k) {
        return Ok(php::get(d, k).clone());
    }
    Err(PhpError::new(format!("Trying to get value of an undefined arg without any default value : {} action{}", k, a.name())))
}

fn ctx_node(ctx: Ctx, method: &str) -> R<NodeId> {
    ctx.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `getEvent()`
pub fn get_event(g: &Game, ctx: Ctx) -> R<Value> {
    Ok(node::get_event(g, ctx_node(ctx, "getEvent")?))
}

/// `getEventRecursive($ctx)`: the first event going up the parents
// PHP: Models/Action.php:120
pub fn get_event_recursive(g: &Game, ctx: Ctx) -> R<Value> {
    let mut n = ctx_node(ctx, "getEvent")?;
    loop {
        let e = node::get_event(g, n);
        if !e.is_null() {
            return Ok(e);
        }
        match node::parent(g, n) {
            Some(p) => n = p,
            None => return Ok(Value::Null),
        }
    }
}

/// `getSourceId()`
pub fn get_source_id(g: &Game, ctx: Ctx) -> R<Value> {
    Ok(node::get_source_id(g, ctx_node(ctx, "getSourceId")?))
}

/// `getSource()`: the source card, or null
// PHP: Models/Action.php:139
pub fn get_source(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let sid = get_source_id(g, ctx)?;
    if sid.is_null() {
        return Ok(None);
    }
    Ok(Some(cards::get(g, &sid)?))
}

/// `resolveAction($args, $checkpoint)`: `automatic` from the action, then proceed
// PHP: Models/Action.php:145
pub fn resolve_action(a: &dyn ActionClass, g: &mut Game, ctx: Ctx, args: Value, checkpoint: Value) -> R<()> {
    let player = players::get_active(g);
    let mut args = if args.is_null() { php::arr() } else { args };
    let auto = a.is_automatic(g, ctx, player)?;
    php::set(&mut args, "automatic", json!(auto));
    let n = ctx_node(ctx, "isReUsable")?;
    engine::resolve_action(g, args, &checkpoint, Some(n))?;
    engine::proceed(g, false, false)
}

/// `insertAsChild($flow)`
// PHP: Models/Action.php:156
pub fn insert_as_child(g: &mut Game, ctx: Ctx, flow: &Value) -> R<()> {
    if php::empty(flow) {
        return Ok(());
    }
    engine::insert_as_child(g, flow, ctx)?;
    Ok(())
}

/// `pushAfterFinishingChilds($childs)`
pub fn push_after_finishing_childs(g: &mut Game, childs: &Value) -> R<()> {
    engine::push_after_finishing_childs(g, childs)
}

/// `pushParallelChild($node)`
pub fn push_parallel_child(g: &mut Game, ctx: Ctx, n: &Value) -> R<()> {
    push_parallel_childs(g, ctx, &json!([n]))
}

/// `pushParallelChilds($childs)`
pub fn push_parallel_childs(g: &mut Game, ctx: Ctx, childs: &Value) -> R<()> {
    engine::insert_or_update_parallel_childs(g, childs, ctx)?;
    Ok(())
}

/// `updateParallelChilds($attributes)`
pub fn update_parallel_childs(g: &mut Game, ctx: Ctx, attrs: &Value) -> R<()> {
    engine::update_parallel_childs(g, attrs, ctx)
}

/// `updateAfterFinishingChilds($attributes)`
pub fn update_after_finishing_childs(g: &mut Game, attrs: &Value) -> R<()> {
    engine::update_after_finishing_childs(g, attrs)
}

/// `insertBonusesFlow($bonuses)`: only flows already in node form (FlowConvertor commented out)
// PHP: Models/Action.php:198
pub fn insert_bonuses_flow(g: &mut Game, ctx: Ctx, bonuses: &Value) -> R<()> {
    if php::empty(bonuses) {
        return Ok(());
    }
    if php::isset(php::geti(bonuses, 0), "type") || php::isset(bonuses, "type") {
        push_parallel_childs(g, ctx, bonuses)?;
        push_after_finishing_childs(g, &php::arr())?;
        return Ok(());
    }
    // $immediate and $after are undefined in PHP here: pushParallelChilds(null) does nothing,
    // pushAfterFinishingChilds(null) does nothing
    Ok(())
}

/// `checkListeners($method, $player, $args, $overrideMethod)`
// PHP: Models/Action.php:237
pub fn check_listeners(a: &dyn ActionClass, g: &mut Game, ctx: Ctx, method: &str, player: PId, args: &Value, override_method: Option<&str>) -> R<()> {
    let base = json!({
        "pId": player,
        "type": "action",
        "action": override_method.unwrap_or(a.name()),
        "method": override_method.unwrap_or(method),
    });
    let event = php::array_merge(&[&base, args]);
    let reaction = cards::get_reaction(g, &event, true, true)?;
    if php::empty(&reaction) {
        return Ok(());
    }
    let reaction = filter_grouped_reactions(g, reaction, &event)?;
    let mut immediate = vec![];
    let mut deferred = vec![];
    for child in php::vals(&reaction) {
        if php::get(child, "immediate") == &json!(true) {
            let mut c = child.clone();
            php::unset(&mut c, "immediate");
            immediate.push(c);
        } else {
            deferred.push(child.clone());
        }
    }
    if !immediate.is_empty() {
        push_parallel_childs(g, ctx, &Value::Array(immediate))?;
    }
    push_after_finishing_childs(g, &Value::Array(deferred))
}

/// `filterGroupedReactions($reaction, $event)`
// PHP: Models/Action.php:283
pub fn filter_grouped_reactions(g: &mut Game, reaction: Value, event: &Value) -> R<Value> {
    if php::empty(&reaction) || php::get(event, "gainGroup").is_null() {
        return Ok(reaction);
    }
    let group = php::get(event, "gainGroup").clone();
    let gain_card = php::coalesce(php::get(event, "cardId"), php::coalesce(php::get(php::get(event, "gain"), "cardId"), &Value::Null)).clone();
    let mut out = vec![];
    for child in php::vals(&reaction) {
        let cid = php::get(php::get(child, "args"), "cardId").clone();
        if !engine::has_pending_grouped_reaction(g, &cid, &gain_card, &group)? {
            out.push(child.clone());
        }
    }
    Ok(Value::Array(out))
}

/// `logReactions($method, $player, $args, $overrideMethod)`: the reactions, not queued
// PHP: Models/Action.php:299
pub fn log_reactions(a: &dyn ActionClass, g: &mut Game, method: &str, player: PId, args: &Value, override_method: &Value) -> R<Value> {
    let action = if php::loose_eq(override_method, &json!(true)) { method.to_string() } else { a.name().to_string() };
    let base = json!({"pId": player, "type": "action", "action": action, "method": method});
    let event = php::array_merge(&[&base, args]);
    cards::get_reaction(g, &event, true, true)
}

/// `checkAfterListeners($player, $args, $duringActionListener, $overrideMethod)`
// PHP: Models/Action.php:326
pub fn check_after_listeners(a: &dyn ActionClass, g: &mut Game, ctx: Ctx, player: PId, args: &Value, during: bool, override_method: Option<&str>) -> R<()> {
    if during {
        check_listeners(a, g, ctx, a.name(), player, args, override_method)?;
    }
    Ok(())
}

/// `checkImmediateListeners($player, $args)`: reactions inserted first at root
// PHP: Models/Action.php:336
pub fn check_immediate_listeners(a: &dyn ActionClass, g: &mut Game, player: PId, args: &Value) -> R<()> {
    let name = format!("Immediate{}", a.name());
    let base = json!({"pId": player, "type": "action", "action": name, "method": name});
    let event = php::array_merge(&[&base, args]);
    let reaction = cards::get_reaction(g, &event, true, false)?;
    if !reaction.is_null() {
        if php::count(&reaction) > 1 {
            engine::insert_at_root(g, &json!({"type": "parallel", "pId": php::get(&event, "pId"), "noIndependent": true, "childs": reaction}), false)?;
        } else {
            engine::insert_at_root(g, &json!({"type": "seq", "childs": reaction}), false)?;
        }
    }
    Ok(())
}

/// `duplicateAction($args, $checkpoint)`: a copy of the ctx inserted right after it
// PHP: Models/Action.php:391
pub fn duplicate_action(g: &mut Game, ctx: Ctx, args: &Value, checkpoint: bool) -> R<()> {
    let n = ctx_node(ctx, "toArray")?;
    let mut nd = node::to_array(g, n);
    php::set(&mut nd, "type", json!("leaf"));
    php::set(&mut nd, "childs", php::arr());
    let merged = php::array_merge(&[php::get(&nd, "args"), args]);
    php::set(&mut nd, "args", merged);
    php::set(&mut nd, "duplicate", json!(true));
    php::unset(&mut nd, "mandatory");
    let built = engine::build_tree(g, &nd)?;
    node::insert_as_brother(g, n, built)?;
    engine::save(g)?;
    if checkpoint {
        engine::checkpoint(g)?;
    }
    Ok(())
}

/// `updateCardId($node, $cardId, $cardFrom, $sourceId, $ownerId, $preserveEffectPlaceholder)`
// PHP: Models/Action.php:442
pub fn update_card_id(g: &mut Game, node_v: &Value, card_id: &Value, card_from: &Value, source_id: &Value, owner_id: &Value, preserve: bool) -> R<Value> {
    let mut nd = node_v.clone();
    let is_target = php::get(&nd, "action") == &json!("Target");
    let is_spend = php::get(&nd, "action") == &json!("Spend");
    let child_preserve = preserve || is_spend;
    let cid = php::get(php::get(&nd, "args"), "cardId").clone();
    let keep_placeholder = cid == json!("me") || cid == json!("mana") || (preserve && cid == json!("EFFECT"));
    let needs_prior = is_target && target_needs_prior_selection_ctx(&nd);
    if !is_target || needs_prior {
        if !php::isset(php::get(&nd, "args"), "cardId") || !keep_placeholder {
            if !is_target || !card_id.is_null() {
                let args = php::get_mut(&mut nd, "args");
                php::set(args, "cardId", card_id.clone());
                php::set(args, "cardFrom", card_from.clone());
                php::set(args, "ownerId", owner_id.clone());
                if !card_id.is_null() && !php::is_arr(card_id) && php::in_list(card_from, &["stormLeft", "stormRight"]) {
                    let found = cards::get_raw(g, card_id, false)?;
                    if let Some(c) = found.card() {
                        let gig = crate::models::card::is_gigantic(g, c, false)?;
                        let args = php::get_mut(&mut nd, "args");
                        php::set(args, "wasGigantic", gig);
                    }
                }
            }
        }
    }
    if php::isset(&nd, "pId") && php::loose_eq(php::get(&nd, "pId"), &json!("owner")) {
        php::set(&mut nd, "pId", owner_id.clone());
    }
    if php::isset(&nd, "1-3") {
        let x = update_card_id(g, php::get(&nd, "1-3"), card_id, card_from, source_id, owner_id, child_preserve)?;
        php::set(&mut nd, "1-3", x);
    }
    if php::isset(&nd, "4+") {
        let x = update_card_id(g, php::get(&nd, "4+"), card_id, card_from, source_id, owner_id, child_preserve)?;
        php::set(&mut nd, "4+", x);
    }
    php::set(&mut nd, "sourceId", source_id.clone());
    let effect_propagate = if is_target { Value::Null } else { card_id.clone() };
    let effect = php::get(php::get(&nd, "args"), "effect").clone();
    if php::is_arr(&effect) {
        let tl = php::get(php::get(&effect, "args"), "targetLocation").clone();
        let child_from = if !tl.is_null() { tl } else { card_from.clone() };
        let eff_preserve = if is_spend { child_preserve } else { preserve };
        let x = update_card_id(g, &effect, &effect_propagate, &child_from, source_id, owner_id, eff_preserve)?;
        php::set(php::get_mut(&mut nd, "args"), "effect", x);
    }
    let opp = php::get(php::get(&nd, "args"), "oppositeEffect").clone();
    if php::is_arr(&opp) {
        let x = update_card_id(g, &opp, card_id, card_from, source_id, owner_id, child_preserve)?;
        php::set(php::get_mut(&mut nd, "args"), "oppositeEffect", x);
    }
    if php::isset(&nd, "childs") {
        let mut out = php::arr();
        for (k, c) in php::entries(php::get(&nd, "childs")) {
            let x = update_card_id(g, c, card_id, card_from, source_id, owner_id, child_preserve)?;
            php::set(&mut out, &k, x);
        }
        php::set(&mut nd, "childs", out);
    }
    Ok(nd)
}

/// `targetNeedsPriorSelectionCtx($node)`
// PHP: Models/Action.php:519
fn target_needs_prior_selection_ctx(nd: &Value) -> bool {
    let args = php::get(nd, "args");
    if php::truthy(php::get(args, "excludePreviousTarget")) {
        return true;
    }
    if let Value::String(s) = php::get(args, "maxHandCost") {
        if s.starts_with("discard") {
            return true;
        }
    }
    let cmp = php::get(args, "compareTargetBiome");
    php::is_arr(cmp) && php::get(cmp, "source") == &json!("cardId")
}
