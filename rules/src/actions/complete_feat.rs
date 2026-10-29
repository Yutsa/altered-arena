//! Actions/CompleteFeat.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:17), isAutomatic (:27), getTargetCard (:41), stCompleteFeat (:63)

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct CompleteFeat;

impl ActionClass for CompleteFeat {
    fn name(&self) -> &'static str {
        "CompleteFeat"
    }

    // PHP: Actions/CompleteFeat.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_COMPLETE_FEAT))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/CompleteFeat.php:17 getDescription"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/CompleteFeat.php:27 isAutomatic"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/CompleteFeat.php:63 stCompleteFeat"))
    }
}
