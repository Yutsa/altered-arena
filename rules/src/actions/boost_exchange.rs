//! Actions/BoostExchange.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:19), isAutomatic (:24), isIndependent (:29), getPlayer (:34), stBoostExchange (:44), resolveSwapIds (:101), findPreviousTargetCardId (:130), isResolvedCardId (:145)
//! Default args: `protected $args` at Actions/BoostExchange.php:40 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct BoostExchange;

impl ActionClass for BoostExchange {
    fn name(&self) -> &'static str {
        "BoostExchange"
    }

    // PHP: Actions/BoostExchange.php:14
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_BOOST_EXCHANGE))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/BoostExchange.php:19 getDescription"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/BoostExchange.php:29 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/BoostExchange.php:24 isAutomatic"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/BoostExchange.php:34 getPlayer"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/BoostExchange.php:44 stBoostExchange"))
    }
}
