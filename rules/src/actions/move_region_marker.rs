//! Actions/MoveRegionMarker.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:16), argsMoveRegionMarker (:35), getMarkers (:44), isDoable (:50), isOptional (:55), actMoveRegionMarker (:60)
//! Default args: `protected $args` at Actions/MoveRegionMarker.php:31 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveRegionMarker;

impl ActionClass for MoveRegionMarker {
    fn name(&self) -> &'static str {
        "MoveRegionMarker"
    }

    // PHP: Actions/MoveRegionMarker.php:11
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_REGION_MARKER))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/MoveRegionMarker.php:16 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MoveRegionMarker.php:50 isDoable"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MoveRegionMarker.php:55 isOptional"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/MoveRegionMarker.php:35 argsMoveRegionMarker"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmoveregionmarker" => Err(PhpError::nt("Actions/MoveRegionMarker.php:60 actMoveRegionMarker")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MoveRegionMarker::{}()", method))),
        }
    }
}
