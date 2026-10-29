//! Actions/BlockExpedition.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:24), argsBlockExpedition (:29), isAutomatic (:34), stBlockExpedition (:39), actBlockExpedition (:46)

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct BlockExpedition;

impl ActionClass for BlockExpedition {
    fn name(&self) -> &'static str {
        "BlockExpedition"
    }

    // PHP: Actions/BlockExpedition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_BLOCK_EXPEDITION))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/BlockExpedition.php:24 getDescription"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/BlockExpedition.php:34 isAutomatic"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/BlockExpedition.php:29 argsBlockExpedition"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/BlockExpedition.php:39 stBlockExpedition"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actblockexpedition" => Err(PhpError::nt("Actions/BlockExpedition.php:46 actBlockExpedition")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\BlockExpedition::{}()", method))),
        }
    }
}
