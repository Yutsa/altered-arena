//! Actions/EndAfternoon.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:17), isOptional (:25), isDoable (:30), stEndAfternoon (:35)

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct EndAfternoon;

impl ActionClass for EndAfternoon {
    fn name(&self) -> &'static str {
        "EndAfternoon"
    }

    // PHP: Actions/EndAfternoon.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_END_AFTERNOON))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/EndAfternoon.php:17 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/EndAfternoon.php:30 isDoable"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/EndAfternoon.php:25 isOptional"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/EndAfternoon.php:35 stEndAfternoon"))
    }
}
