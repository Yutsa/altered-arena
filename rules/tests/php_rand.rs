//! PHP's Mersenne Twister, shuffle and array_rand against PHP 8.2: the reference values are
//! produced by `tests/data/php_rand_php.php` in the server container.

use altered_rules::php_rand::{PhpArrayShape, PhpMt};
use serde_json::Value;

fn data() -> Value {
    serde_json::from_str(include_str!("data/php_rand_php.json")).unwrap()
}

fn i64s(v: &Value) -> Vec<i64> {
    v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
}

fn seed(c: &Value) -> u32 {
    c["seed"].as_i64().unwrap() as u32
}

#[test]
fn mt_rand_sequences() {
    let d = data();
    assert_eq!(d["php"].as_str().map(|v| v.starts_with("8.2")), Some(true));
    for c in d["mt"].as_array().unwrap() {
        let mut mt = PhpMt::new(seed(c));
        let got: Vec<i64> = (0..c["values"].as_array().unwrap().len()).map(|_| mt.mt_rand()).collect();
        assert_eq!(got, i64s(&c["values"]), "mt_srand({})", c["seed"]);
    }
}

#[test]
fn mt_rand_ranges() {
    for c in data()["range"].as_array().unwrap() {
        let (min, max) = (c["min"].as_i64().unwrap(), c["max"].as_i64().unwrap());
        let mut mt = PhpMt::new(seed(c));
        let got: Vec<i64> = (0..c["values"].as_array().unwrap().len()).map(|_| mt.range(min, max)).collect();
        assert_eq!(got, i64s(&c["values"]), "mt_rand({}, {}) seed {}", min, max, c["seed"]);
    }
}

#[test]
fn shuffles() {
    for c in data()["shuffle"].as_array().unwrap() {
        let n = c["n"].as_i64().unwrap();
        let mut mt = PhpMt::new(seed(c));
        let mut a: Vec<i64> = (0..n).collect();
        mt.shuffle(&mut a);
        assert_eq!(a, i64s(&c["out"]), "shuffle of {} seed {}", n, c["seed"]);
        assert_eq!(mt.mt_rand(), c["next"].as_i64().unwrap(), "draws used by the shuffle of {}", n);
    }
}

#[test]
fn array_rands() {
    let mut packed_with_holes = 0;
    for c in data()["rand"].as_array().unwrap() {
        let keys = c["keys"].as_array().unwrap();
        let num = c["num"].as_u64().unwrap() as usize;
        let shape = match c["via"].as_str().unwrap() {
            "list" | "assoc" => PhpArrayShape::compact(keys.len()),
            _ => PhpArrayShape::from_int_keys(&i64s(&c["keys"])),
        };
        if shape.packed && shape.used > keys.len() as u64 {
            packed_with_holes += 1;
        }
        let mut mt = PhpMt::new(seed(c));
        // array_rand returns the keys in iteration order (= insertion order here)
        let got: Vec<Value> = mt.array_rand(&shape, num).into_iter().map(|i| keys[i].clone()).collect();
        assert_eq!(Value::Array(got), c["picked"], "array_rand({:?}, {}) via {} seed {}", keys, num, c["via"], c["seed"]);
        assert_eq!(mt.mt_rand(), c["next"].as_i64().unwrap(), "draws used by array_rand({:?}, {}) via {}", keys, num, c["via"]);
    }
    assert!(packed_with_holes > 50, "the cases cover packed arrays with holes");
}
