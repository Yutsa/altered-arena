//! Actions/MoveExpedition.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:18), getSides (:35), argsMoveExpedition (:63), isDoable (:98), isAutomatic (:104), stMoveExpedition (:110), actMoveExpedition (:128)
//! Default args: `protected $args` at Actions/MoveExpedition.php:23 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveExpedition;

impl ActionClass for MoveExpedition {
    fn name(&self) -> &'static str {
        "MoveExpedition"
    }

    // PHP: Actions/MoveExpedition.php:13
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_EXPEDITION))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/MoveExpedition.php:18 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MoveExpedition.php:98 isDoable"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MoveExpedition.php:104 isAutomatic"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/MoveExpedition.php:63 argsMoveExpedition"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/MoveExpedition.php:110 stMoveExpedition"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmoveexpedition" => Err(PhpError::nt("Actions/MoveExpedition.php:128 actMoveExpedition")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MoveExpedition::{}()", method))),
        }
    }
}
