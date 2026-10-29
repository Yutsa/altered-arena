//! Managers/Meeples.php: storm tokens, boosts and statuses on cards, terrain markers

use crate::db::pieces::{self, Pieces};
use crate::game::{Game, Meeple, PId};
use crate::php;
use crate::R;
use serde_json::{json, Value};

pub struct MeeplesT;

impl Pieces for MeeplesT {
    fn all_ids(g: &Game) -> Vec<i64> {
        g.meeples.keys().copied().collect()
    }
    fn has(g: &Game, id: i64) -> bool {
        g.meeples.contains_key(&id)
    }
    fn location(g: &Game, id: i64) -> String {
        g.meeples[&id].location.clone()
    }
    fn state_value(g: &Game, id: i64) -> Value {
        json!(g.meeples.get(&id).or_else(|| g.deleted_meeples.get(&id)).map(|m| m.state.clone()).unwrap_or_default())
    }
    fn set_location(g: &mut Game, id: i64, loc: &str) -> R<()> {
        g.meeple_mut(id)?.location = loc.to_string();
        Ok(())
    }
    fn set_state(g: &mut Game, id: i64, state: i64) -> R<()> {
        g.meeple_mut(id)?.state = state.to_string();
        Ok(())
    }
    fn pid_value(g: &Game, id: i64) -> Value {
        json!(g.meeples[&id].pid)
    }
    fn remove(g: &mut Game, id: i64) {
        if let Some(m) = g.meeples.shift_remove(&id) {
            g.deleted_meeples.insert(id, m);
        }
    }
}

/// `Meeples::getAll()`
pub fn get_all(g: &Game) -> Vec<i64> {
    MeeplesT::all_ids(g)
}

/// `Meeples::getInLocation($location, $state)`
pub fn get_in_location(g: &Game, location: &str, state: Option<i64>) -> R<Vec<i64>> {
    pieces::get_in_location::<MeeplesT>(g, location, state)
}

/// `CachedPieces::getFiltered($pId, $location, $type)` on meeples
// PHP: Helpers/CachedPieces.php:290
pub fn get_filtered(g: &Game, pid: &Value, location: &Value, ty: &Value) -> R<Vec<i64>> {
    let mut ids = get_all(g);
    let filt = |ids: Vec<i64>, value: &Value, f: &dyn Fn(&Meeple) -> String| -> Vec<i64> {
        if value.is_null() {
            return ids;
        }
        ids.into_iter()
            .filter(|id| {
                let ov = Value::String(f(&g.meeples[id]));
                if php::is_arr(value) {
                    php::in_array(&ov, value)
                } else {
                    let v = php::strval(value);
                    if v.contains('%') {
                        php::like_match(&v, &php::strval(&ov))
                    } else {
                        php::loose_eq(&ov, value)
                    }
                }
            })
            .collect()
    };
    ids = filt(ids, location, &|m| m.location.clone());
    ids = filt(ids, pid, &|m| m.pid.clone());
    ids = filt(ids, ty, &|m| m.kind.clone());
    Ok(ids)
}

/// `Meeples::getFilteredOrdered($pId, $location, $type)`
pub fn get_filtered_ordered(g: &Game, pid: &Value, location: &Value, ty: &Value) -> R<Vec<i64>> {
    let mut ids = get_filtered(g, pid, location, ty)?;
    pieces::order_by_state::<MeeplesT>(g, &mut ids, true);
    Ok(ids)
}

/// `Meeples::countMeeples($location, $type)`
pub fn count_meeples(g: &Game, location: &str, ty: &Value) -> R<usize> {
    Ok(get_of_type(g, location, ty)?.len())
}

/// `Meeples::getOfType($location, $type)`
pub fn get_of_type(g: &Game, location: &str, ty: &Value) -> R<Vec<i64>> {
    get_filtered_ordered(g, &Value::Null, &json!(location), ty)
}

/// `Meeples::getStormTokens($pId)`
pub fn get_storm_tokens(g: &Game, pid: PId) -> R<Vec<i64>> {
    get_filtered_ordered(g, &json!(pid), &Value::Null, &json!(["companion", "hero"]))
}

/// `Meeples::getAscended($pId, $location)`
pub fn get_ascended(g: &Game, pid: PId, location: &str) -> R<Vec<i64>> {
    get_filtered_ordered(g, &json!(pid), &json!(location), &json!(["ascend"]))
}

/// `Meeples::create($meeples)`: rows read back from the DB (state, type and player as strings)
// PHP: Helpers/CachedPieces.php:519
pub fn create(g: &mut Game, items: &Value) -> R<Vec<i64>> {
    let mut ids = vec![];
    for info in php::vals(items) {
        let n = php::intval(php::coalesce(php::get(info, "nbr"), &json!(1)));
        let location = php::strval(php::get(info, "location"));
        if php::get(info, "location").is_null() {
            return Err(crate::PhpError::new("Class Pieces : create location cannot be null (set per token location or location_global"));
        }
        pieces::check_location(&location, false)?;
        let state = php::intval(php::get(info, "state"));
        for _ in 0..n {
            let id = g.next_meeple_id;
            g.next_meeple_id += 1;
            g.meeples.insert(
                id,
                Meeple { id, location: location.clone(), state: state.to_string(), kind: php::strval(php::get(info, "type")), pid: php::strval(php::get(info, "player_id")) },
            );
            ids.push(id);
        }
    }
    Ok(ids)
}

/// `Meeples::singleCreate($meeple)`
pub fn single_create(g: &mut Game, info: &Value) -> R<Option<i64>> {
    Ok(create(g, &json!([info]))?.first().copied())
}

/// `Meeples::delete($ids)`
pub fn delete(g: &mut Game, ids: &Value) -> R<Vec<i64>> {
    let ids: Vec<Value> = if php::is_arr(ids) { php::vals(ids).into_iter().cloned().collect() } else { vec![ids.clone()] };
    pieces::delete::<MeeplesT>(g, &ids)
}

/// `Meeples::move($ids, $location, $state)`
pub fn move_meeples(g: &mut Game, ids: &Value, location: &str, state: i64) -> R<Vec<i64>> {
    let ids: Vec<Value> = if php::is_arr(ids) { php::vals(ids).into_iter().cloned().collect() } else { vec![ids.clone()] };
    pieces::move_pieces::<MeeplesT>(g, &ids, location, state)
}

/// `Meeples::setupPlayer($player)`
// PHP: Managers/Meeples.php:47
pub fn setup_player(g: &mut Game, pid: PId) -> R<Vec<i64>> {
    create(g, &json!([{"type": "companion", "location": "storm-7", "player_id": pid}, {"type": "hero", "location": "storm-0", "player_id": pid}]))
}

/// `Meeples::createHeroMarkers()`
// PHP: Managers/Meeples.php:55
pub fn create_hero_markers(g: &mut Game) -> R<Vec<i64>> {
    let mut to_create = vec![];
    for pid in crate::managers::players::get_all(g) {
        let hero = crate::models::player::get_hero(g, pid)?.ok_or_else(|| crate::PhpError::new("Error: Call to a member function isCreateMarkers() on null"))?;
        if crate::models::card::is(g, hero, "createMarkers")? {
            for t in ["ocean", "forest", "mountain"] {
                to_create.push(json!({"type": t, "location": format!("card-{}", hero), "player_id": pid}));
            }
        }
    }
    if !to_create.is_empty() {
        return create(g, &Value::Array(to_create));
    }
    Ok(vec![])
}

/// `Meeples::createOnCard($type, $cardId, $pId, $nbr)`
pub fn create_on_card(g: &mut Game, ty: &str, card_id: i64, pid: &Value, nbr: i64) -> R<Vec<i64>> {
    if nbr <= 0 {
        return Ok(vec![]);
    }
    if nbr == 1 {
        return Ok(single_create(g, &json!({"type": ty, "location": format!("card-{}", card_id), "player_id": pid}))?.into_iter().collect());
    }
    create(g, &json!([{"type": ty, "location": format!("card-{}", card_id), "player_id": pid, "nbr": nbr}]))
}

/// `Meeples::nightCleanup()`: ASCEND markers deleted
// PHP: Managers/Meeples.php:115
pub fn night_cleanup(g: &mut Game) -> R<()> {
    let ascended = get_filtered_ordered(g, &Value::Null, &Value::Null, &json!(["ascend"]))?;
    if !ascended.is_empty() {
        delete(g, &json!(ascended))?;
        g.notify("silentKill", json!({"ids": ascended}));
    }
    Ok(())
}

/// `Meeples::getNextPlayedMarker()`
pub fn get_next_played_marker(g: &Game) -> R<i64> {
    let mut max = 1;
    for m in get_of_type(g, "storm-%", &json!(["ocean", "forest", "mountain"]))? {
        max = max.max(php::intval(&json!(g.meeples[&m].state)));
    }
    Ok(max + 1)
}
