//! Helpers/Utils.php: flow tree utilities and attribute conditions

use crate::game::{CardId, Game, PId};
use crate::helpers::conditions;
use crate::php;
use crate::R;
use serde_json::{json, Value};

/// `Utils::tagTree($t, $tags, $replaceOnly)`: sets the tags on every node (childs, args.effect,
/// args.oppositeEffect), overwriting existing values
// PHP: Helpers/Utils.php:139
pub fn tag_tree(t: &Value, tags: &Value, replace_only: bool) -> Value {
    let mut t = t.clone();
    for (k, v) in php::entries(tags) {
        if !replace_only || php::isset(&t, &k) {
            php::set(&mut t, &k, v.clone());
        }
    }
    if php::isset(&t, "childs") {
        let mut out = php::arr();
        for (k, c) in php::entries(php::get(&t, "childs")) {
            php::set(&mut out, &k, tag_tree(c, tags, replace_only));
        }
        php::set(&mut t, "childs", out);
    }
    for key in ["effect", "oppositeEffect"] {
        let e = php::get(php::get(&t, "args"), key).clone();
        if php::is_arr(&e) {
            let x = tag_tree(&e, tags, replace_only);
            php::set(php::get_mut(&mut t, "args"), key, x);
        }
    }
    t
}

/// `Utils::bindOwnEffectActivateCardId($t, $cardId)`
// PHP: Helpers/Utils.php:173
pub fn bind_own_effect_activate_card_id(t: &Value, card_id: CardId) -> Value {
    if !php::is_arr(t) {
        return t.clone();
    }
    let mut t = t.clone();
    let args = php::get(&t, "args").clone();
    if php::coalesce(php::get(&t, "action"), &json!("")) == &json!("ActivateEffect")
        && php::truthy(php::get(&args, "ownEffect"))
        && php::coalesce(php::get(&args, "effectType"), &json!("")) == &json!("Support")
    {
        php::set(php::get_mut(&mut t, "args"), "cardId", json!(card_id));
    }
    if php::isset(&t, "childs") {
        let mut out = php::arr();
        for (k, c) in php::entries(php::get(&t, "childs")) {
            php::set(&mut out, &k, bind_own_effect_activate_card_id(c, card_id));
        }
        php::set(&mut t, "childs", out);
    }
    for key in ["effect", "oppositeEffect"] {
        let e = php::get(php::get(&t, "args"), key).clone();
        if php::is_arr(&e) {
            let x = bind_own_effect_activate_card_id(&e, card_id);
            php::set(php::get_mut(&mut t, "args"), key, x);
        }
    }
    t
}

/// `Utils::tagPId($t, $pId)`: sets `pId` where it is not set
pub fn tag_pid(t: &Value, pid: &Value) -> Value {
    let mut t = t.clone();
    if !php::isset(&t, "pId") {
        php::set(&mut t, "pId", pid.clone());
    }
    if php::isset(&t, "childs") {
        let mut out = php::arr();
        for (k, c) in php::entries(php::get(&t, "childs")) {
            php::set(&mut out, &k, tag_pid(c, pid));
        }
        php::set(&mut t, "childs", out);
    }
    t
}

/// `Utils::updateTree($t, $searched, $newValue, $limitedKeys)`: replaces values `=== $searched`
// PHP: Helpers/Utils.php:211
pub fn update_tree(t: &Value, searched: &Value, new_value: &Value, limited_keys: &[&str]) -> Value {
    let mut out = t.clone();
    for (k, v) in php::entries(t) {
        if limited_keys.is_empty() && php::strict_eq(v, searched) {
            php::set(&mut out, &k, new_value.clone());
        } else if limited_keys.iter().any(|lk| php::loose_eq(&php::key_value(&k), &json!(lk))) && php::strict_eq(v, searched) {
            php::set(&mut out, &k, new_value.clone());
        } else if php::is_arr(v) {
            php::set(&mut out, &k, update_tree(v, searched, new_value, limited_keys));
        }
    }
    out
}

/// `Utils::searchTree($t, $searched)`
pub fn search_tree(t: &Value, searched: &Value) -> bool {
    for (_, v) in php::entries(t) {
        if php::strict_eq(v, searched) {
            return true;
        } else if php::is_arr(v) && search_tree(v, searched) {
            return true;
        }
    }
    false
}

/// `Utils::checkAttributeCondition($attribute, $data, $player, $card, $location)`: `"value"` →
/// value; `"value:cond1:..."` → value if the conditions hold, else null
// PHP: Helpers/Utils.php:339
pub fn check_attribute_condition(g: &mut Game, _attribute: &str, data: &Value, _player: Option<PId>, card: CardId, location: &Value) -> R<Value> {
    let parts = php::explode(":", &php::strval(data));
    if parts.len() == 1 {
        return Ok(json!(parts[0]));
    }
    let cond = parts[1..].join(":");
    if conditions::check(g, &json!({"condition": cond}), Some(card), &json!({"location": location}))? {
        return Ok(json!(parts[0]));
    }
    Ok(Value::Null)
}

/// `Utils::topological_sort($nodeids, $edges)`
pub fn topological_sort(nodes: &[Value], edges: &[(Value, Value)]) -> Option<Vec<Value>> {
    let mut ins: Vec<Vec<Value>> = vec![];
    let mut outs: Vec<Vec<Value>> = vec![];
    for id in nodes {
        let mut i = vec![];
        let mut o = vec![];
        for (a, b) in edges {
            if php::loose_eq(id, a) {
                o.push(b.clone());
            }
            if php::loose_eq(id, b) {
                i.push(a.clone());
            }
        }
        ins.push(i);
        outs.push(o);
    }
    let pos = |v: &Value| nodes.iter().position(|n| php::loose_eq(n, v));
    let mut l = vec![];
    let mut s: Vec<usize> = (0..nodes.len()).filter(|i| ins[*i].is_empty()).collect();
    while !s.is_empty() {
        let id = s.remove(0);
        l.push(nodes[id].clone());
        for m in outs[id].clone() {
            if let Some(mi) = pos(&m) {
                ins[mi].retain(|x| php::strval(x) != php::strval(&nodes[id]));
                if ins[mi].is_empty() {
                    s.push(mi);
                }
            }
        }
        outs[id].clear();
    }
    if ins.iter().any(|i| !i.is_empty()) || outs.iter().any(|o| !o.is_empty()) {
        return None;
    }
    Some(l)
}
