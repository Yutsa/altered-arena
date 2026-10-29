//! Actions/MoveCard.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:21), isAutomatic (:37), getCard (:44), stMoveCard (:61)
//! Default args: `protected $args` at Actions/MoveCard.php:42 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveCard;

impl ActionClass for MoveCard {
    fn name(&self) -> &'static str {
        "MoveCard"
    }

    // PHP: Actions/MoveCard.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_CARD))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/MoveCard.php:21 getDescription"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/MoveCard.php:37 isAutomatic"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/MoveCard.php:61 stMoveCard"))
    }
}
