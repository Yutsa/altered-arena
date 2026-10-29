//! Actions/NightCleanup.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), argsNightCleanup (:34), actNightCleanup (:61)
//! Default args: `protected $args` at Actions/NightCleanup.php:29 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct NightCleanup;

impl ActionClass for NightCleanup {
    fn name(&self) -> &'static str {
        "NightCleanup"
    }

    // PHP: Actions/NightCleanup.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_NIGHT_CLEANUP))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/NightCleanup.php:24 getDescription"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/NightCleanup.php:34 argsNightCleanup"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actnightcleanup" => Err(PhpError::nt("Actions/NightCleanup.php:61 actNightCleanup")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\NightCleanup::{}()", method))),
        }
    }
}
