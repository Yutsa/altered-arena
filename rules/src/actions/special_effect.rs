//! Actions/SpecialEffect.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), isAutomatic (:275), isIndependent (:280), getCard (:321), stSpecialEffect (:336)
//! Default args: `protected $args` at Actions/SpecialEffect.php:334 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct SpecialEffect;

impl ActionClass for SpecialEffect {
    fn name(&self) -> &'static str {
        "SpecialEffect"
    }

    // PHP: Actions/SpecialEffect.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPECIAL_EFFECT))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/SpecialEffect.php:24 getDescription"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/SpecialEffect.php:280 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/SpecialEffect.php:275 isAutomatic"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/SpecialEffect.php:336 stSpecialEffect"))
    }
}
