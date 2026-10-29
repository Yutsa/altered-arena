//! Actions/TargetPlayer.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:29), argsTargetPlayer (:43), stTargetPlayer (:53), actTargetPlayer (:62)
//! Default args: `protected $args` at Actions/TargetPlayer.php:24 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct TargetPlayer;

impl ActionClass for TargetPlayer {
    fn name(&self) -> &'static str {
        "TargetPlayer"
    }

    // PHP: Actions/TargetPlayer.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET_PLAYER))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/TargetPlayer.php:29 getDescription"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/TargetPlayer.php:43 argsTargetPlayer"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/TargetPlayer.php:53 stTargetPlayer"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttargetplayer" => Err(PhpError::nt("Actions/TargetPlayer.php:62 actTargetPlayer")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\TargetPlayer::{}()", method))),
        }
    }
}
