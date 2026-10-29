//! Actions/TargetExpedition.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:26), argsTargetExpedition (:41), isDoable (:81), stTargetExpedition (:86), actTargetExpedition (:93)
//! Default args: `protected $args` at Actions/TargetExpedition.php:24 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct TargetExpedition;

impl ActionClass for TargetExpedition {
    fn name(&self) -> &'static str {
        "TargetExpedition"
    }

    // PHP: Actions/TargetExpedition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET_EXPEDITION))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/TargetExpedition.php:26 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/TargetExpedition.php:81 isDoable"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/TargetExpedition.php:41 argsTargetExpedition"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/TargetExpedition.php:86 stTargetExpedition"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttargetexpedition" => Err(PhpError::nt("Actions/TargetExpedition.php:93 actTargetExpedition")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\TargetExpedition::{}()", method))),
        }
    }
}
