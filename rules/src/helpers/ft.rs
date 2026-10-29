//! Helpers/FT.php: flow builders

use serde_json::{json, Map, Value};

/// `FT::SEQ(...$childs)`
pub fn seq(childs: Vec<Value>) -> Value {
    json!({"type": "seq", "childs": childs})
}

/// `FT::SEQ_DISTRIBUTE_GAINS(...$childs)`
pub fn seq_distribute_gains(childs: Vec<Value>) -> Value {
    json!({"type": "seq", "groupGains": true, "childs": childs})
}

/// `FT::SEQ_OPTIONAL(...$childs)`
pub fn seq_optional(childs: Vec<Value>) -> Value {
    json!({"type": "seq", "optional": true, "childs": childs})
}

/// `FT::SEQ_OPTIONAL_MANUAL(...$childs)`
pub fn seq_optional_manual(childs: Vec<Value>) -> Value {
    json!({"type": "seq", "optional": true, "manualChoice": true, "childs": childs})
}

/// `FT::OR(...$childs)`
pub fn or(childs: Vec<Value>) -> Value {
    json!({"type": "or", "childs": childs})
}

/// `FT::XOR(...$childs)`
pub fn xor(childs: Vec<Value>) -> Value {
    json!({"type": "xor", "childs": childs})
}

/// `FT::XOR_OPTIONAL(...$childs)`
pub fn xor_optional(childs: Vec<Value>) -> Value {
    json!({"type": "xor", "childs": childs, "optional": true})
}

/// `FT::PAR(...$childs)`
pub fn par(childs: Vec<Value>) -> Value {
    json!({"type": "parallel", "childs": childs})
}

/// `FT::ACTION($actionName, $args, $node)`: `action` and `args` set after the node's own keys
pub fn action(name: &str, args: Value, node: Value) -> Value {
    let mut m = match node {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    m.insert("action".into(), json!(name));
    m.insert("args".into(), args);
    Value::Object(m)
}

/// `FT::GAIN($card, $token, $n, $upTo)` (card id or `ME`)
pub fn gain(card_id: Value, token: &str, n: Value, up_to: Value) -> Value {
    action("Gain", json!({"cardId": card_id, "type": token, "n": n, "upTo": up_to}), json!({}))
}

/// `FT::AUGMENT($card)`
pub fn augment(card_id: Value) -> Value {
    action("Gain", json!({"cardId": card_id, "augment": true}), json!({}))
}

/// `FT::LOOSE($card, $token, $n)`
pub fn loose(card_id: Value, token: &str, n: Value) -> Value {
    action("Loose", json!({"cardId": card_id, "type": token, "n": n}), json!({}))
}

/// `FT::RETURN_TO_HAND()`
pub fn return_to_hand() -> Value {
    action("Discard", json!({"destination": "hand"}), json!({}))
}

/// `FT::DISCARD_TO_RESERVE()`
pub fn discard_to_reserve() -> Value {
    action("Discard", json!({"destination": "reserve"}), json!({}))
}

/// `FT::RUSH()`
pub fn rush() -> Value {
    action("ChooseAssignment", json!({"actions": ["play"], "mandatory": true}), json!({}))
}

/// `FT::RUSH_OPTIONAL()`
pub fn rush_optional() -> Value {
    action("ChooseAssignment", json!({"actions": ["play"], "mandatory": true}), json!({"optional": true}))
}

/// `FT::RUSH_CHARACTER()`
pub fn rush_character() -> Value {
    action("ChooseAssignment", json!({"actions": ["play"], "mandatory": true, "types": ["character"]}), json!({}))
}

/// `FT::SABOTAGE()`
pub fn sabotage() -> Value {
    action(
        "Target",
        json!({
            "targetType": ["character", "spell", "token", "permanent"],
            "targetLocation": ["reserve"],
            "upTo": true,
            "effect": action("Discard", json!({"desc": "sabotage"}), json!({})),
        }),
        json!({}),
    )
}
