//! Actions/InvokeToken.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:31), getInvokePlayerId (:44), argsInvokeToken (:72), getN (:94), getToken (:106), isAutomatic (:114), isIndependent (:122), getLocationInfos (:130), getDisplayLocation (:176), stInvokeToken (:188), getPlayer (:202), actInvokeToken (:214)
//! Default args: `protected $args` at Actions/InvokeToken.php:24 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct InvokeToken;

impl ActionClass for InvokeToken {
    fn name(&self) -> &'static str {
        "InvokeToken"
    }

    // PHP: Actions/InvokeToken.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_INVOKE_TOKEN))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/InvokeToken.php:31 getDescription"))
    }

    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/InvokeToken.php:122 isIndependent"))
    }

    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/InvokeToken.php:114 isAutomatic"))
    }

    fn get_player(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<PId>> {
        Err(PhpError::nt("Actions/InvokeToken.php:202 getPlayer"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/InvokeToken.php:72 argsInvokeToken"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/InvokeToken.php:188 stInvokeToken"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actinvoketoken" => Err(PhpError::nt("Actions/InvokeToken.php:214 actInvokeToken")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\InvokeToken::{}()", method))),
        }
    }
}
