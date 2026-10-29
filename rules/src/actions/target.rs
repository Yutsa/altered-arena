//! Actions/Target.php
//!
//! Not translated yet: every method below returns `PhpError::nt` (the base-class defaults of
//! Models/Action.php apply to the methods the PHP class does not define). PHP methods:
//! getDescription (:65), isDoable (:181), isOptional (:203), getTargetableCards (:213), getTargetCosts (:440), getAdditionalTargetCost (:463), argsTarget (:478), stTarget (:495), actTarget (:503), isCompareTargetBiomeSpecActive (:622), resolveCompareTargetBiomeFilter (:633), getCompareBiomeDescriptionLabel (:708), resolveSingleSubType (:725), canTargetSourceAsBoostSwapPair (:742), isResolvedTargetCardId (:772), getSubtypeTargetLabel (:780)
//! Default args: `protected $args` at Actions/Target.php:25 (to port into `default_args`).

#![allow(unused_imports)]
use crate::framework::*;
use crate::game::{Game, PId};
use crate::models::action::{ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Target;

impl ActionClass for Target {
    fn name(&self) -> &'static str {
        "Target"
    }

    // PHP: Actions/Target.php:20
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET))
    }

    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Err(PhpError::nt("Actions/Target.php:65 getDescription"))
    }

    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Target.php:181 isDoable"))
    }

    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Err(PhpError::nt("Actions/Target.php:203 isOptional"))
    }

    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Target.php:478 argsTarget"))
    }

    fn st(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Err(PhpError::nt("Actions/Target.php:495 stTarget"))
    }

    fn act(&self, _g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttarget" => Err(PhpError::nt("Actions/Target.php:503 actTarget")),
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Target::{}()", method))),
        }
    }
}
