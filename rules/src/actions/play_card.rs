//! Actions/PlayCard.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:20), getPlayer (:43), getCard (:49), isDoable (:77), argsPlayCard (:85), stPlayCard (:116), actPlayCard (:123)
//! Default args: `protected $args` at Actions/PlayCard.php:64 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct PlayCard;

impl ActionClass for PlayCard {
    fn name(&self) -> &'static str {
        "PlayCard"
    }

    // PHP: Actions/PlayCard.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_PLAY_CARD))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/PlayCard.php:20 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/PlayCard.php:77 isDoable"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/PlayCard.php:43 getPlayer"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/PlayCard.php:85 argsPlayCard"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/PlayCard.php:116 stPlayCard"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actplaycard" => Err(PhpError::nt("Actions/PlayCard.php:123 actPlayCard")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\PlayCard::{}()", method))),
        }
    }
}
