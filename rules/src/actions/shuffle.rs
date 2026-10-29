//! Actions/Shuffle.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:20), isAutomatic (:25), isIndependent (:30), isOptional (:35), isIrreversible (:40), stShuffle (:47)
//! Default args: `protected $args` at Actions/Shuffle.php:45 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Shuffle;

impl ActionClass for Shuffle {
    fn name(&self) -> &'static str {
        "Shuffle"
    }

    // PHP: Actions/Shuffle.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SHUFFLE))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Shuffle.php:20 getDescription"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Shuffle.php:35 isOptional"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Shuffle.php:30 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Shuffle.php:25 isAutomatic"))
    }

    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Shuffle.php:40 isIrreversible"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Shuffle.php:47 stShuffle"))
    }
}
