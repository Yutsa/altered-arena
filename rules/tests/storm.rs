//! Expeditions, Dusk and Night: Collection::sortBy order, storm computations, the storm actions.

use altered_rules::helpers::collection;
use serde_json::Value;

/// `Collection::sortBy` (uasort with a bool comparator) gives PHP 8.2's order: reference cases
/// produced by `tests/data/sortby_php82.php` in the server container
#[test]
fn sort_by_matches_php82() {
    let data: Value = serde_json::from_str(include_str!("data/sortby_php82.json")).unwrap();
    for case in data.as_array().unwrap() {
        let keys: Vec<String> = case["keys"].as_array().unwrap().iter().map(|k| k.as_str().unwrap().to_string()).collect();
        let ids: Vec<i64> = (0..keys.len() as i64).map(|i| 100 + i).collect();
        let expected: Vec<i64> = case["order"].as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
        assert_eq!(collection::sort_by(&ids, &keys), expected, "keys {:?}", keys);
    }
}
