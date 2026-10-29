//! Actions/AfterYou.php: "After You" (pass the turn without ending the Afternoon), possibly paying
//! mana.

use crate::core::globals;
use crate::framework::ST_AFTER_YOU;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{get_ctx_arg, get_source, resolve_action, ActionClass, Ctx};
use crate::models::player;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct AfterYou;

/// `getCost($player)` (null when the condition is unknown)
// PHP: Actions/AfterYou.php:28
fn get_cost(g: &mut Game, ctx: Ctx, p: PId) -> R<Value> {
    let pay = get_ctx_arg(g, ctx, "pay");
    let condition = get_ctx_arg(g, ctx, "condition");
    if php::loose_eq(&pay, &json!(0)) {
        return Ok(pay);
    }

    if php::loose_eq(&condition, &json!("none")) {
        return Ok(pay);
    }

    let first = globals::get_int(g, "firstPlayer")?;
    if php::loose_eq(&condition, &json!("notFirstPlayer")) && first != p {
        return Ok(pay);
    } else if php::loose_eq(&condition, &json!("notFirstPlayer")) && first == p {
        return Ok(json!(0));
    }
    Ok(Value::Null)
}

impl ActionClass for AfterYou {
    fn name(&self) -> &'static str {
        "AfterYou"
    }

    // PHP: Actions/AfterYou.php:21
    fn default_args(&self) -> Value {
        json!({"pay": 0, "condition": "none"})
    }

    // PHP: Actions/AfterYou.php:11
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_AFTER_YOU))
    }

    // PHP: Actions/AfterYou.php:16
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Wait next turn"))
    }

    // PHP: Actions/AfterYou.php:23
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let mana = player::get_mana(g, p)?;
        let cost = get_cost(g, ctx, p)?;
        Ok(php::ge(&json!(mana), &cost))
    }

    // PHP: Actions/AfterYou.php:47
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let cost = get_cost(g, ctx, p)?;
        if php::lt(&json!(player::get_mana(g, p)?), &cost) {
            return Err(PhpError::new("Cannot pay cost of After You. Should not happen"));
        }

        if php::gt(&cost, &json!(0)) {
            player::pay_mana(g, p, php::intval(&cost))?;
        }

        globals::inc(g, "playedCards", 1)?;
        let source = get_source(g, ctx)?;
        g.notify("afterYou", json!({"player_id": p, "mana_cost": cost, "card": source}));
        resolve_action(self, g, ctx, json!([cost]), json!(false))?;
        Ok(None)
    }
}
