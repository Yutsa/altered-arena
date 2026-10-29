//! Actions/Draw.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:20), isAutomatic (:68), isIrreversible (:73), getDrawTargetPlayers (:78), isDoable (:101), stDraw (:117)
//! Default args: `protected $args` at Actions/Draw.php:109 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Draw;

impl ActionClass for Draw {
    fn name(&self) -> &'static str {
        "Draw"
    }

    // PHP: Actions/Draw.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DRAW))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Draw.php:20 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Draw.php:101 isDoable"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Draw.php:68 isAutomatic"))
    }

    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Draw.php:73 isIrreversible"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Draw.php:117 stDraw"))
    }
}
