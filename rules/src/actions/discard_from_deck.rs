//! Actions/DiscardFromDeck.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:23), isAutomatic (:33), isIrreversible (:38), getPlayer (:43), stDiscardFromDeck (:64)
//! Default args: `protected $args` at Actions/DiscardFromDeck.php:59 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DiscardFromDeck;

impl ActionClass for DiscardFromDeck {
    fn name(&self) -> &'static str {
        "DiscardFromDeck"
    }

    // PHP: Actions/DiscardFromDeck.php:18
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD_FROM_DECK))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/DiscardFromDeck.php:23 getDescription"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DiscardFromDeck.php:33 isAutomatic"))
    }

    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/DiscardFromDeck.php:38 isIrreversible"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/DiscardFromDeck.php:43 getPlayer"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/DiscardFromDeck.php:64 stDiscardFromDeck"))
    }
}
