//! Actions/Discard.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! isOptional (:46), canPassAction (:51), cannotFulfillDiscard (:56), isSacrifice (:73), isSabotage (:78), getDiscardControllerId (:83), getDescription (:98), isDoable (:152), getPlayer (:157), argsDiscard (:163), stDiscard (:220), actDiscard (:273)
//! Default args: `protected $args` at Actions/Discard.php:26 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Discard;

impl ActionClass for Discard {
    fn name(&self) -> &'static str {
        "Discard"
    }

    // PHP: Actions/Discard.php:21
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Discard.php:98 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Discard.php:152 isDoable"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Discard.php:46 isOptional"))
    }

    fn can_pass_action(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Discard.php:51 canPassAction"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/Discard.php:157 getPlayer"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Discard.php:163 argsDiscard"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Discard.php:220 stDiscard"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actdiscard" => Err(PhpError::nt("Actions/Discard.php:273 actDiscard")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Discard::{}()", method))),
        }
    }
}
