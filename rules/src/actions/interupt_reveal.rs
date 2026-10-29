//! Actions/InteruptReveal.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:22), argsMarkRegion (:32), actInteruptReveal (:39)
//! Default args: `protected $args` at Actions/InteruptReveal.php:27 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct InteruptReveal;

impl ActionClass for InteruptReveal {
    fn name(&self) -> &'static str {
        "InteruptReveal"
    }

    // PHP: Actions/InteruptReveal.php:17
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_INTERRUPT_REVEAL))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/InteruptReveal.php:22 getDescription"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actinteruptreveal" => Err(PhpError::nt("Actions/InteruptReveal.php:39 actInteruptReveal")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\InteruptReveal::{}()", method))),
        }
    }
}
