//! Actions/MoveRegionMarker.php: moves a terrain marker of a type to the region of the source's
//! expedition.

use crate::framework::*;
use crate::game::{Game, PId};
use crate::managers::meeples;
use crate::models::action::{get_arg, get_source, param_req, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveRegionMarker;

/// A collection of meeples as BGA serializes it: `id => [id, location, state, type, pId]`
pub fn meeples_json(g: &Game, ids: &[i64]) -> R<Value> {
    let mut out = php::arr();
    for id in ids {
        let m = g.meeple(*id)?;
        php::set(&mut out, &id.to_string(), json!({"id": m.id, "location": m.location, "state": m.state, "type": m.kind, "pId": m.pid}));
    }
    Ok(out)
}

/// `getMarkers()`: the terrain markers of the type in the regions
// PHP: Actions/MoveRegionMarker.php:48
fn get_markers(a: &MoveRegionMarker, g: &mut Game, ctx: Ctx) -> R<Vec<i64>> {
    let types = json!([get_arg(a, g, ctx, "markerType")?]);
    meeples::get_of_type(g, "storm-%", &types)
}

impl ActionClass for MoveRegionMarker {
    fn name(&self) -> &'static str {
        "MoveRegionMarker"
    }

    // PHP: Actions/MoveRegionMarker.php:32
    fn default_args(&self) -> Value {
        json!({"markerType": null})
    }

    // PHP: Actions/MoveRegionMarker.php:11
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_REGION_MARKER))
    }

    // PHP: Actions/MoveRegionMarker.php:16
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let marker_type = get_arg(self, g, ctx, "markerType")?;
        let log = match php::key_of(&marker_type).as_str() {
            "forest" => Some("Move a Forest {V} Terrain Marker to my region"),
            "mountain" => Some("Move a {M} Terrain Marker to my region"),
            "ocean" => Some("Move a {O} Terrain Marker to my region"),
            _ => None,
        };
        if let Some(log) = log {
            return Ok(json!({"log": log, "args": []}));
        }
        Ok(json!("Move a region terrain marker"))
    }

    // PHP: Actions/MoveRegionMarker.php:36
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let marker_type = get_arg(self, g, ctx, "markerType")?;
        let markers = get_markers(self, g, ctx)?;
        Ok(Some(json!({"markerType": marker_type, "markers": meeples_json(g, &markers)?})))
    }

    // PHP: Actions/MoveRegionMarker.php:54
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(!get_markers(self, g, ctx)?.is_empty())
    }

    // PHP: Actions/MoveRegionMarker.php:59
    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmoveregionmarker" => {
                let marker_id = param_req(args, 0, "markerId", "ALT\\Actions\\MoveRegionMarker::actMoveRegionMarker")?;
                act_move_region_marker(self, g, ctx, &marker_id)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MoveRegionMarker::{}()", method))),
        }
    }
}

/// `actMoveRegionMarker($markerId)`
// PHP: Actions/MoveRegionMarker.php:64
fn act_move_region_marker(a: &MoveRegionMarker, g: &mut Game, ctx: Ctx, marker_id: &Value) -> R<()> {
    let markers = get_markers(a, g, ctx)?;
    let Some(marker) = markers.iter().copied().find(|m| php::loose_eq(&json!(m.to_string()), &json!(php::key_of(marker_id)))) else {
        return Err(PhpError::new("Invalid terrain marker. Should not happen"));
    };

    // move the marker
    let source = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
    let sp = card::get_player(g, source)?.ok_or_else(|| PhpError::new("Error: Call to a member function getHeroToken() on null"))?;
    let tok = if card::location(g, source)? == "stormLeft" { player::get_hero_token(g, sp)? } else { player::get_companion_token(g, sp)? };
    let loc = g.meeple(tok)?.location.clone();
    g.meeple_mut(marker)?.location = loc;
    let next = meeples::get_next_played_marker(g)?;
    g.meeple_mut(marker)?.state = next.to_string();

    // Notify
    g.notify("moveTerrainMarker", json!({"player_id": g.active_player_id(), "meepleId": marker}));
    resolve_action(a, g, ctx, json!([marker_id]), json!(false))
}
