//! Actions/Tap.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:20), isDoable (:29), getCard (:34), stTap (:47)

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Tap;

impl ActionClass for Tap {
    fn name(&self) -> &'static str {
        "Tap"
    }

    // PHP: Actions/Tap.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TAP))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Tap.php:20 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Tap.php:29 isDoable"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Tap.php:47 stTap"))
    }
}
