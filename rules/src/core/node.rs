//! Core/Engine/{AbstractNode,SeqNode,OrNode,XorNode,ParallelNode,LeafNode}.php
//!
//! The tree is an arena: a node is a `NodeId` into `Game.tree.nodes`. Replacing a node
//! (`AbstractNode::replace`) writes the new node into the same slot, so a `NodeId` held by an
//! action (PHP keeps its ctx by reference) follows the replacement. The arena is rebuilt at each
//! request from the `engine` global (Engine::boot), so ids are only valid within a request.

use crate::core::globals;
use crate::game::{Game, PId};
use crate::managers::{actions, cards, players};
use crate::php;
use crate::{PhpError, R};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub type NodeId = usize;

pub const PASS: i64 = 99;
pub const ZOMBIE: i64 = 98;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Seq,
    Or,
    Xor,
    Parallel,
    Leaf,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Seq => "seq",
            NodeKind::Or => "or",
            NodeKind::Xor => "xor",
            NodeKind::Parallel => "parallel",
            NodeKind::Leaf => "leaf",
        }
    }
    /// `ucfirst($type) . 'Node'` must name a class
    pub fn from_str(s: &str) -> R<NodeKind> {
        match s {
            "seq" => Ok(NodeKind::Seq),
            "or" => Ok(NodeKind::Or),
            "xor" => Ok(NodeKind::Xor),
            "parallel" => Ok(NodeKind::Parallel),
            "leaf" => Ok(NodeKind::Leaf),
            _ => Err(PhpError::new(format!("Error: Class \"ALT\\Core\\Engine\\{}Node\" not found", php::ucfirst(s)))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub infos: Map<String, Value>,
    pub childs: Vec<NodeId>,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// `Engine::$tree`
    pub root: Option<NodeId>,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// `new <Kind>Node($infos, $childs)`: attaches the children (a LeafNode ignores them) and forces
/// `infos['type']`
// PHP: Core/Engine/AbstractNode.php:21
pub fn new_node(g: &mut Game, kind: NodeKind, infos: Map<String, Value>, childs: Vec<NodeId>) -> NodeId {
    let id = g.tree.nodes.len();
    let childs = if kind == NodeKind::Leaf { vec![] } else { childs };
    for c in &childs {
        g.tree.nodes[*c].parent = Some(id);
    }
    let mut infos = infos;
    infos.insert("type".into(), json!(kind.as_str()));
    g.tree.nodes.push(Node { infos, childs, parent: None, kind });
    id
}

/// `Engine::buildTree($t)`
// PHP: Core/Engine.php:68
pub fn build_tree(g: &mut Game, t: &Value) -> R<NodeId> {
    let mut infos: Map<String, Value> = match t {
        Value::Object(m) => m.clone(),
        Value::Array(a) => a.iter().enumerate().map(|(i, v)| (i.to_string(), v.clone())).collect(),
        // `$t['childs'] = ...` turns null (and false, deprecated) into an array
        Value::Null | Value::Bool(false) => Map::new(),
        _ => return Err(PhpError::new("Error: Cannot use a scalar value as an array (buildTree)")),
    };
    let childs_v = infos.get("childs").cloned().unwrap_or(Value::Null);
    let childs_v = if childs_v.is_null() { php::arr() } else { childs_v };
    let type_v = infos.get("type").cloned().unwrap_or(Value::Null);
    let ty = if type_v.is_null() {
        if php::empty(&childs_v) {
            "leaf".to_string()
        } else {
            "seq".to_string()
        }
    } else {
        php::strval(&type_v)
    };
    let mut childs = vec![];
    for c in php::vals(&childs_v) {
        childs.push(build_tree(g, c)?);
    }
    let kind = NodeKind::from_str(&ty)?;
    infos.shift_remove("childs");
    Ok(new_node(g, kind, infos, childs))
}

/// `AbstractNode::toArray()`
// PHP: Core/Engine/AbstractNode.php:130
pub fn to_array(g: &Game, id: NodeId) -> Value {
    let n = &g.tree.nodes[id];
    let mut m = n.infos.clone();
    let childs: Vec<Value> = n.childs.iter().map(|c| to_array(g, *c)).collect();
    m.shift_remove("childs");
    m.insert("childs".into(), Value::Array(childs));
    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Tree utilities
// ---------------------------------------------------------------------------

pub fn node(g: &Game, id: NodeId) -> &Node {
    &g.tree.nodes[id]
}

pub fn info(g: &Game, id: NodeId, k: &str) -> Value {
    g.tree.nodes[id].infos.get(k).cloned().unwrap_or(Value::Null)
}

pub fn kind(g: &Game, id: NodeId) -> NodeKind {
    g.tree.nodes[id].kind
}

pub fn childs(g: &Game, id: NodeId) -> Vec<NodeId> {
    g.tree.nodes[id].childs.clone()
}

pub fn parent(g: &Game, id: NodeId) -> Option<NodeId> {
    g.tree.nodes[id].parent
}

/// `AbstractNode::getIndex()`
// PHP: Core/Engine/AbstractNode.php:46
pub fn get_index(g: &Game, id: NodeId) -> R<Option<usize>> {
    let Some(p) = g.tree.nodes[id].parent else { return Ok(None) };
    match g.tree.nodes[p].childs.iter().position(|c| *c == id) {
        Some(i) => Ok(Some(i)),
        None => Err(PhpError::new("Can't find index of a child")),
    }
}

/// `AbstractNode::replace($newNode)`: the new node takes the place (and the id) of this one
// PHP: Core/Engine/AbstractNode.php:60
pub fn replace(g: &mut Game, id: NodeId, new: NodeId) -> R<NodeId> {
    if get_index(g, id)?.is_none() {
        return Err(PhpError::new("Trying to replace the root"));
    }
    let parent = g.tree.nodes[id].parent;
    let n = std::mem::replace(
        &mut g.tree.nodes[new],
        Node { infos: Map::new(), childs: vec![], parent: None, kind: NodeKind::Leaf },
    );
    for c in &n.childs {
        g.tree.nodes[*c].parent = Some(id);
    }
    g.tree.nodes[id] = Node { infos: n.infos, childs: n.childs, parent, kind: n.kind };
    Ok(id)
}

/// `AbstractNode::replaceAtPos($node, $index)`
pub fn replace_at_pos(g: &mut Game, id: NodeId, new: NodeId, index: usize) -> NodeId {
    g.tree.nodes[id].childs[index] = new;
    g.tree.nodes[new].parent = Some(id);
    new
}

/// `AbstractNode::pushChild($child)`
pub fn push_child(g: &mut Game, id: NodeId, child: NodeId) {
    g.tree.nodes[id].childs.push(child);
    g.tree.nodes[child].parent = Some(id);
}

/// `AbstractNode::unshiftChild($child)`
pub fn unshift_child(g: &mut Game, id: NodeId, child: NodeId) {
    g.tree.nodes[id].childs.insert(0, child);
    g.tree.nodes[child].parent = Some(id);
}

/// `AbstractNode::insertChildAtPos($node, $index)`: `array_splice($childs, $index + 1, 0, [$node])`
pub fn insert_child_at_pos(g: &mut Game, id: NodeId, new: NodeId, index: usize) -> NodeId {
    let len = g.tree.nodes[id].childs.len();
    g.tree.nodes[id].childs.insert((index + 1).min(len), new);
    g.tree.nodes[new].parent = Some(id);
    new
}

/// `AbstractNode::insertAsBrother($newNode)`: wraps this node into a SEQ if the parent is not one
// PHP: Core/Engine/AbstractNode.php:75
pub fn insert_as_brother(g: &mut Game, id: NodeId, new: NodeId) -> R<NodeId> {
    let Some(index) = get_index(g, id)? else {
        return Err(PhpError::new("Trying to insert a brother of the root"));
    };
    let parent = g.tree.nodes[id].parent.unwrap();
    if g.tree.nodes[parent].kind != NodeKind::Seq {
        let np = new_node(g, NodeKind::Seq, Map::new(), vec![]);
        let np = replace_at_pos(g, parent, np, index);
        push_child(g, np, id);
    }
    let p = g.tree.nodes[id].parent.unwrap();
    Ok(insert_child_at_pos(g, p, new, index))
}

pub fn set_info(g: &mut Game, id: NodeId, k: &str, v: Value) {
    g.tree.nodes[id].infos.insert(k.to_string(), v);
}

pub fn unset_info(g: &mut Game, id: NodeId, k: &str) {
    g.tree.nodes[id].infos.shift_remove(k);
}

// ---------------------------------------------------------------------------
// Getters (sugar)
// ---------------------------------------------------------------------------

/// `getState()` of a non-leaf node
pub fn get_state_raw(g: &Game, id: NodeId) -> Value {
    info(g, id, "state")
}

/// `AbstractNode::getPId()` and `ParallelNode::getPId()`
// PHP: Core/Engine/AbstractNode.php:223, Core/Engine/ParallelNode.php:38
pub fn get_pid(g: &mut Game, id: NodeId) -> R<Value> {
    if kind(g, id) == NodeKind::Parallel {
        let mut counts = php::arr();
        for c in childs(g, id) {
            if is_resolved(g, c)? {
                continue;
            }
            let p = get_pid(g, c)?;
            let key = if p.is_null() { "-1".to_string() } else { php::key_of(&p) };
            let n = php::intval(php::get(&counts, &key)) + 1;
            php::set(&mut counts, &key, json!(n));
        }
        if php::count(&counts) == 1 && php::isset(&counts, "-1") {
            return Ok(Value::Null);
        } else if php::count(&counts) == 1 {
            return Ok(php::keys(&counts)[0].clone());
        }
        let p = info(g, id, "pId");
        if !p.is_null() {
            return Ok(p);
        }
        return globals::get(g, "activePId");
    }
    abstract_get_pid(g, id)
}

fn abstract_get_pid(g: &mut Game, id: NodeId) -> R<Value> {
    let p = info(g, id, "pId");
    if p.is_null() {
        return Ok(Value::Null);
    }
    if php::loose_eq(&p, &json!("nextPlayer")) {
        let active = players::get_active_id(g);
        let next = players::get_next_id(g, active)?;
        return Ok(json!(next));
    } else if php::loose_eq(&p, &json!("active")) {
        return Ok(json!(players::get_active_id(g)));
    } else if php::loose_eq(&p, &json!("source")) {
        let sid = get_source_id(g, id);
        if sid.is_null() {
            return Ok(Value::Null);
        }
        let c = cards::get(g, &sid)?;
        return Ok(json!(g.card(c)?.pid));
    }
    Ok(p)
}

/// The player object of a pId value (`Players::get($pId)`; null when no such player)
pub fn player_of(g: &mut Game, pid: &Value) -> Option<PId> {
    players::get_by_value(g, pid)
}

pub fn get_type(g: &Game, id: NodeId) -> String {
    let t = info(g, id, "type");
    if t.is_null() {
        "leaf".into()
    } else {
        php::strval(&t)
    }
}

pub fn get_flag(g: &Game, id: NodeId) -> Value {
    info(g, id, "flag")
}

/// `getArgs()`: `infos['args'] ?? null`
pub fn get_args(g: &Game, id: NodeId) -> Value {
    info(g, id, "args")
}

pub fn get_card_id(g: &Game, id: NodeId) -> Value {
    info(g, id, "cardId")
}

pub fn get_source(g: &Game, id: NodeId) -> Value {
    info(g, id, "source")
}

pub fn get_optional(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "optional"))
}

pub fn get_source_id(g: &Game, id: NodeId) -> Value {
    info(g, id, "sourceId")
}

pub fn get_event(g: &Game, id: NodeId) -> Value {
    info(g, id, "event")
}

pub fn get_action(g: &Game, id: NodeId) -> Value {
    info(g, id, "action")
}

pub fn force_confirmation(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "forceConfirmation"))
}

pub fn is_reusable(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "reusable"))
}

pub fn is_resolving_parent(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "resolveParent"))
}

pub fn get_count_choices(g: &Game, id: NodeId) -> i64 {
    php::intval(&info(g, id, "countChoices"))
}

pub fn get_resolution_args(g: &Game, id: NodeId) -> Value {
    info(g, id, "resolutionArgs")
}

pub fn is_action_resolved(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "actionResolved"))
}

pub fn get_action_resolution_args(g: &Game, id: NodeId) -> Value {
    info(g, id, "actionResolutionArgs")
}

pub fn is_manual_choice(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "manualChoice"))
}

pub fn is_mandatory(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "mandatory"))
}

pub fn enforce_mandatory(g: &mut Game, id: NodeId) {
    set_info(g, id, "mandatory", json!(true));
}

pub fn flag_st_pre_action(g: &mut Game, id: NodeId) {
    set_info(g, id, "flag", json!("preActionDone"));
}

/// `AbstractNode::setFlag` writes `flagToto`
pub fn set_flag(g: &mut Game, id: NodeId, v: Value) {
    set_info(g, id, "flagToto", v);
}

/// `AbstractNode::resolve($args)`
pub fn resolve(g: &mut Game, id: NodeId, args: Value) {
    set_info(g, id, "resolved", json!(true));
    set_info(g, id, "resolutionArgs", args);
}

/// `AbstractNode::resolveAction($args)`
pub fn resolve_action(g: &mut Game, id: NodeId, args: Value) {
    set_info(g, id, "actionResolved", json!(true));
    set_info(g, id, "actionResolutionArgs", args);
    set_info(g, id, "optional", json!(false));
}

/// `AbstractNode::unresolveAction()`
pub fn unresolve_action(g: &mut Game, id: NodeId) {
    unset_info(g, id, "actionResolved");
    unset_info(g, id, "actionResolutionArgs");
    unset_info(g, id, "optional");
}

pub fn unchoose(g: &mut Game, id: NodeId) {
    unset_info(g, id, "choice");
}

// ---------------------------------------------------------------------------
// Memoization (AbstractNode::memoized)
// ---------------------------------------------------------------------------

fn memo_key(id: NodeId, what: &str, player: Option<PId>) -> String {
    format!("node|{}|{}|{}", id, what, player.map(|p| p.to_string()).unwrap_or_default())
}

fn memo_bool(g: &mut Game, id: NodeId, what: &str, player: Option<PId>, f: impl FnOnce(&mut Game) -> R<bool>) -> R<bool> {
    let v = g.eval_run(memo_key(id, what, player), |g| Ok(json!(f(g)?)))?;
    Ok(php::truthy(&v))
}

fn engine_choices(g: &mut Game) -> R<i64> {
    globals::get_int(g, "engineChoices")
}

fn player_id(player: Option<PId>) -> R<PId> {
    player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

fn abstract_is_resolved(g: &Game, id: NodeId) -> bool {
    php::truthy(&info(g, id, "resolved"))
}

/// `isResolved()` of each node class
pub fn is_resolved(g: &mut Game, id: NodeId) -> R<bool> {
    match kind(g, id) {
        // PHP: Core/Engine/SeqNode.php:62, ParallelNode.php:101
        NodeKind::Seq | NodeKind::Parallel => {
            if abstract_is_resolved(g, id) {
                return Ok(true);
            }
            for c in childs(g, id) {
                if !is_resolved(g, c)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // PHP: Core/Engine/OrNode.php:68
        NodeKind::Or => {
            if abstract_is_resolved(g, id) {
                return Ok(true);
            }
            if or_get_remaining_choices(g, id)? <= 0 {
                let choice = info(g, id, "choice");
                let c = child_at(g, id, &choice)?;
                return is_resolved(g, c);
            }
            Ok(false)
        }
        // PHP: Core/Engine/XorNode.php:57
        NodeKind::Xor => {
            if abstract_is_resolved(g, id) {
                return Ok(true);
            }
            for c in childs(g, id) {
                if is_resolved(g, c)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        // PHP: Core/Engine/LeafNode.php:23
        NodeKind::Leaf => Ok(abstract_is_resolved(g, id) || (!get_action(g, id).is_null() && is_action_resolved(g, id))),
    }
}

/// `$this->childs[$index]` (fatal error when missing)
fn child_at(g: &Game, id: NodeId, index: &Value) -> R<NodeId> {
    let k = php::key_of(index);
    match php::int_key(&k) {
        Some(i) if i >= 0 && (i as usize) < g.tree.nodes[id].childs.len() => Ok(g.tree.nodes[id].childs[i as usize]),
        _ => Err(PhpError::new("Error: Call to a member function isResolved() on null")),
    }
}

/// `getNextUnresolved()`
// PHP: Core/Engine/AbstractNode.php:330, SeqNode.php:73
pub fn get_next_unresolved(g: &mut Game, id: NodeId) -> R<Option<NodeId>> {
    if is_resolved(g, id)? {
        return Ok(None);
    }
    if kind(g, id) == NodeKind::Seq {
        let active = players::get_active(g);
        if is_optional(g, id, active)? {
            return Ok(Some(id));
        }
        for c in childs(g, id) {
            if !is_resolved(g, c)? {
                return get_next_unresolved(g, c);
            }
        }
        return Ok(None);
    }
    let choice = info(g, id, "choice");
    if choice.is_null() {
        return Ok(Some(id));
    }
    let c = child_at(g, id, &choice)?;
    if is_resolved(g, c)? {
        Ok(Some(id))
    } else {
        get_next_unresolved(g, c)
    }
}

// ---------------------------------------------------------------------------
// OR helpers
// ---------------------------------------------------------------------------

/// `OrNode::getN()`
// PHP: Core/Engine/OrNode.php:77
pub fn or_get_n(g: &mut Game, id: NodeId) -> R<Value> {
    let mut args = get_args(g, id);
    let n = php::get(&args, "n").clone();
    let nchilds = childs(g, id).len() as i64;
    if php::isset(&args, "n") && php::loose_eq(&n, &json!("sourceCounter1")) {
        let sid = get_source_id(g, id);
        if sid.is_null() {
            return Ok(json!(nchilds));
        }
        let source = cards::get(g, &sid)?;
        let extra = crate::models::card::get(g, source, "extraDatas")?;
        let counter = php::intval(php::coalesce(php::get(&extra, "counter"), &json!(0)));
        php::set(&mut args, "n", json!(counter + 1));
    } else if php::isset(&args, "n") && php::loose_eq(&n, &json!("landmarks")) {
        let sid = get_source_id(g, id);
        if sid.is_null() {
            return Ok(json!(nchilds));
        }
        let source = cards::get(g, &sid)?;
        let pid = g.card(source)?.pid;
        let lm = crate::models::player::get_landmarks(g, pid)?.len() as i64;
        php::set(&mut args, "n", json!(lm));
        let can_reuse = php::get(&args, "canReuse").clone();
        if php::isset(&args, "canReuse") || php::loose_eq(&can_reuse, &json!(false)) {
            let cur = php::intval(php::get(&args, "n"));
            php::set(&mut args, "n", json!(cur.min(nchilds)));
        }
        set_info(g, id, "args", args.clone());
    }
    if php::truthy(&args) && php::isset(&args, "n") {
        Ok(php::get(&args, "n").clone())
    } else {
        Ok(json!(nchilds))
    }
}

/// `OrNode::getRemainingChoices()`
pub fn or_get_remaining_choices(g: &mut Game, id: NodeId) -> R<i64> {
    let n = or_get_n(g, id)?;
    Ok(php::intval(&n) - get_count_choices(g, id))
}

fn resolved_not_pass(g: &mut Game, c: NodeId) -> R<bool> {
    Ok(is_resolved(g, c)? && php::loose_ne(&get_resolution_args(g, c), &json!(PASS)))
}

/// `areChildrenOptional()` (true only for an OR with a child resolved otherwise than by PASS)
// PHP: Core/Engine/OrNode.php:54
pub fn are_children_optional(g: &mut Game, id: NodeId) -> R<bool> {
    if kind(g, id) != NodeKind::Or {
        return Ok(false);
    }
    for c in childs(g, id) {
        if resolved_not_pass(g, c)? {
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/// `AbstractNode::isOptional`: `infos['optional'] ?? (parent && parent->areChildrenOptional())`
fn abstract_is_optional(g: &mut Game, id: NodeId) -> R<bool> {
    let o = info(g, id, "optional");
    if !o.is_null() {
        return Ok(php::truthy(&o));
    }
    match parent(g, id) {
        Some(p) => are_children_optional(g, p),
        None => Ok(false),
    }
}

/// `isOptional($player)` of each node class
pub fn is_optional(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    match kind(g, id) {
        NodeKind::Seq => abstract_is_optional(g, id),
        // PHP: Core/Engine/OrNode.php:42
        NodeKind::Or => {
            if abstract_is_optional(g, id)? {
                return Ok(true);
            }
            if !php::get(&get_args(g, id), "n").is_null() {
                return Ok(false);
            }
            for c in childs(g, id) {
                if resolved_not_pass(g, c)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        // PHP: Core/Engine/XorNode.php:42
        NodeKind::Xor => {
            if abstract_is_optional(g, id)? {
                return Ok(true);
            }
            for c in childs(g, id) {
                if is_doable(g, c, player)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // PHP: Core/Engine/ParallelNode.php:69
        NodeKind::Parallel => {
            let mut child_with_other_player = true;
            for c in childs(g, id) {
                let cp = get_pid(g, c)?;
                let cp = if cp.is_null() { json!(player_id(player)?) } else { cp };
                let me = json!(player.unwrap_or(-1));
                let r = php::loose_ne(&cp, &me) && !is_resolved(g, c)?;
                if !r {
                    child_with_other_player = false;
                    break;
                }
            }
            if child_with_other_player {
                return Ok(false);
            }
            let pid = get_pid(g, id)?;
            if !pid.is_null() {
                let pid2 = get_pid(g, id)?;
                if php::loose_ne(&pid2, &json!(player_id(player)?)) {
                    return Ok(false);
                }
            }
            if abstract_is_optional(g, id)? {
                return Ok(true);
            }
            for c in childs(g, id) {
                if !(is_optional(g, c, player)? || is_resolved(g, c)?) {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // PHP: Core/Engine/LeafNode.php:48
        NodeKind::Leaf => memo_bool(g, id, "isOptional", player, |g| leaf_compute_is_optional(g, id, player)),
    }
}

// PHP: Core/Engine/LeafNode.php:55
fn leaf_compute_is_optional(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    if is_mandatory(g, id) {
        return Ok(false);
    }
    let action = get_action(g, id);
    let a = actions::get(&action)?;
    if a.is_mandatory(g, Some(id))? {
        return Ok(false);
    }
    let pid = get_pid(g, id)?;
    if !pid.is_null() {
        let pid2 = get_pid(g, id)?;
        if php::loose_ne(&pid2, &json!(player_id(player)?)) {
            return Ok(false);
        }
    }
    if abstract_is_optional(g, id)? || action.is_null() {
        return abstract_is_optional(g, id);
    }
    if a.is_optional(g, Some(id), player)? {
        return Ok(true);
    }
    Ok(!a.is_doable(g, Some(id), player)?)
}

/// `isDoable($player)` of each node class
pub fn is_doable(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    match kind(g, id) {
        // PHP: Core/Engine/SeqNode.php:32
        NodeKind::Seq => {
            let Some(c) = childs(g, id).first().copied() else { return Ok(false) };
            Ok(is_doable(g, c, player)? || is_optional(g, c, player)?)
        }
        // PHP: Core/Engine/OrNode.php:31
        NodeKind::Or => {
            if is_optional(g, id, player)? {
                return Ok(true);
            }
            for c in childs(g, id) {
                if is_doable(g, c, player)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        // PHP: Core/Engine/XorNode.php:31
        NodeKind::Xor => {
            if php::truthy(&info(g, id, "optional")) {
                return Ok(true);
            }
            for c in childs(g, id) {
                if is_doable(g, c, player)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        // PHP: Core/Engine/ParallelNode.php:30
        NodeKind::Parallel => {
            if is_optional(g, id, player)? {
                return Ok(true);
            }
            for c in childs(g, id) {
                if !(is_doable(g, c, player)? || is_optional(g, c, player)?) {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // PHP: Core/Engine/LeafNode.php:84
        NodeKind::Leaf => memo_bool(g, id, "isDoable", player, |g| {
            if g.tree.nodes[id].infos.contains_key("willBeDoable") && !info(g, id, "willBeDoable").is_null() {
                return Ok(true);
            }
            if is_resolved(g, id)? {
                return Ok(true);
            }
            let action = get_action(g, id);
            if !action.is_null() {
                let p = player_id(player)?;
                // Player::canTakeAction = Actions::isDoable
                return actions::is_doable(g, &action, Some(id), Some(p));
            }
            Err(PhpError::new("Unimplemented isDoable function for non-action Leaf"))
        }),
    }
}

/// `isAutomatic($player)`
// PHP: Core/Engine/AbstractNode.php:423, LeafNode.php:28
pub fn is_automatic(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    if kind(g, id) == NodeKind::Leaf {
        let action = get_action(g, id);
        if action.is_null() {
            return Ok(false);
        }
        return memo_bool(g, id, "isAutomatic", player, |g| {
            let a = actions::get(&action)?;
            Ok(a.is_automatic(g, Some(id), player)? && engine_choices(g)? <= 20)
        });
    }
    let choices = get_choices(g, id, player, false)?;
    Ok(php::count(&choices) < 2 && engine_choices(g)? <= 20)
}

/// `isIndependent($player)`
// PHP: Core/Engine/AbstractNode.php:430, LeafNode.php:38
pub fn is_independent(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    if kind(g, id) == NodeKind::Leaf {
        let action = get_action(g, id);
        if action.is_null() {
            return Ok(false);
        }
        return memo_bool(g, id, "isIndependent", player, |g| {
            let a = actions::get(&action)?;
            Ok(a.is_independent(g, Some(id), player)? && engine_choices(g)? <= 20)
        });
    }
    memo_bool(g, id, "isIndependent", player, |g| {
        if php::truthy(&info(g, id, "noIndependent")) {
            return Ok(false);
        }
        if !is_automatic(g, id, player)? {
            return Ok(false);
        }
        for c in childs(g, id) {
            if !is_independent(g, c, player)? {
                return Ok(false);
            }
        }
        Ok(engine_choices(g)? <= 20)
    })
}

/// `isIrreversible($player)`
// PHP: Core/Engine/AbstractNode.php:539, LeafNode.php:73
pub fn is_irreversible(g: &mut Game, id: NodeId, player: Option<PId>) -> R<bool> {
    if kind(g, id) != NodeKind::Leaf {
        return Ok(false);
    }
    let action = get_action(g, id);
    if action.is_null() {
        return Ok(false);
    }
    let a = actions::get(&action)?;
    a.is_irreversible(g, Some(id), player)
}

/// `getUndoableMandatoryNode($player)`
// PHP: Core/Engine/AbstractNode.php:289, SeqNode.php:44, ParallelNode.php:84
pub fn get_undoable_mandatory_node(g: &mut Game, id: NodeId, player: Option<PId>) -> R<Option<NodeId>> {
    match kind(g, id) {
        NodeKind::Seq | NodeKind::Parallel => {
            if is_optional(g, id, player)? {
                return Ok(None);
            }
            for c in childs(g, id) {
                if let Some(n) = get_undoable_mandatory_node(g, c, player)? {
                    return Ok(Some(n));
                }
            }
            Ok(None)
        }
        _ => {
            if !is_resolved(g, id)? && !is_doable(g, id, player)? && (is_mandatory(g, id) || !is_optional(g, id, player)?) {
                return Ok(Some(id));
            }
            Ok(None)
        }
    }
}

/// `getState()`
// PHP: Core/Engine/AbstractNode.php:218, LeafNode.php:112
pub fn get_state(g: &mut Game, id: NodeId) -> R<Value> {
    if kind(g, id) != NodeKind::Leaf {
        return Ok(get_state_raw(g, id));
    }
    let s = info(g, id, "state");
    if !s.is_null() {
        return Ok(s);
    }
    let action = get_action(g, id);
    if !action.is_null() {
        return actions::get_state(g, &action, Some(id));
    }
    Err(PhpError::new("Trying to get state on a leaf without state nor action"))
}

fn separator(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Seq => ", ",
        NodeKind::Or => " + ",
        NodeKind::Xor => " / ",
        NodeKind::Parallel => " | ",
        NodeKind::Leaf => "",
    }
}

/// `getDescription()` (only empty vs non-empty matters for the rules, see PHP_ENGINE.md §2.6)
// PHP: Core/Engine/AbstractNode.php:164, LeafNode.php:129
pub fn get_description(g: &mut Game, id: NodeId) -> R<Value> {
    if kind(g, id) == NodeKind::Leaf {
        let action = get_action(g, id);
        if !action.is_null() {
            let a = actions::get(&action)?;
            return a.get_description(g, Some(id));
        }
    }
    let custom = info(g, id, "customDescription");
    if !custom.is_null() {
        let d = php::strval(&custom);
        let pos = d.find("${eventCardName}");
        let event = info(g, id, "event");
        if pos.is_some_and(|p| p > 0) && php::isset(&event, "cardId") {
            let cid = cards::get(g, php::get(&event, "cardId"))?;
            let name = crate::models::card::get(g, cid, "name")?;
            return Ok(json!({"log": d, "args": {"eventCardName": name}}));
        }
        return Ok(custom);
    }
    let mut desc: Vec<String> = vec![];
    let mut args = php::arr();
    for (i, c) in childs(g, id).into_iter().enumerate() {
        let name = format!("action{}", i);
        let mut tmp = get_description(g, c)?;
        if php::loose_ne(&tmp, &json!("")) {
            if force_confirmation(g, c) {
                let p = get_pid(g, c)?;
                let pid = if p.is_null() { Some(players::get_active_id(g)) } else { player_of(g, &p) };
                let pname = match pid {
                    Some(p) => json!(g.player(p)?.name.clone()),
                    None => return Err(PhpError::new("Error: Call to a member function getName() on null")),
                };
                tmp = json!({"log": "Allow ${player_name} to take a triggered action", "args": {"player_name": pname}});
            }
            php::set(&mut args, &name, tmp);
            let mut i18n = php::get(&args, "i18n").clone();
            php::push(&mut i18n, json!(name));
            php::set(&mut args, "i18n", i18n);
            desc.push(format!("${{{}}}", name));
        }
    }
    Ok(json!({"log": desc.join(separator(kind(g, id))), "args": args}))
}

// ---------------------------------------------------------------------------
// Choices
// ---------------------------------------------------------------------------

/// `getChoices($player, $displayAllChoices)` (with the ParallelNode independent-first rule)
// PHP: Core/Engine/AbstractNode.php:441, ParallelNode.php:112
pub fn get_choices(g: &mut Game, id: NodeId, player: Option<PId>, display_all: bool) -> R<Value> {
    let key = memo_key(id, &format!("getChoices{}", display_all as i32), player);
    let choices = g.eval_run(key, |g| compute_choices(g, id, player, display_all))?;
    if kind(g, id) != NodeKind::Parallel {
        return Ok(choices);
    }
    if php::truthy(&info(g, id, "noIndependent")) {
        return Ok(choices);
    }
    let independent: Vec<&Value> = php::vals(&choices)
        .into_iter()
        .filter(|c| php::truthy(php::get(c, "independentAction")) && !php::truthy(php::get(c, "optionalAction")))
        .collect();
    if let Some(choice) = independent.first() {
        let mut out = php::arr();
        php::set(&mut out, &php::key_of(php::get(choice, "id")), (*choice).clone());
        return Ok(out);
    }
    Ok(choices)
}

// PHP: Core/Engine/AbstractNode.php:448
fn compute_choices(g: &mut Game, id: NodeId, player: Option<PId>, display_all: bool) -> R<Value> {
    let mut choice: Value = Value::Null;
    let mut choices = php::arr();
    let all = childs(g, id);
    let cands: Vec<(usize, NodeId)> = if get_type(g, id) == "seq" && !all.is_empty() { vec![(0, all[0])] } else { all.into_iter().enumerate().collect() };
    let mut all_optional = true;
    let mut has_one_action = false;
    let is_or = kind(g, id) == NodeKind::Or;

    for (cid, child) in cands {
        let cp = get_pid(g, child)?;
        let player_test = if !cp.is_null() {
            let p2 = get_pid(g, child)?;
            player_of(g, &p2)
        } else if player.is_none() {
            players::get_active(g)
        } else {
            player
        };
        let doable = is_doable(g, child, player_test)?;
        let can_reuse = is_or && php::isset(&get_args(g, id), "canReuse");
        if (!is_resolved(g, child)? || can_reuse) && (display_all || doable) {
            let description = if get_type(g, id) == "seq" { get_description(g, id)? } else { get_description(g, child)? };
            let args = get_args(g, child);
            let optional_action = (is_optional(g, child, player_test)? && {
                let p = get_pid(g, child)?;
                let p = if p.is_null() { json!(player_id(player_test)?) } else { p };
                php::loose_eq(&p, &json!(player_id(player_test)?))
            }) || is_manual_choice(g, id);
            let automatic = is_automatic(g, child, player_test)?;
            let independent = is_independent(g, child, player_test)?;
            let irreversible = is_irreversible(g, child, player_test)?;
            let child_pid_or_test = {
                let p = get_pid(g, child)?;
                if p.is_null() {
                    json!(player_id(player_test)?)
                } else {
                    p
                }
            };
            choice = json!({
                "id": cid,
                "description": description,
                "args": args,
                "optionalAction": optional_action,
                "automaticAction": automatic,
                "independentAction": independent,
                "irreversibleAction": irreversible,
                "source": get_source(g, child),
                "sourceId": get_source_id(g, child),
                "player": child_pid_or_test,
                "forceManualChoice": is_manual_choice(g, id),
            });
            if php::loose_ne(php::get(&choice, "description"), &json!("")) || doable {
                php::set(&mut choices, &cid.to_string(), choice.clone());
            }
            if !is_optional(g, child, player_test)? {
                all_optional = false;
            }
            let p = get_pid(g, child)?;
            let p = if p.is_null() { json!(player_id(player_test)?) } else { p };
            if php::loose_eq(&p, &json!(player_id(player_test)?)) {
                has_one_action = true;
            }
        }
    }

    let add_pass = php::count(&choices) == 0 || (has_one_action && (is_optional(g, id, player)? || all_optional)) || engine_choices(g)? > 20;
    if add_pass && (php::count(&choices) != 1 || php::truthy(php::get(&choice, "optionalAction")) || php::truthy(php::get(&choice, "automaticAction"))) {
        php::set(
            &mut choices,
            &PASS.to_string(),
            json!({"id": PASS, "description": "Pass", "irreversibleAction": false, "optionalAction": false, "args": []}),
        );
    }
    Ok(choices)
}

/// `choose($childIndex, $auto)`
// PHP: Core/Engine/AbstractNode.php:514, SeqNode.php:93
pub fn choose(g: &mut Game, id: NodeId, index: i64, auto: bool) -> R<()> {
    if kind(g, id) == NodeKind::Seq {
        if index != 0 {
            return Err(PhpError::new("SEQ Choice shouldnt happen with $childIndex different from 0"));
        }
        set_info(g, id, "optional", json!(false));
        return Ok(());
    }
    set_info(g, id, "choice", json!(index));
    let cc = get_count_choices(g, id) + 1;
    set_info(g, id, "countChoices", json!(cc));
    let child = child_at(g, id, &json!(index))?;
    if kind(g, child) != NodeKind::Leaf {
        if kind(g, child) == NodeKind::Seq && get_optional(g, child) {
            choose(g, child, 0, auto)?;
        } else if !auto {
            enforce_mandatory(g, child);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// `hasPendingGroupedReaction($cardId, $gainCardId, $group)`
// PHP: Core/Engine/AbstractNode.php:372
pub fn has_pending_grouped_reaction(g: &mut Game, id: NodeId, card_id: &Value, gain_card_id: &Value, group: &Value) -> R<bool> {
    for c in childs(g, id) {
        let args = php::coalesce(&get_args(g, c), &php::arr()).clone();
        let event = php::get(&args, "event");
        let ev_card = php::coalesce(php::get(event, "cardId"), php::coalesce(php::get(php::get(event, "gain"), "cardId"), &Value::Null)).clone();
        if !is_resolved(g, c)?
            && php::loose_eq(&get_action(g, c), &json!("ActivateCard"))
            && php::loose_eq(php::get(&args, "cardId"), card_id)
            && php::loose_eq(&ev_card, gain_card_id)
            && php::loose_eq(php::get(event, "gainGroup"), group)
        {
            return Ok(true);
        }
        if has_pending_grouped_reaction(g, c, card_id, gain_card_id, group)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `clearZombieNodes($pId)`
pub fn clear_zombie_nodes(g: &mut Game, id: NodeId, pid: &Value) -> R<()> {
    for c in childs(g, id) {
        clear_zombie_nodes(g, c, pid)?;
    }
    if php::loose_eq(&get_pid(g, id)?, pid) {
        resolve(g, id, json!(ZOMBIE));
    }
    Ok(())
}

/// `getResolvedActions($types)`
pub fn get_resolved_actions(g: &mut Game, id: NodeId, types: &Value) -> Vec<NodeId> {
    let mut out = vec![];
    if php::in_array(&get_action(g, id), types) && is_action_resolved(g, id) {
        out.push(id);
    }
    for c in childs(g, id) {
        out.extend(get_resolved_actions(g, c, types));
    }
    out
}

/// `getNextSibling()`
pub fn get_next_sibling(g: &Game, id: NodeId) -> R<Option<NodeId>> {
    let idx = get_index(g, id)?.ok_or_else(|| PhpError::new("Error: Call to a member function getChilds() on null"))?;
    let p = parent(g, id).unwrap();
    Ok(g.tree.nodes[p].childs.get(idx + 1).copied())
}
