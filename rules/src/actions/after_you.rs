//! Actions/AfterYou.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:16), isDoable (:23), getCost (:28), stAfterYou (:47)
//! Default args: `protected $args` at Actions/AfterYou.php:21 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct AfterYou;

impl ActionClass for AfterYou {
    fn name(&self) -> &'static str {
        "AfterYou"
    }

    // PHP: Actions/AfterYou.php:11
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_AFTER_YOU))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/AfterYou.php:16 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/AfterYou.php:23 isDoable"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/AfterYou.php:47 stAfterYou"))
    }
}
