//! Actions/Pay.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), isDoable (:35), isMandatory (:47), argsPay (:53), stPay (:85), actPay (:92)
//! Default args: `protected $args` at Actions/Pay.php:19 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Pay;

impl ActionClass for Pay {
    fn name(&self) -> &'static str {
        "Pay"
    }

    // PHP: Actions/Pay.php:14
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_PAY))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Pay.php:24 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Pay.php:35 isDoable"))
    }

    fn is_mandatory(&self, _g: &mut Game, _ctx: Ctx) -> R<bool> {
        Err(PhpError::nt("Actions/Pay.php:47 isMandatory"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Pay.php:53 argsPay"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Pay.php:85 stPay"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actpay" => Err(PhpError::nt("Actions/Pay.php:92 actPay")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Pay::{}()", method))),
        }
    }
}
