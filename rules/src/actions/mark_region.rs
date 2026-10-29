//! Actions/MarkRegion.php: puts a terrain marker of the source card on a visible region.

use crate::core::globals;
use crate::framework::*;
use crate::game::{Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::action::{get_arg, get_source, get_source_id, param_req, resolve_action, ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

use super::move_region_marker::meeples_json;

pub struct MarkRegion;

/// `getMarkers()`: the terrain markers on the source card
// PHP: Actions/MarkRegion.php:41
fn get_markers(g: &mut Game, ctx: Ctx) -> R<Vec<i64>> {
    let sid = get_source_id(g, ctx)?;
    let source = cards::get(g, &sid)?;
    meeples::get_of_type(g, &format!("card-{}", source), &json!(["ocean", "forest", "mountain"]))
}

impl ActionClass for MarkRegion {
    fn name(&self) -> &'static str {
        "MarkRegion"
    }

    // PHP: Actions/MarkRegion.php:27
    fn default_args(&self) -> Value {
        json!({"create": false, "regionType": null})
    }

    // PHP: Actions/MarkRegion.php:17
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MARK_REGION))
    }

    // PHP: Actions/MarkRegion.php:22
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Mark a visible region"))
    }

    // PHP: Actions/MarkRegion.php:32
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let regions = globals::get_visible_regions(g)?;
        let markers = get_markers(g, ctx)?;
        Ok(Some(json!({"regions": regions, "markers": meeples_json(g, &markers)?})))
    }

    // PHP: Actions/MarkRegion.php:47
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(!get_markers(g, ctx)?.is_empty() || get_arg(self, g, ctx, "create")? == json!(true))
    }

    fn has_st_pre(&self) -> bool {
        true
    }

    // PHP: Actions/MarkRegion.php:52
    fn st_pre(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        if get_arg(self, g, ctx, "create")? == json!(true) {
            let ty = get_arg(self, g, ctx, "regionType")?;
            let sid = get_source_id(g, ctx)?;
            let cur = players::get_current_id(g);
            meeples::single_create(g, &json!({"type": ty, "location": format!("card-{}", php::strval(&sid)), "player_id": cur}))?;
        }
        Ok(Value::Null)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmarkregion" => {
                let marker_id = param_req(args, 0, "markerId", "ALT\\Actions\\MarkRegion::actMarkRegion")?;
                let storm_id = param_req(args, 1, "stormId", "ALT\\Actions\\MarkRegion::actMarkRegion")?;
                act_mark_region(self, g, ctx, &marker_id, &storm_id)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MarkRegion::{}()", method))),
        }
    }
}

/// `actMarkRegion($markerId, $stormId)`
// PHP: Actions/MarkRegion.php:59
fn act_mark_region(a: &MarkRegion, g: &mut Game, ctx: Ctx, marker_id: &Value, storm_id: &Value) -> R<()> {
    let _regions = globals::get_visible_regions(g)?;
    let markers = get_markers(g, ctx)?;
    let Some(marker) = markers.iter().copied().find(|m| m.to_string() == php::key_of(marker_id)) else {
        return Err(PhpError::new("Invalid terrain marker. Should not happen"));
    };

    // move the marker
    g.meeple_mut(marker)?.location = format!("storm-{}", php::strval(storm_id));
    let next = meeples::get_next_played_marker(g)?;
    g.meeple_mut(marker)?.state = next.to_string();

    // Notify (the source card is serialized: getSource() must hold)
    let _source = get_source(g, ctx)?;
    g.notify("setTerrainMarker", json!({"player_id": g.active_player_id(), "meepleId": marker}));
    resolve_action(a, g, ctx, json!([marker_id, storm_id]), json!(false))
}
