//! Candidate answers of a protocol v1 decision (`protocol/protocol.ts`), without the decision id.
//!
//! Every kind is enumerated. Selections of k cards among n grow fast: past `MAX_COMBOS` answers,
//! the combinations are cut (the order of `cardIds` decides which are kept), so the AI never
//! builds more than a few hundred candidates.

use serde_json::{json, Value};

pub const MAX_COMBOS: usize = 300;

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

fn int(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

/// All subsets of `items` of size `k` (in order), at most `cap`
pub fn combos(items: &[Value], k: usize, cap: usize) -> Vec<Vec<Value>> {
    let mut out = vec![];
    let mut idx: Vec<usize> = (0..k).collect();
    if k > items.len() {
        return out;
    }
    loop {
        out.push(idx.iter().map(|&i| items[i].clone()).collect());
        if out.len() >= cap {
            return out;
        }
        // Next combination in lexicographic order: the rightmost index that can still move
        let n = items.len();
        let mut i = k;
        while i > 0 && idx[i - 1] == n - k + i - 1 {
            i -= 1;
        }
        if i == 0 {
            return out;
        }
        idx[i - 1] += 1;
        for j in i..k {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

/// The answers to a decision. A `turn` gives every play (card x destination), support, exhaust
/// ability and the end of turn; `confirm` never offers the undo.
pub fn candidates(d: &Value) -> Vec<Value> {
    let mut a = vec![];
    let kind = d["kind"].as_str().unwrap_or("");
    match kind {
        "turn" => {
            for p in arr(&d["play"]) {
                for dest in arr(&p["destinations"]) {
                    a.push(json!({"type": "play", "cardId": p["cardId"], "destination": dest}));
                }
            }
            for id in arr(&d["support"]) {
                a.push(json!({"type": "support", "cardId": id}));
            }
            for id in arr(&d["tap"]) {
                a.push(json!({"type": "tap", "cardId": id}));
            }
            if d["canEndTurn"].as_bool().unwrap_or(false) {
                a.push(json!({"type": "endTurn"}));
            } else if d["canPass"].as_bool().unwrap_or(false) {
                a.push(json!({"type": "pass"}));
            }
        }
        "select" => {
            let ids = arr(&d["cardIds"]);
            let max = (int(&d["max"]).max(0) as usize).min(ids.len());
            let min = (int(&d["min"]).max(0) as usize).min(max);
            for k in min..=max {
                for c in combos(ids, k, MAX_COMBOS.saturating_sub(a.len()).max(1)) {
                    a.push(json!({"type": "select", "cardIds": c}));
                }
                if a.len() >= MAX_COMBOS {
                    break;
                }
            }
        }
        "choice" => {
            for c in arr(&d["choices"]) {
                if c["enabled"].as_bool().unwrap_or(true) {
                    a.push(json!({"type": "choice", "id": c["id"]}));
                }
            }
        }
        "expedition" => {
            let keys: Vec<Value> = arr(&d["expeditions"]).iter().map(|e| e["key"].clone()).collect();
            let max = (int(&d["max"]).max(1) as usize).min(keys.len());
            let min = (int(&d["min"]).max(1) as usize).min(max);
            for k in min..=max {
                for c in combos(&keys, k, MAX_COMBOS) {
                    a.push(json!({"type": "expedition", "keys": c}));
                }
            }
        }
        "player" => {
            for p in arr(&d["players"]) {
                a.push(json!({"type": "player", "player": p}));
            }
        }
        "number" => {
            for v in arr(&d["values"]) {
                a.push(json!({"type": "number", "value": v}));
            }
            for id in arr(&d["discardCardIds"]) {
                a.push(json!({"type": "select", "cardIds": [id]}));
            }
        }
        "location" => {
            for l in arr(&d["locations"]) {
                a.push(json!({"type": "location", "key": l["key"]}));
            }
        }
        "buttons" => {
            for b in arr(&d["buttons"]) {
                a.push(json!({"type": "button", "key": b["key"]}));
            }
        }
        "exchange" => {
            for h in arr(&d["hand"]) {
                for r in arr(&d["reserve"]) {
                    a.push(json!({"type": "exchange", "hand": h, "reserve": r}));
                }
            }
        }
        "cleanup" => {
            let r = &d["reserve"];
            let l = &d["landmarks"];
            let rs = combos(arr(&r["cardIds"]), (int(&r["n"]).max(0) as usize).min(arr(&r["cardIds"]).len()), 60);
            let ls = combos(arr(&l["cardIds"]), (int(&l["n"]).max(0) as usize).min(arr(&l["cardIds"]).len()), 20);
            for x in &rs {
                for y in &ls {
                    a.push(json!({"type": "cleanup", "reserve": x, "landmarks": y}));
                }
            }
        }
        "confirm" => a.push(json!({"type": "confirm"})),
        _ => {}
    }
    if d["canPass"].as_bool().unwrap_or(false) && kind != "turn" {
        a.push(json!({"type": "pass"}));
    }
    a
}

/// The answer with the decision's id
pub fn with_id(d: &Value, a: &Value) -> Value {
    let mut a = a.clone();
    a["decisionId"] = d["id"].clone();
    a
}

/// A canonical form of an answer (card lists sorted), to compare two answers
pub fn canonical(a: &Value) -> Value {
    let mut a = a.clone();
    if let Some(o) = a.as_object_mut() {
        o.remove("decisionId");
        for k in ["cardIds", "keys", "reserve", "landmarks"] {
            if let Some(Value::Array(v)) = o.get_mut(k) {
                v.sort_by_key(|x| x.to_string());
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_count() {
        let items: Vec<Value> = (0..9).map(|i| json!(i)).collect();
        assert_eq!(combos(&items, 3, 1000).len(), 84);
        assert_eq!(combos(&items, 0, 1000).len(), 1);
        assert_eq!(combos(&items, 9, 1000).len(), 1);
        assert_eq!(combos(&items, 10, 1000).len(), 0);
        assert_eq!(combos(&items, 3, 10).len(), 10);
    }

    #[test]
    fn turn_answers() {
        let d = json!({"kind": "turn", "play": [{"cardId": 5, "destinations": ["heroExpedition", "reserve"]}], "support": [7], "tap": [], "canEndTurn": true});
        let c = candidates(&d);
        assert_eq!(c.len(), 4);
        assert_eq!(c[3], json!({"type": "endTurn"}));
    }
}
