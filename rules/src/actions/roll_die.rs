//! Actions/RollDie.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), getN (:36), isAutomatic (:48), isIrreversible (:53), getSource (:66), getGain (:76), getSourceCounterValue (:114), getSelectableRolls (:128), getCounterSpendForRoll (:141), stPreRollDie (:165), argsRollDie (:207), stRollDie (:234), actRollDie (:245), actDiscardAdd (:299)
//! Default args: `protected $args` at Actions/RollDie.php:58 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct RollDie;

impl ActionClass for RollDie {
    fn name(&self) -> &'static str {
        "RollDie"
    }

    // PHP: Actions/RollDie.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_ROLL_DIE))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/RollDie.php:24 getDescription"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/RollDie.php:48 isAutomatic"))
    }

    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/RollDie.php:53 isIrreversible"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/RollDie.php:207 argsRollDie"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/RollDie.php:234 stRollDie"))
    }

    fn has_st_pre(&self) -> bool {
        true
    }

    fn st_pre(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/RollDie.php:165 stPreRollDie"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actrolldie" => Err(PhpError::nt("Actions/RollDie.php:245 actRollDie")),
            "actdiscardadd" => Err(PhpError::nt("Actions/RollDie.php:299 actDiscardAdd")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\RollDie::{}()", method))),
        }
    }
}
