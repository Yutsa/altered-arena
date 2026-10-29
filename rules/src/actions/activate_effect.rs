//! Actions/ActivateEffect.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:21), isAutomatic (:63), isIndependent (:68), getCard (:73), resolveActivateCardId (:102), resolveTargetPickCardId (:138), stActivateEffect (:164)
//! Default args: `protected $args` at Actions/ActivateEffect.php:158 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct ActivateEffect;

impl ActionClass for ActivateEffect {
    fn name(&self) -> &'static str {
        "ActivateEffect"
    }

    // PHP: Actions/ActivateEffect.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_ACTIVATE_EFFECT))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/ActivateEffect.php:21 getDescription"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/ActivateEffect.php:68 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/ActivateEffect.php:63 isAutomatic"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/ActivateEffect.php:164 stActivateEffect"))
    }
}
