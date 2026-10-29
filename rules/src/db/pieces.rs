//! Helpers/CachedPieces.php: pieces (cards, meeples) cached in memory in table order, filtered
//! in memory, written through the model setters. A collection is the ordered list of ids it
//! holds (the PHP Collection is keyed by id).

use crate::game::Game;
use crate::php;
use crate::{PhpError, R};
use serde_json::Value;

/// The two piece tables
pub trait Pieces {
    /// Ids in cache order (`static::$datas`)
    fn all_ids(g: &Game) -> Vec<i64>;
    fn has(g: &Game, id: i64) -> bool;
    fn location(g: &Game, id: i64) -> String {
        Self::location_ref(g, id).to_string()
    }
    fn location_ref(g: &Game, id: i64) -> &str;
    /// `getState()` as PHP has it (int for cards, string for meeples)
    fn state_value(g: &Game, id: i64) -> Value;
    fn set_location(g: &mut Game, id: i64, loc: &str) -> R<()>;
    fn set_state(g: &mut Game, id: i64, state: i64) -> R<()>;
    /// `getPId()` as PHP has it
    fn pid_value(g: &Game, id: i64) -> Value;
    /// Removes the rows (`unset(static::$datas[$id])`)
    fn remove(g: &mut Game, id: i64);
    /// `$autoreshuffleCustom`
    fn reshuffle_custom() -> &'static [(&'static str, &'static str)] {
        &[]
    }
}

/// `checkLocation($location, $like)`
// PHP: Helpers/CachedPieces.php:113
pub fn check_location(location: &str, like: bool) -> R<()> {
    let ok = !location.is_empty()
        && location.chars().enumerate().all(|(i, c)| c.is_ascii_alphanumeric() || c == '-' || (like && c == '%') || (i > 0 && c == '_'));
    if !ok {
        return Err(PhpError::new(format!("Class Pieces: location must be alphanum and underscore non empty string '{}'", location)));
    }
    Ok(())
}

/// `where('location', $location)` with LIKE when the value contains `%`
pub fn location_matches(loc: &str, filter: &str) -> bool {
    if filter.contains('%') {
        php::like_match(filter, loc)
    } else {
        php::strs_loose_eq(loc, filter)
    }
}

/// `getSelectWhere(null, $location, $state)` / `getInLocation($location, $state)`
// PHP: Helpers/CachedPieces.php:187
pub fn get_in_location<T: Pieces>(g: &Game, location: &str, state: Option<i64>) -> R<Vec<i64>> {
    check_location(location, true)?;
    Ok(T::all_ids(g)
        .into_iter()
        .filter(|id| location_matches(T::location_ref(g, *id), location))
        .filter(|id| state.is_none_or(|s| php::loose_eq(&T::state_value(g, *id), &Value::from(s))))
        .collect())
}

/// `getInLocation` for several locations (`where('location', [...])` = in_array)
pub fn get_in_locations<T: Pieces>(g: &Game, locations: &[&str]) -> Vec<i64> {
    T::all_ids(g)
        .into_iter()
        .filter(|id| {
            let l = T::location_ref(g, *id);
            locations.iter().any(|x| php::strs_loose_eq(l, x))
        })
        .collect()
}

/// `Collection::orderBy('state', $asc)`: stable sort on `$a - $b`
pub fn order_by_state<T: Pieces>(g: &Game, ids: &mut [i64], asc: bool) {
    ids.sort_by(|a, b| {
        let x = php::floatval(&T::state_value(g, *a));
        let y = php::floatval(&T::state_value(g, *b));
        let o = x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
        if asc {
            o
        } else {
            o.reverse()
        }
    });
}

/// `getExtremePosition($getMax, $location)`
pub fn get_extreme_position<T: Pieces>(g: &Game, get_max: bool, location: &str) -> R<i64> {
    let ids = get_in_location::<T>(g, location, None)?;
    let states: Vec<i64> = ids.iter().map(|id| php::intval(&T::state_value(g, *id))).collect();
    if states.is_empty() {
        return Ok(0);
    }
    Ok(if get_max { *states.iter().max().unwrap() } else { *states.iter().min().unwrap() })
}

/// `getTopOf($location, $n)`
// PHP: Helpers/CachedPieces.php:256
pub fn get_top_of<T: Pieces>(g: &Game, location: &str, n: usize) -> R<Vec<i64>> {
    check_location(location, false)?;
    let mut ids = get_in_location::<T>(g, location, None)?;
    order_by_state::<T>(g, &mut ids, false);
    ids.truncate(n);
    Ok(ids)
}

/// `getMany($ids, $raise)`: de-duplicated, in the requested order
// PHP: Helpers/CachedPieces.php:207
pub fn get_many<T: Pieces>(g: &Game, ids: &[Value], raise: bool) -> R<Vec<i64>> {
    let mut seen: Vec<String> = vec![];
    for id in ids {
        let s = php::strval(id);
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    let mut out = vec![];
    for s in &seen {
        if let Some(i) = php::int_key(s) {
            if T::has(g, i) && !out.contains(&i) {
                out.push(i);
            }
        }
    }
    if out.len() != seen.len() && raise {
        return Err(PhpError::new(format!("Class Pieces: getMany, some pieces have not been found ! asked={:?}", seen)));
    }
    Ok(out)
}

/// `move($ids, $location, $state)`
// PHP: Helpers/CachedPieces.php:316
pub fn move_pieces<T: Pieces>(g: &mut Game, ids: &[Value], location: &str, state: i64) -> R<Vec<i64>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    check_location(location, false)?;
    let ids = get_many::<T>(g, ids, true)?;
    for id in &ids {
        T::set_location(g, *id, location)?;
    }
    for id in &ids {
        T::set_state(g, *id, state)?;
    }
    Ok(ids)
}

/// `moveAllInLocation($from, $to, $fromState, $toState)` (filtered by `pId` when `$to` is
/// `<x>-<pId>`)
// PHP: Helpers/CachedPieces.php:338
pub fn move_all_in_location<T: Pieces>(g: &mut Game, from: &str, to: &str, from_state: Option<i64>, to_state: Option<i64>) -> R<Vec<i64>> {
    check_location(from, false)?;
    check_location(to, false)?;
    let mut ids = get_in_location::<T>(g, from, from_state)?;
    let parts: Vec<&str> = to.split('-').collect();
    if parts.len() > 1 {
        let pid = Value::String(parts[1].to_string());
        ids.retain(|id| php::loose_eq(&T::pid_value(g, *id), &pid));
    }
    for id in &ids {
        T::set_location(g, *id, to)?;
    }
    for id in &ids {
        // update('state', null) writes 0 through the int cast of the setter
        T::set_state(g, *id, to_state.unwrap_or(0))?;
    }
    Ok(ids)
}

/// `pickForLocation($nbr, $from, $to, $state, $deckReform)`
// PHP: Helpers/CachedPieces.php:367
pub fn pick_for_location<T: Pieces>(g: &mut Game, nbr: usize, from: &str, to: &str, state: i64, deck_reform: bool, reshuffle_listener: &dyn Fn(&mut Game, &str) -> R<()>) -> R<Vec<i64>> {
    check_location(from, false)?;
    check_location(to, false)?;
    let ids = get_top_of::<T>(g, from, nbr)?;
    let vals: Vec<Value> = ids.iter().map(|i| Value::from(*i)).collect();
    let picked = get_many::<T>(g, &vals, true)?;
    for id in &picked {
        T::set_location(g, *id, to)?;
    }
    for id in &picked {
        T::set_state(g, *id, state)?;
    }
    let generic = from.split('-').next().unwrap_or("");
    let custom = T::reshuffle_custom();
    if (custom.iter().any(|(k, _)| *k == from) || custom.iter().any(|(k, _)| *k == generic)) && picked.len() < nbr && deck_reform {
        let missing = nbr - picked.len();
        reform_deck_from_discard::<T>(g, from, reshuffle_listener)?;
        let more = pick_for_location::<T>(g, missing, from, to, state, false, reshuffle_listener)?;
        let mut out = picked;
        for m in more {
            if !out.contains(&m) {
                out.push(m);
            }
        }
        return Ok(out);
    }
    Ok(picked)
}

/// `reformDeckFromDiscard($fromLocation)`
// PHP: Helpers/CachedPieces.php:403
pub fn reform_deck_from_discard<T: Pieces>(g: &mut Game, from: &str, listener: &dyn Fn(&mut Game, &str) -> R<()>) -> R<()> {
    let generic = from.split('-').next().unwrap_or("");
    check_location(from, false)?;
    let Some((_, discard)) = T::reshuffle_custom().iter().find(|(k, _)| *k == generic) else {
        return Err(PhpError::new(format!("Class Pieces:reformDeckFromDiscard: Unknown discard location for {} !", generic)));
    };
    check_location(discard, false)?;
    move_all_in_location::<T>(g, discard, from, None, Some(0))?;
    shuffle::<T>(g, from)?;
    listener(g, from)
}

/// `shuffle($location)`: state = index in the shuffled list
// PHP: Helpers/CachedPieces.php:426
pub fn shuffle<T: Pieces>(g: &mut Game, location: &str) -> R<()> {
    check_location(location, false)?;
    let mut ids = get_in_location::<T>(g, location, None)?;
    g.php_shuffle(&mut ids);
    for (state, id) in ids.iter().enumerate() {
        T::set_state(g, *id, state as i64)?;
    }
    Ok(())
}

/// `insertOnTop($id, $location)`
pub fn insert_on_top<T: Pieces>(g: &mut Game, id: &Value, location: &str) -> R<()> {
    let pos = get_extreme_position::<T>(g, true, location)?;
    move_pieces::<T>(g, std::slice::from_ref(id), location, pos + 1)?;
    Ok(())
}

/// `insertAtBottom($id, $location)`
pub fn insert_at_bottom<T: Pieces>(g: &mut Game, id: &Value, location: &str) -> R<()> {
    let pos = get_extreme_position::<T>(g, false, location)?;
    move_pieces::<T>(g, std::slice::from_ref(id), location, pos - 1)?;
    Ok(())
}

/// `moveAtPosition($id, $location, $position)`: renumbers the location
// PHP: Helpers/CachedPieces.php:453
pub fn move_at_position<T: Pieces>(g: &mut Game, id: i64, location: &str, position: i64) -> R<()> {
    let mut pieces = get_in_location::<T>(g, location, None)?;
    order_by_state::<T>(g, &mut pieces, false);
    let mut ordered: Vec<i64> = pieces.into_iter().filter(|p| *p != id).collect();
    let position = position.min(ordered.len() as i64 + 1).max(1);
    ordered.insert((position - 1) as usize, id);
    let mut pos = ordered.len() as i64;
    for p in ordered {
        if !T::has(g, p) {
            return Err(PhpError::new("Error: Call to a member function setState() on null"));
        }
        T::set_state(g, p, pos)?;
        pos -= 1;
    }
    Ok(())
}

/// `delete($ids)`
// PHP: Helpers/CachedPieces.php:479
pub fn delete<T: Pieces>(g: &mut Game, ids: &[Value]) -> R<Vec<i64>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let pieces = get_many::<T>(g, ids, true)?;
    for id in ids {
        if let Some(i) = php::int_key(&php::strval(id)) {
            T::remove(g, i);
        }
    }
    Ok(pieces)
}
