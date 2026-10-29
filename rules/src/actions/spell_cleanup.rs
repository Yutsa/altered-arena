//! Actions/SpellCleanup.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:17), isAutomatic (:22), isIndependent (:27), getCard (:32), stSpellCleanup (:42)

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct SpellCleanup;

impl ActionClass for SpellCleanup {
    fn name(&self) -> &'static str {
        "SpellCleanup"
    }

    // PHP: Actions/SpellCleanup.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPELL_CLEANUP))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/SpellCleanup.php:17 getDescription"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/SpellCleanup.php:27 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/SpellCleanup.php:22 isAutomatic"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/SpellCleanup.php:42 stSpellCleanup"))
    }
}
