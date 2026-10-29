//! Actions/DrawMana.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:20), isAutomatic (:30), isIrreversible (:35), isDoable (:40), getPlayer (:45), stDrawMana (:57)
//! Default args: `protected $args` at Actions/DrawMana.php:51 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DrawMana;

impl ActionClass for DrawMana {
    fn name(&self) -> &'static str {
        "DrawMana"
    }

    // PHP: Actions/DrawMana.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DRAW_MANA))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/DrawMana.php:20 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DrawMana.php:40 isDoable"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DrawMana.php:30 isAutomatic"))
    }

    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DrawMana.php:35 isIrreversible"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/DrawMana.php:45 getPlayer"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/DrawMana.php:57 stDrawMana"))
    }
}
