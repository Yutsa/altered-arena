//! Core/Globals.php: typed game variables stored as JSON in `global_variables`

use crate::game::Game;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};
use crate::game::GlobalSlot;
use std::sync::Arc;

/// Core/Globals.php:13-98 `$variables`
pub const VARIABLES: &[(&str, &str)] = &[
    ("engine", "obj"),
    ("engineChoices", "int"),
    ("engineAutomatic", "int"),
    ("callbackEngineResolved", "obj"),
    ("anytimeRecursion", "int"),
    ("customTurnOrders", "obj"),
    ("playerDecks", "obj"),
    ("deckOptions", "str"),
    ("deckContent", "obj"),
    ("tokenStyles", "obj"),
    ("undo", "bool"),
    ("statMapping", "obj"),
    ("deckFormat", "str"),
    ("firstPlayer", "int"),
    ("skippedPlayers", "obj"),
    ("dayPhase", "bool"),
    ("activePId", "int"),
    ("zombie", "bool"),
    ("storm", "obj"),
    ("day", "int"),
    ("phase", "int"),
    ("heroes", "obj"),
    ("playedCards", "int"),
    ("playedCardsHistory", "obj"),
    ("stormMoves", "obj"),
    ("expeditionMoves", "obj"),
    ("blockedExpeditions", "obj"),
    ("tieBreakerMode", "bool"),
    ("enterTieBreakerMode", "bool"),
    ("instantWin", "bool"),
    ("diceRolls", "obj"),
    ("firstDayManaSelection", "obj"),
    ("nightSelection", "obj"),
    ("deckSelection", "obj"),
    ("costReduction", "obj"),
    ("nextCharacterBoost", "int"),
    ("nextCharacterBoostOccurence", "int"),
    ("nextReserveCharacterBoost", "int"),
    ("nextCharacterCost3Anchored", "bool"),
    ("nextCharacterAnchored", "bool"),
    ("nextCharacterFleeting", "bool"),
    ("additionalEffect", "obj"),
    ("afterRest", "obj"),
    ("afterNightCleanup", "obj"),
    ("nextSpellIsFree", "bool"),
    ("removeFleetingIfSpellPlayedHand", "bool"),
    ("removeFleetingSpellPlayed", "bool"),
    ("removeFleetingCharacterPlayed", "bool"),
    ("playedForFree", "bool"),
    ("nextTokenAnchored", "bool"),
    ("removeFleetingCharacterStat0Played", "bool"),
    ("removeFleetingSongArtistPlayed", "bool"),
    ("nextCharacterBoostV", "int"),
    ("globalTough", "obj"),
    ("nextTokenAsleep", "bool"),
    ("turnCards", "obj"),
    ("nextCharacterInExpeditionBoost", "obj"),
    ("nextCharacterBaseCost3Anchored", "bool"),
    ("nextCharacterAsleep", "obj"),
    ("abilityActivatedThisTurn", "obj"),
    ("nextAnimalBoost", "int"),
    ("nextAnimalBoostOccurence", "int"),
    ("abilityActivatedThisTurnCount", "obj"),
    ("abilityActivatedThisTurnTypeCount", "obj"),
    ("smokeThemOutArmed", "obj"),
    ("samSpookEverDiscarded", "obj"),
    ("newDayManaSelection", "obj"),
    ("testingOption", "bool"),
    ("beginner", "int"),
    ("firstPass", "int"),
];

/// Index in VARIABLES and type of each variable
fn index() -> &'static std::collections::HashMap<&'static str, (usize, &'static str)> {
    static INDEX: std::sync::OnceLock<std::collections::HashMap<&'static str, (usize, &'static str)>> = std::sync::OnceLock::new();
    INDEX.get_or_init(|| VARIABLES.iter().enumerate().map(|(i, (n, t))| (*n, (i, *t))).collect())
}

pub fn var_type(name: &str) -> Option<&'static str> {
    index().get(name).map(|(_, t)| *t)
}

/// Bit of a variable in `Game::globals_clean`
fn clean_bit(name: &str) -> u128 {
    index().get(name).map(|(i, _)| 1u128 << i).unwrap_or(0)
}

pub fn default_of(t: &str) -> Value {
    match t {
        "int" => json!(0),
        "bool" => json!(false),
        "str" => json!(""),
        _ => json!([]),
    }
}


/// `Globals::fetch()` (Core/Globals.php:110-126): every row read back from its JSON.
/// The round trip is idempotent: values read back since their last change (bits of
/// `Game::globals_clean`) are already what it gives.
// PHP: Core/Globals.php:110
pub fn fetch(g: &mut Game) {
    let mut clean = g.globals_clean;
    let mut changes = vec![];
    for (pos, (n, v)) in g.globals.iter().enumerate() {
        let Some((i, t)) = index().get(n.as_str()) else { continue };
        if clean & (1u128 << i) != 0 {
            continue;
        }
        clean |= 1u128 << i;
        if *t == "int" {
            let n = json!(php::intval(&php::round_trip(v)));
            if n != **v {
                changes.push((pos, GlobalSlot::new(n)));
            }
        } else if v.has_db() || !php::is_normalized(v) || has_backslash(v) {
            // The row (what the last set stored, as JSON), read by cast(): backslashes doubled
            let db = if v.has_db() { v.db_value().clone() } else { php::round_trip(v) };
            let mut cache = db.clone();
            double_backslashes(&mut cache);
            changes.push((pos, GlobalSlot::read_back(cache, db)));
        }
    }
    g.globals_clean = clean;
    if !changes.is_empty() {
        let m = Arc::make_mut(&mut g.globals);
        for (pos, slot) in changes {
            if let Some((_, s)) = m.get_index_mut(pos) {
                *s = slot;
            }
        }
    }
}

/// `str_replace('\\\\', '\\\\\\\\', $json)` before `json_decode` (Core/Globals.php:104): every
/// backslash of a string or key (`\\` in the JSON text) becomes two. True when one did.
fn has_backslash(v: &Value) -> bool {
    match v {
        Value::String(s) => s.contains('\\'),
        Value::Array(a) => a.iter().any(has_backslash),
        Value::Object(o) => o.iter().any(|(k, x)| k.contains('\\') || has_backslash(x)),
        _ => false,
    }
}

fn double_backslashes(v: &mut Value) -> bool {
    fn dbl(s: &str) -> Option<String> {
        s.contains('\\').then(|| s.replace('\\', "\\\\"))
    }
    match v {
        Value::String(s) => match dbl(s) {
            Some(d) => {
                *s = d;
                true
            }
            None => false,
        },
        Value::Array(a) => a.iter_mut().fold(false, |acc, x| double_backslashes(x) | acc),
        Value::Object(o) => {
            let mut changed = false;
            if o.keys().any(|k| k.contains('\\')) {
                let old = std::mem::take(o);
                for (k, x) in old {
                    o.insert(dbl(&k).unwrap_or(k), x);
                }
                changed = true;
            }
            o.values_mut().fold(changed, |acc, x| double_backslashes(x) | acc)
        }
        _ => false,
    }
}

/// `Globals::create($name)` (Core/Globals.php:132-150)
fn create(g: &mut Game, name: &str) {
    if let Some(t) = var_type(name) {
        Arc::make_mut(&mut g.globals).insert(name.to_string(), GlobalSlot::new(default_of(t)));
    }
}

fn check(g: &mut Game, name: &str) -> R<&'static str> {
    let t = var_type(name).ok_or_else(|| PhpError::new(format!("InvalidArgumentException: Property {} doesn't exist", name)))?;
    if !g.globals.contains_key(name) {
        create(g, name);
    }
    Ok(t)
}

/// `Globals::get<Name>()`
// PHP: Core/Globals.php:169
pub fn get(g: &mut Game, name: &str) -> R<Value> {
    check(g, name)?;
    Ok((*g.globals[name]).clone())
}

/// `Globals::is<Name>()` (bool variables only)
pub fn is(g: &mut Game, name: &str) -> R<bool> {
    let t = check(g, name)?;
    if t != "bool" {
        return Err(PhpError::new(format!("InvalidArgumentException: Property {} is not of type bool", name)));
    }
    Ok(php::truthy(&g.globals[name]))
}

/// `Globals::set<Name>($v)`: ints and bools are cast
pub fn set(g: &mut Game, name: &str, v: Value) -> R<Value> {
    let t = check(g, name)?;
    let v = match t {
        "int" => json!(php::intval(&v)),
        "bool" => json!(php::truthy(&v)),
        _ => v,
    };
    Arc::make_mut(&mut g.globals).insert(name.to_string(), GlobalSlot::new(v.clone()));
    g.globals_clean &= !clean_bit(name);
    Ok(v)
}

/// A variable written outside `set` (its value must be read back at the next fetch)
pub fn touched(g: &mut Game, name: &str) {
    g.globals_clean &= !clean_bit(name);
}

/// `Globals::inc<Name>($n = 1)` (int variables only)
pub fn inc(g: &mut Game, name: &str, n: i64) -> R<i64> {
    let t = check(g, name)?;
    if t != "int" {
        return Err(PhpError::new(format!("InvalidArgumentException: Trying to increase {} which is not an int", name)));
    }
    let v = php::intval(&g.globals[name]) + n;
    set(g, name, json!(v))?;
    Ok(v)
}

/// `Globals::get<Name>()` of an int variable
pub fn get_int(g: &mut Game, name: &str) -> R<i64> {
    Ok(php::intval(&get(g, name)?))
}

/// `Globals::getStorm($ui)` (Core/Globals.php:274-290): reads `$data` directly
pub fn get_storm(g: &mut Game, ui: bool) -> Value {
    let mut storm = g.globals.get("storm").map(|v| (**v).clone()).unwrap_or(Value::Null);
    if ui {
        if let Value::Array(a) = &mut storm {
            for c in a.iter_mut() {
                if !php::truthy(php::get(c, "visible")) {
                    php::set(c, "cardId", json!(6));
                    php::unset(c, "rotated");
                }
                php::unset(c, "visible");
            }
        }
    }
    storm
}

/// `Globals::getVisibleRegions()` (Core/Globals.php:292-322)
pub fn get_visible_regions(g: &mut Game) -> R<Value> {
    if is(g, "tieBreakerMode")? {
        return Ok(json!({"4": ["forest", "mountain", "ocean"]}));
    }
    let storm_cards = get_storm(g, false);
    let mut storms = php::arr();
    let mut index = 0i64;
    for (_, sc) in php::entries(&storm_cards) {
        let mut storm = storm_card(php::intval(php::get(sc, "cardId")));
        if !php::truthy(php::get(sc, "visible")) {
            index += 2;
            continue;
        }
        if php::truthy(php::get(sc, "rotated")) {
            storm.reverse();
        }
        for biomes in storm {
            if biomes.is_empty() {
                continue;
            }
            php::seti(&mut storms, index, php::str_list(&biomes));
            index += 1;
        }
    }
    Ok(storms)
}

/// constants.inc.php STORM_CARDS
pub fn storm_card(i: i64) -> Vec<Vec<&'static str>> {
    match i {
        0 => vec![vec![], vec!["forest", "mountain", "ocean"]],
        1 => vec![vec!["forest"], vec!["mountain", "ocean"]],
        2 => vec![vec!["mountain"], vec!["forest", "ocean"]],
        3 => vec![vec!["ocean"], vec!["mountain", "forest"]],
        _ => vec![vec!["forest", "mountain", "ocean"], vec![]],
    }
}

/// `Globals::setupNewGame($players, $options)` (Core/Globals.php:226-272)
pub fn setup_new_game(g: &mut Game, options: &Value) -> R<()> {
    let mut storm = vec![json!({"cardId": 0, "visible": true, "rotated": false})];
    let mut keys = vec![1i64, 2, 3];
    g.php_shuffle(&mut keys);
    for _ in 0..3 {
        let key = keys.remove(0);
        let rotated = g.bga_rand(0, 1) == 1;
        storm.push(json!({"cardId": key, "visible": false, "rotated": rotated}));
    }
    storm.push(json!({"cardId": 4, "visible": true, "rotated": false}));
    set(g, "storm", Value::Array(storm))?;
    set(g, "day", json!(0))?;
    set(g, "playedCards", json!(0))?;
    set(g, "dayPhase", json!(false))?;
    let df = php::intval(php::get(options, "155"));
    let fmt = match df {
        0 => Some("FRONTIER"),
        1 => Some("STANDARD"),
        2 => Some("SINGLETON_NUC"),
        3 => Some("SANDBOX"),
        4 => Some("NO_UNIQUE"),
        6 => Some("TEST"),
        5 => Some("DEMO"),
        7 => Some("SEALED"),
        _ => None,
    };
    if let Some(f) = fmt {
        set(g, "deckFormat", json!(f))?;
    }
    set(g, "deckOptions", json!(1))?;
    let beginner = php::coalesce(php::get(options, "150"), &json!(1)).clone();
    set(g, "beginner", beginner)?;
    let undo = php::coalesce(php::get(options, "151"), &json!(0)).clone();
    set(g, "undo", undo)?;
    Ok(())
}
