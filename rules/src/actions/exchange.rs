//! Actions/Exchange.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:48), isOptional (:66), isDoable (:76), getTargetableCards (:87), argsExchange (:109), actExchange (:121)
//! Default args: `protected $args` at Actions/Exchange.php:25 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Exchange;

impl ActionClass for Exchange {
    fn name(&self) -> &'static str {
        "Exchange"
    }

    // PHP: Actions/Exchange.php:20
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_EXCHANGE))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Exchange.php:48 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Exchange.php:76 isDoable"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Exchange.php:66 isOptional"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Exchange.php:109 argsExchange"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actexchange" => Err(PhpError::nt("Actions/Exchange.php:121 actExchange")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Exchange::{}()", method))),
        }
    }
}
