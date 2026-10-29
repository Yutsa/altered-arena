//! Actions/MarkRegion.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:22), argsMarkRegion (:32), getMarkers (:41), isDoable (:47), stPreMarkRegion (:52), actMarkRegion (:59)
//! Default args: `protected $args` at Actions/MarkRegion.php:27 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MarkRegion;

impl ActionClass for MarkRegion {
    fn name(&self) -> &'static str {
        "MarkRegion"
    }

    // PHP: Actions/MarkRegion.php:17
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MARK_REGION))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/MarkRegion.php:22 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MarkRegion.php:47 isDoable"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/MarkRegion.php:32 argsMarkRegion"))
    }

    fn has_st_pre(&self) -> bool {
        true
    }

    fn st_pre(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/MarkRegion.php:52 stPreMarkRegion"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmarkregion" => Err(PhpError::nt("Actions/MarkRegion.php:59 actMarkRegion")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MarkRegion::{}()", method))),
        }
    }
}
