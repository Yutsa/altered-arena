//! Helpers/Collection.php on collections of pieces: a collection is the ordered list of the ids
//! it holds (the PHP Collection is keyed by id)

use serde_json::{json, Value};

/// `$a->merge($b)`: `$a + $b` (ids of `$b` already in `$a` are dropped)
pub fn merge(a: &[i64], b: &[i64]) -> Vec<i64> {
    let mut out = a.to_vec();
    for x in b {
        if !out.contains(x) {
            out.push(*x);
        }
    }
    out
}

/// `$c->limit($n)`
pub fn limit(a: &[i64], n: usize) -> Vec<i64> {
    a.iter().take(n).copied().collect()
}

/// `$c->first()`
pub fn first(a: &[i64]) -> Option<i64> {
    a.first().copied()
}

/// `$c->getIds()` as a PHP list
pub fn ids(a: &[i64]) -> Value {
    json!(a)
}

/// `$c->includes($t)` / `$c->has($id)`
pub fn has(a: &[i64], id: i64) -> bool {
    a.contains(&id)
}
