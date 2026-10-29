//! Actions/DiscardDo.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), isOptional (:60), getTargetableCards (:65), argsDiscardDo (:148), actTarget (:165)
//! Default args: `protected $args` at Actions/DiscardDo.php:39 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DiscardDo;

impl ActionClass for DiscardDo {
    fn name(&self) -> &'static str {
        "DiscardDo"
    }

    // PHP: Actions/DiscardDo.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD_DO))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/DiscardDo.php:24 getDescription"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DiscardDo.php:60 isOptional"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/DiscardDo.php:148 argsDiscardDo"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttarget" => Err(PhpError::nt("Actions/DiscardDo.php:165 actTarget")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\DiscardDo::{}()", method))),
        }
    }
}
