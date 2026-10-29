//! Actions/CheckCondition.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! isFlowEffect (:26), getConditions (:31), getDescription (:42), isIndependent (:68), isDoable (:85), effectRequiresDeck (:90), getDeckPlayer (:133), checkCondition (:143), stCheckCondition (:170)
//! Default args: `protected $args` at Actions/CheckCondition.php:24 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct CheckCondition;

impl ActionClass for CheckCondition {
    fn name(&self) -> &'static str {
        "CheckCondition"
    }

    // PHP: Actions/CheckCondition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_CHECK_CONDITION))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/CheckCondition.php:42 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/CheckCondition.php:85 isDoable"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/CheckCondition.php:68 isIndependent"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/CheckCondition.php:170 stCheckCondition"))
    }
}
