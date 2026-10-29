//! PHP semantics on `serde_json::Value`.
//!
//! A PHP array is either a list (`Value::Array`, keys 0..n-1 in order) or an ordered map
//! (`Value::Object`, keys as strings; integer keys are their decimal form). PHP does not make
//! the difference: every helper here accepts both. `normalize` reproduces a JSON round trip
//! (`json_decode(json_encode($x), true)`): a map whose keys are 0..n-1 in order becomes a list,
//! and an empty map becomes an empty list.

use serde_json::{Map, Value};
use std::cmp::Ordering;

pub static NULL: Value = Value::Null;

// ---------------------------------------------------------------------------
// Scalars
// ---------------------------------------------------------------------------

/// PHP truthiness (`if ($x)`, `(bool) $x`, `empty()` negated)
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !(s.is_empty() || s == "0"),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `empty($x)`
pub fn empty(v: &Value) -> bool {
    !truthy(v)
}

/// PHP 8 `is_numeric` on a string
pub fn is_numeric_str(s: &str) -> bool {
    // A numeric string starts with whitespace, a sign, a digit or a dot (most strings fail here)
    match s.as_bytes().first() {
        Some(c) if c.is_ascii_digit() || matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c | b'+' | b'-' | b'.') => {}
        _ => return false,
    }
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let t = t.trim_end_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    if t.is_empty() {
        return false;
    }
    let b = t.as_bytes();
    let mut i = 0;
    if b[i] == b'+' || b[i] == b'-' {
        i += 1;
    }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let mut ed = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            ed += 1;
        }
        if ed == 0 {
            return false;
        }
    }
    i == b.len()
}

/// `is_numeric($x)`
pub fn is_numeric(v: &Value) -> bool {
    match v {
        Value::Number(_) => true,
        Value::String(s) => is_numeric_str(s),
        _ => false,
    }
}

fn str_to_f64(s: &str) -> f64 {
    // Leading numeric prefix, as PHP's (float) cast
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut end = 0;
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut seen = false;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        seen = true;
        end = i;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            seen = true;
            end = i;
        }
    }
    if seen && i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let mut ed = false;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
            ed = true;
        }
        if ed {
            end = j;
        }
    }
    if !seen {
        return 0.0;
    }
    t[..end].parse::<f64>().unwrap_or(0.0)
}

/// `intval($x)` / `(int) $x`
pub fn intval(v: &Value) -> i64 {
    match v {
        Value::Null => 0,
        Value::Bool(b) => *b as i64,
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().map(|f| f.trunc() as i64).unwrap_or(0)),
        Value::String(s) => {
            let f = str_to_f64(s);
            if f.is_finite() {
                f.trunc() as i64
            } else {
                0
            }
        }
        Value::Array(a) => !a.is_empty() as i64,
        Value::Object(o) => !o.is_empty() as i64,
    }
}

/// `(float) $x`
pub fn floatval(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => *b as i64 as f64,
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => str_to_f64(s),
        Value::Array(a) => !a.is_empty() as i64 as f64,
        Value::Object(o) => !o.is_empty() as i64 as f64,
    }
}

/// `(string) $x` (arrays give "Array")
pub fn strval(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(true) => "1".into(),
        Value::Bool(false) => String::new(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                let f = n.as_f64().unwrap_or(0.0);
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{}", f as i64)
                } else {
                    format!("{}", f)
                }
            }
        }
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => "Array".into(),
    }
}

/// `strval`, borrowing strings
pub fn strval_cow(v: &Value) -> std::borrow::Cow<'_, str> {
    match v {
        Value::String(s) => std::borrow::Cow::Borrowed(s.as_str()),
        _ => std::borrow::Cow::Owned(strval(v)),
    }
}

/// `count(array_intersect($a, $b)) > 0` for the values of `a` and the values `b` (string
/// comparison), without building the intersection
pub fn intersects(a: &Value, b: &[&Value]) -> bool {
    let a: Box<dyn Iterator<Item = &Value>> = match a {
        Value::Array(x) => Box::new(x.iter()),
        Value::Object(o) => Box::new(o.values()),
        _ => return false,
    };
    for x in a {
        let xs = strval_cow(x);
        if b.iter().any(|y| strval_cow(y) == xs) {
            return true;
        }
    }
    false
}

fn is_array(v: &Value) -> bool {
    matches!(v, Value::Array(_) | Value::Object(_))
}

/// `is_array($x)`
pub fn is_arr(v: &Value) -> bool {
    is_array(v)
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// PHP 8 loose comparison `$a == $b`
pub fn loose_eq(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Null, Null) => true,
        (Bool(x), _) => *x == truthy(b),
        (_, Bool(y)) => truthy(a) == *y,
        (Null, String(s)) | (String(s), Null) => s.is_empty(),
        (Null, Number(_)) | (Number(_), Null) => !truthy(if a.is_null() { b } else { a }),
        (Null, _) | (_, Null) => {
            let other = if a.is_null() { b } else { a };
            !truthy(other)
        }
        (Number(_), Number(_)) => num_of(a) == num_of(b),
        (Number(_), String(s)) | (String(s), Number(_)) => {
            let n = if let Number(_) = a { a } else { b };
            if is_numeric_str(s) {
                num_of(n) == Some(str_to_f64(s))
            } else {
                strval(n) == *s
            }
        }
        (String(x), String(y)) => {
            if x == y {
                // Equal numeric strings are equal numbers too
                true
            } else if is_numeric_str(x) && is_numeric_str(y) {
                str_to_f64(x) == str_to_f64(y)
            } else {
                x == y
            }
        }
        (x, y) if is_array(x) && is_array(y) => {
            let ex = entries(x);
            let ey = entries(y);
            if ex.len() != ey.len() {
                return false;
            }
            ex.iter().all(|(k, v)| match get_opt(y, k) {
                Some(w) => loose_eq(v, w),
                None => false,
            })
        }
        _ => false,
    }
}

/// `$a == $b` for a string `$a` (as `loose_eq(&Value::String(a), b)`, without the copy)
pub fn loose_eq_str(a: &str, b: &Value) -> bool {
    use Value::*;
    match b {
        Null => a.is_empty(),
        Bool(y) => !(a.is_empty() || a == "0") == *y,
        Number(_) => {
            if is_numeric_str(a) {
                num_of(b) == Some(str_to_f64(a))
            } else {
                strval(b) == a
            }
        }
        String(y) => strs_loose_eq(a, y),
        _ => false,
    }
}

/// `$a == $b` for two strings
pub fn strs_loose_eq(a: &str, b: &str) -> bool {
    a == b || (is_numeric_str(a) && is_numeric_str(b) && str_to_f64(a) == str_to_f64(b))
}

/// `in_array($a, $haystack)` for a string `$a`
pub fn in_array_str(a: &str, haystack: &Value) -> bool {
    match haystack {
        Value::Array(x) => x.iter().any(|v| loose_eq_str(a, v)),
        Value::Object(o) => o.values().any(|v| loose_eq_str(a, v)),
        _ => false,
    }
}

/// `$a != $b`
pub fn loose_ne(a: &Value, b: &Value) -> bool {
    !loose_eq(a, b)
}

/// PHP 8 `<=>` for scalars (numbers, numeric strings, strings, null, bool)
pub fn loose_cmp(a: &Value, b: &Value) -> Ordering {
    use Value::*;
    match (a, b) {
        (Bool(_), _) | (_, Bool(_)) | (Null, _) | (_, Null) => {
            if let (String(s), Null) | (Null, String(s)) = (a, b) {
                let (x, y) = if a.is_null() { ("", s.as_str()) } else { (s.as_str(), "") };
                return x.cmp(y);
            }
            truthy(a).cmp(&truthy(b))
        }
        (Number(_), Number(_)) => num_of(a).unwrap_or(0.0).partial_cmp(&num_of(b).unwrap_or(0.0)).unwrap_or(Ordering::Equal),
        (Number(_), String(s)) => {
            if is_numeric_str(s) {
                num_of(a).unwrap_or(0.0).partial_cmp(&str_to_f64(s)).unwrap_or(Ordering::Equal)
            } else {
                strval(a).as_str().cmp(s.as_str())
            }
        }
        (String(s), Number(_)) => {
            if is_numeric_str(s) {
                str_to_f64(s).partial_cmp(&num_of(b).unwrap_or(0.0)).unwrap_or(Ordering::Equal)
            } else {
                s.as_str().cmp(strval(b).as_str())
            }
        }
        (String(x), String(y)) => {
            if is_numeric_str(x) && is_numeric_str(y) {
                str_to_f64(x).partial_cmp(&str_to_f64(y)).unwrap_or(Ordering::Equal)
            } else {
                x.cmp(y)
            }
        }
        (x, y) if is_array(x) && is_array(y) => count(x).cmp(&count(y)),
        (x, _) if is_array(x) => Ordering::Greater,
        _ => Ordering::Less,
    }
}

pub fn lt(a: &Value, b: &Value) -> bool {
    loose_cmp(a, b) == Ordering::Less
}
pub fn gt(a: &Value, b: &Value) -> bool {
    loose_cmp(a, b) == Ordering::Greater
}
pub fn le(a: &Value, b: &Value) -> bool {
    loose_cmp(a, b) != Ordering::Greater
}
pub fn ge(a: &Value, b: &Value) -> bool {
    loose_cmp(a, b) != Ordering::Less
}

/// `$a === $b`
pub fn strict_eq(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Number(x), Number(y)) => {
            // int and float are different types in PHP
            (x.is_f64() == y.is_f64()) && x.as_f64() == y.as_f64()
        }
        (x, y) if is_array(x) && is_array(y) => {
            let ex = entries(x);
            let ey = entries(y);
            ex.len() == ey.len() && ex.iter().zip(ey.iter()).all(|((k1, v1), (k2, v2))| k1 == k2 && strict_eq(v1, v2))
        }
        _ => a == b,
    }
}

// ---------------------------------------------------------------------------
// Arrays
// ---------------------------------------------------------------------------

/// The canonical PHP key of a string (integer-like strings are integer keys)
pub fn int_key(k: &str) -> Option<i64> {
    if k.is_empty() || k.len() > 19 {
        return None;
    }
    let b = k.as_bytes();
    let digits = if b[0] == b'-' { &k[1..] } else { k };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    if digits == "0" && b[0] == b'-' {
        return None;
    }
    k.parse::<i64>().ok()
}

/// The key a value takes when used as an array key
pub fn key_of(v: &Value) -> String {
    match v {
        Value::Bool(b) => (*b as i64).to_string(),
        Value::Null => String::new(),
        Value::Number(n) => n.as_i64().map(|i| i.to_string()).unwrap_or_else(|| (n.as_f64().unwrap_or(0.0).trunc() as i64).to_string()),
        Value::String(s) => s.clone(),
        _ => strval(v),
    }
}

/// The value form of a key: integer keys as numbers, as `array_keys` returns them
pub fn key_value(k: &str) -> Value {
    match int_key(k) {
        Some(i) => Value::from(i),
        None => Value::String(k.to_string()),
    }
}

/// All (key, value) pairs in order
pub fn entries(v: &Value) -> Vec<(String, &Value)> {
    match v {
        Value::Array(a) => a.iter().enumerate().map(|(i, x)| (i.to_string(), x)).collect(),
        Value::Object(o) => o.iter().map(|(k, x)| (k.clone(), x)).collect(),
        _ => vec![],
    }
}

/// Values in order
pub fn vals(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => o.values().collect(),
        _ => vec![],
    }
}

/// `$a[$k]` if set (null values count as set here; use `isset` for PHP's isset)
pub fn get_opt<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
        Value::Array(a) => int_key(k).and_then(|i| if i >= 0 { a.get(i as usize) } else { None }),
        Value::Object(o) => o.get(k),
        _ => None,
    }
}

/// `$a[$k] ?? null`
pub fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    get_opt(v, k).unwrap_or(&NULL)
}

/// `$a[$i] ?? null` with an integer key
pub fn geti(v: &Value, i: i64) -> &Value {
    get(v, &i.to_string())
}

/// `isset($a[$k])`
pub fn isset(v: &Value, k: &str) -> bool {
    get_opt(v, k).is_some_and(|x| !x.is_null())
}

/// `array_key_exists($k, $a)`
pub fn key_exists(v: &Value, k: &str) -> bool {
    get_opt(v, k).is_some()
}

/// Mutable access to `$a[$k]`, creating it (as null) if missing; `$a` becomes an array if null
pub fn get_mut<'a>(v: &'a mut Value, k: &str) -> &'a mut Value {
    if !key_exists(v, k) {
        set(v, k, Value::Null);
    }
    match v {
        Value::Array(a) => &mut a[int_key(k).unwrap() as usize],
        Value::Object(o) => o.get_mut(k).unwrap(),
        _ => unreachable!(),
    }
}

fn to_object(v: &mut Value) {
    if let Value::Array(a) = v {
        let mut m = Map::new();
        for (i, x) in std::mem::take(a).into_iter().enumerate() {
            m.insert(i.to_string(), x);
        }
        *v = Value::Object(m);
    }
}

/// `$a[$k] = $x` (a null or scalar `$a` becomes an array, as PHP does for null)
pub fn set(v: &mut Value, k: &str, x: Value) {
    if !is_array(v) {
        *v = Value::Array(vec![]);
    }
    if let Value::Array(a) = v {
        if let Some(i) = int_key(k) {
            if i >= 0 && (i as usize) < a.len() {
                a[i as usize] = x;
                return;
            }
            if i >= 0 && i as usize == a.len() {
                a.push(x);
                return;
            }
        }
        to_object(v);
    }
    if let Value::Object(o) = v {
        o.insert(k.to_string(), x);
    }
}

/// `$a[$i] = $x`
pub fn seti(v: &mut Value, i: i64, x: Value) {
    set(v, &i.to_string(), x)
}

/// `$a[] = $x`
pub fn push(v: &mut Value, x: Value) {
    if !is_array(v) {
        *v = Value::Array(vec![]);
    }
    match v {
        Value::Array(a) => a.push(x),
        Value::Object(o) => {
            let next = o.keys().filter_map(|k| int_key(k)).max().map(|m| m + 1).unwrap_or(0).max(0);
            o.insert(next.to_string(), x);
        }
        _ => unreachable!(),
    }
}

/// `unset($a[$k])`
pub fn unset(v: &mut Value, k: &str) {
    match v {
        Value::Array(a) => {
            if let Some(i) = int_key(k) {
                if i >= 0 && (i as usize) < a.len() {
                    if i as usize == a.len() - 1 {
                        a.pop();
                    } else {
                        to_object(v);
                        if let Value::Object(o) = v {
                            o.shift_remove(k);
                        }
                    }
                }
            }
        }
        Value::Object(o) => {
            o.shift_remove(k);
        }
        _ => {}
    }
}

/// `count($a)` (0 for non arrays: callers that must throw use `count_strict`)
pub fn count(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        Value::Object(o) => o.len(),
        _ => 0,
    }
}

/// `count($x)` that throws a TypeError on non arrays, as PHP 8
pub fn count_strict(v: &Value) -> Result<usize, crate::PhpError> {
    if is_array(v) {
        Ok(count(v))
    } else {
        Err(crate::PhpError::new(format!("TypeError: count(): Argument #1 must be of type Countable|array, {} given", type_name(v))))
    }
}

pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "string",
        _ => "array",
    }
}

/// `array_keys($a)`
pub fn keys(v: &Value) -> Vec<Value> {
    entries(v).into_iter().map(|(k, _)| key_value(&k)).collect()
}

/// `array_keys($a)` as strings
pub fn keys_str(v: &Value) -> Vec<String> {
    entries(v).into_iter().map(|(k, _)| k).collect()
}

/// `array_values($a)`
pub fn values(v: &Value) -> Value {
    Value::Array(vals(v).into_iter().cloned().collect())
}

/// `in_array($needle, $haystack)` (loose)
pub fn in_array(needle: &Value, haystack: &Value) -> bool {
    match haystack {
        Value::Array(a) => a.iter().any(|x| loose_eq(needle, x)),
        Value::Object(o) => o.values().any(|x| loose_eq(needle, x)),
        _ => false,
    }
}

/// `in_array($needle, $haystack, true)`
pub fn in_array_strict(needle: &Value, haystack: &Value) -> bool {
    vals(haystack).into_iter().any(|x| strict_eq(needle, x))
}

/// `in_array` on a list of strings
pub fn in_list(needle: &Value, list: &[&str]) -> bool {
    list.iter().any(|s| loose_eq_str(s, needle))
}

/// `array_merge($a, $b, ...)`: integer keys renumbered, string keys overwritten
pub fn array_merge(arrays: &[&Value]) -> Value {
    let mut out = Value::Array(vec![]);
    for a in arrays {
        for (k, x) in entries(a) {
            if int_key(&k).is_some() {
                push(&mut out, x.clone());
            } else {
                set(&mut out, &k, x.clone());
            }
        }
    }
    out
}

/// `$a + $b`
pub fn union(a: &Value, b: &Value) -> Value {
    let mut out = if is_array(a) { a.clone() } else { Value::Array(vec![]) };
    for (k, x) in entries(b) {
        if !key_exists(&out, &k) {
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `array_diff($a, $b)` (string comparison, keys kept)
pub fn array_diff(a: &Value, b: &Value) -> Value {
    let bs: Vec<String> = vals(b).into_iter().map(strval).collect();
    let mut out = Value::Array(vec![]);
    for (k, x) in entries(a) {
        if !bs.contains(&strval(x)) {
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `array_intersect($a, $b)` (string comparison, keys of $a kept)
pub fn array_intersect(a: &Value, b: &Value) -> Value {
    let bs: Vec<String> = vals(b).into_iter().map(strval).collect();
    let mut out = Value::Array(vec![]);
    for (k, x) in entries(a) {
        if bs.contains(&strval(x)) {
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `array_unique($a)` (string comparison, first keys kept)
pub fn array_unique(a: &Value) -> Value {
    let mut seen: Vec<String> = vec![];
    let mut out = Value::Array(vec![]);
    for (k, x) in entries(a) {
        let s = strval(x);
        if !seen.contains(&s) {
            seen.push(s);
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `array_slice($a, $offset, $length)` (integer keys renumbered)
pub fn array_slice(a: &Value, offset: usize, length: Option<usize>) -> Value {
    let e = entries(a);
    let end = length.map(|l| (offset + l).min(e.len())).unwrap_or(e.len());
    let mut out = Value::Array(vec![]);
    for (k, x) in e.into_iter().take(end).skip(offset) {
        if int_key(&k).is_some() {
            push(&mut out, x.clone());
        } else {
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `array_reverse($a)` (integer keys renumbered)
pub fn array_reverse(a: &Value) -> Value {
    let mut e = entries(a);
    e.reverse();
    let mut out = Value::Array(vec![]);
    for (k, x) in e {
        if int_key(&k).is_some() {
            push(&mut out, x.clone());
        } else {
            set(&mut out, &k, x.clone());
        }
    }
    out
}

/// `explode($sep, $s)`
pub fn explode(sep: &str, s: &str) -> Vec<String> {
    s.split(sep).map(String::from).collect()
}

/// `implode($sep, $a)`
pub fn implode(sep: &str, a: &Value) -> String {
    vals(a).into_iter().map(strval).collect::<Vec<_>>().join(sep)
}

/// A list Value of strings
pub fn str_list(items: &[&str]) -> Value {
    Value::Array(items.iter().map(|s| Value::String(s.to_string())).collect())
}

/// A list Value of ints
pub fn int_list(items: &[i64]) -> Value {
    Value::Array(items.iter().map(|i| Value::from(*i)).collect())
}

/// `json_decode(json_encode($v), true)`: maps with keys 0..n-1 in order become lists, empty maps
/// become empty lists, recursively
pub fn normalize(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(normalize),
        Value::Object(o) => {
            o.values_mut().for_each(normalize);
            let sequential = o.keys().enumerate().all(|(i, k)| is_index_key(k, i));
            if sequential {
                let m = std::mem::take(o);
                *v = Value::Array(m.into_iter().map(|(_, x)| x).collect());
            }
        }
        _ => {}
    }
}

/// `normalize` would leave the value as it is
pub fn is_normalized(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().all(is_normalized),
        Value::Object(o) => !o.keys().enumerate().all(|(i, k)| is_index_key(k, i)) && o.values().all(is_normalized),
        _ => true,
    }
}

/// `k == (string)$i`, without allocating
pub fn is_index_key(k: &str, i: usize) -> bool {
    let mut buf = [0u8; 20];
    let mut n = i;
    let mut p = buf.len();
    loop {
        p -= 1;
        buf[p] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    k.as_bytes() == &buf[p..]
}

/// `json_decode(json_encode($v), true)` of a value
pub fn round_trip(v: &Value) -> Value {
    let mut x = v.clone();
    normalize(&mut x);
    x
}

/// `$x ?? $default`
pub fn coalesce<'a>(x: &'a Value, default: &'a Value) -> &'a Value {
    if x.is_null() {
        default
    } else {
        x
    }
}

/// An empty PHP array
pub fn arr() -> Value {
    Value::Array(vec![])
}

/// A PHP map from (key, value) pairs
pub fn map(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    Value::Object(m)
}

/// `ucfirst`
pub fn ucfirst(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// `max()` of two loosely compared values
pub fn max_v(a: Value, b: Value) -> Value {
    if gt(&b, &a) {
        b
    } else {
        a
    }
}

/// `strpos($haystack, $needle) !== false`
pub fn contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

/// PHP SQL LIKE as in Collection::like_match (`%` wildcard, case-insensitive, whole string)
pub fn like_match(pattern: &str, subject: &str) -> bool {
    if pattern.is_ascii() && subject.is_ascii() {
        // ASCII: lowercase is ASCII lowercase, compared byte by byte without copies
        fn rec_ascii(p: &[u8], s: &[u8]) -> bool {
            match p.first() {
                None => s.is_empty(),
                Some(b'%') => (0..=s.len()).any(|i| rec_ascii(&p[1..], &s[i..])),
                Some(c) => !s.is_empty() && s[0].eq_ignore_ascii_case(c) && rec_ascii(&p[1..], &s[1..]),
            }
        }
        return rec_ascii(pattern.as_bytes(), subject.as_bytes());
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let s: Vec<char> = subject.to_lowercase().chars().collect();
    fn rec(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('%') => (0..=s.len()).any(|i| rec(&p[1..], &s[i..])),
            Some(c) => !s.is_empty() && s[0] == *c && rec(&p[1..], &s[1..]),
        }
    }
    rec(&p, &s)
}
