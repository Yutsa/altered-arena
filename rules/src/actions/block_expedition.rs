//! Actions/BlockExpedition.php: an expedition (or all of them) cannot move at the next Dusk.

use crate::core::globals;
use crate::framework::*;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{get_ctx_arg, param_req, ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct BlockExpedition;

/// `($this->getCtxArg('expedition') ?? '') == 'all'`
fn is_all(g: &Game, ctx: Ctx) -> bool {
    php::loose_eq(php::coalesce(&get_ctx_arg(g, ctx, "expedition"), &json!("")), &json!("all"))
}

impl ActionClass for BlockExpedition {
    fn name(&self) -> &'static str {
        "BlockExpedition"
    }

    // PHP: Actions/BlockExpedition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_BLOCK_EXPEDITION))
    }

    // PHP: Actions/BlockExpedition.php:24
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Block an expedition"))
    }

    // PHP: Actions/BlockExpedition.php:29
    fn args(&self, _g: &mut Game, _ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(php::arr()))
    }

    // PHP: Actions/BlockExpedition.php:34
    fn is_automatic(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(is_all(g, ctx))
    }

    // PHP: Actions/BlockExpedition.php:39
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        if is_all(g, ctx) {
            return Ok(Some(json!(["all"])));
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actblockexpedition" => {
                let expedition = param_req(args, 0, "expedition", "ALT\\Actions\\BlockExpedition::actBlockExpedition")?;
                act_block_expedition(g, ctx, &expedition)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\BlockExpedition::{}()", method))),
        }
    }
}

/// `actBlockExpedition($expedition)`: 'all', or `board-<expedition>-<pId>`
// PHP: Actions/BlockExpedition.php:46
fn act_block_expedition(g: &mut Game, ctx: Ctx, expedition: &Value) -> R<()> {
    let _player = BlockExpedition.get_player(g, ctx)?;
    if php::loose_eq(expedition, &json!("all")) && !is_all(g, ctx) {
        return Err(PhpError::new("Invalid expedition all. Should not happen"));
    }
    if php::loose_eq(expedition, &json!("all")) {
        let mut blocks = php::arr();
        for p in players::get_all(g) {
            php::set(&mut blocks, &p.to_string(), json!(["stormLeft", "stormRight"]));
        }
        globals::set(g, "blockedExpeditions", blocks)?;
        g.notify("blockAllExpeditions", json!({"player_id": players::get_active_id(g)}));
    } else {
        let expeditions = php::explode("-", &php::strval(expedition));
        let mut blocked = globals::get(g, "blockedExpeditions")?;
        let pk = expeditions.get(2).cloned().ok_or_else(|| PhpError::new("Undefined array key 2"))?;
        let side = expeditions.get(1).cloned().ok_or_else(|| PhpError::new("Undefined array key 1"))?;
        let mut block = php::arr();
        if php::isset(&blocked, &pk) {
            block = php::get(&blocked, &pk).clone();
        }
        php::push(&mut block, json!(side));
        php::set(&mut blocked, &pk, block);
        globals::set(g, "blockedExpeditions", blocked)?;
        g.notify("blockExpedition", json!({"player_id": players::get_active_id(g), "player_id2": pk, "expedition": side}));
    }
    Ok(())
}
