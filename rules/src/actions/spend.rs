//! Actions/Spend.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), isAutomatic (:39), getSource (:52), getCard (:62), resolveSpendCardId (:75), argsSpend (:113), hasEnoughToSpend (:127), isDoable (:135), stSpend (:141), actSpend (:148)
//! Default args: `protected $args` at Actions/Spend.php:45 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Spend;

impl ActionClass for Spend {
    fn name(&self) -> &'static str {
        "Spend"
    }

    // PHP: Actions/Spend.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPEND))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Spend.php:24 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Spend.php:135 isDoable"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Spend.php:39 isAutomatic"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Spend.php:113 argsSpend"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Spend.php:141 stSpend"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actspend" => Err(PhpError::nt("Actions/Spend.php:148 actSpend")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Spend::{}()", method))),
        }
    }
}
