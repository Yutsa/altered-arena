//! Actions/Exchange.php: swaps one Reserve card with one hand card (two DISCARD nodes inserted as
//! child).

use crate::core::node;
use crate::framework::ST_EXCHANGE;
use crate::game::{CardId, Game, PId};
use crate::helpers::{collection, ft};
use crate::managers::{cards, players};
use crate::models::action::{get_arg, get_ctx_arg, insert_as_child, param_req, ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Exchange;

/// `getTargetableCards($player, $checkTough)`: [reserve cards, hand cards]
// PHP: Actions/Exchange.php:87
fn get_targetable_cards(a: &Exchange, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<(Vec<CardId>, Vec<CardId>)> {
    let pid = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut pids = json!(players::get_all(g));
    let target_player = get_arg(a, g, ctx, "targetPlayer")?;
    if php::loose_eq(&target_player, &json!("me")) {
        pids = json!([pid]);
    }
    if php::loose_eq(&target_player, &json!("opponent")) {
        pids = php::array_diff(&pids, &json!([pid]));
    }
    let target_type = get_arg(a, g, ctx, "targetType")?;
    let reserve = cards::get_filtered(g, &pids, &json!("reserve"), &target_type, true)?;
    let hand = cards::get_filtered(g, &pids, &json!("hand"), &Value::Null, false)?;
    Ok((reserve, hand))
}

/// `argsExchange()`
// PHP: Actions/Exchange.php:109
fn args_exchange(a: &Exchange, g: &mut Game, ctx: Ctx) -> R<Value> {
    let player = players::get_active(g);
    let (reserve, hand) = get_targetable_cards(a, g, ctx, player)?;
    Ok(json!({"n": get_arg(a, g, ctx, "n")?, "reserveIds": collection::ids(&reserve), "handIds": collection::ids(&hand)}))
}

/// `actExchange($reserveId, $handId)`
// PHP: Actions/Exchange.php:121
fn act_exchange(a: &Exchange, g: &mut Game, ctx: Ctx, reserve_id: &Value, hand_id: &Value) -> R<Value> {
    let args = args_exchange(a, g, ctx)?;
    if !php::in_array(reserve_id, php::get(&args, "reserveIds")) {
        return Err(PhpError::new("You cannot target this card. Should not happen"));
    }
    if !php::in_array(hand_id, php::get(&args, "handIds")) {
        return Err(PhpError::new("You cannot target this card. Should not happen"));
    }
    cards::get(g, hand_id)?;
    cards::get(g, reserve_id)?;
    insert_as_child(
        g,
        ctx,
        &json!({"type": "seq", "childs": [
            ft::action("Discard", json!({"destination": "hand", "cardId": reserve_id}), php::arr()),
            ft::action("Discard", json!({"destination": "reserve", "cardId": hand_id}), php::arr()),
        ]}),
    )?;
    Ok(json!("exchange"))
}

impl ActionClass for Exchange {
    fn name(&self) -> &'static str {
        "Exchange"
    }

    // PHP: Actions/Exchange.php:25
    fn default_args(&self) -> Value {
        json!({
            "upTo": false,
            "targetPlayer": "me",
            "targetType": ["character", "token"],
            "maxReserveCost": 1000,
            "maxHandCost": 1000,
            "minReserveCost": 0,
            "minHandCost": 0,
            "n": 1,
            "statuses": "disabled",
            "excludeSelf": false,
            "totalCost": 1000,
            "totalMountain": 1000,
            "hasEffects": "disabled",
            "cards": [],
            "discardRemaining": false,
            "subType": "disabled",
            "expeditionAttributes": null,
            "excludeBiomes": false,
            "isTapped": false,
            "maxStatistic": 99,
        })
    }

    // PHP: Actions/Exchange.php:20
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_EXCHANGE))
    }

    // PHP: Actions/Exchange.php:48
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let target_type = get_arg(self, g, ctx, "targetType")?;
        let msg = if php::count_strict(&target_type)? == 1 && php::loose_eq(&target_type, &json!(["character"])) {
            "Exchange Character in your Reserve with a card from your Hand"
        } else {
            "Exchange card in your Reserve with a card from your Hand"
        };
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        Ok(json!({"log": msg, "args": {"n": n}}))
    }

    // PHP: Actions/Exchange.php:66
    fn is_optional(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getOptional() on null"))?;
        if php::strict_eq(php::coalesce(&node::info(g, n, "optional"), &json!(false)), &json!(true)) {
            return Ok(true);
        }
        let (reserve, hand) = get_targetable_cards(self, g, ctx, player)?;
        Ok(reserve.is_empty() || hand.is_empty())
    }

    // PHP: Actions/Exchange.php:76
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let (reserve, hand) = get_targetable_cards(self, g, ctx, player)?;
        if php::truthy(&get_arg(self, g, ctx, "upTo")?) || (!reserve.is_empty() && !hand.is_empty()) {
            return Ok(true);
        }
        // Voluntary bypass: may activate exchange with no valid pair (Pass / no-op).
        self.is_optional(g, ctx, player)
    }

    // PHP: Actions/Exchange.php:109
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_exchange(self, g, ctx)?))
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actexchange" => {
                let r = param_req(args, 0, "reserveId", "actExchange")?;
                let h = param_req(args, 1, "handId", "actExchange")?;
                act_exchange(self, g, ctx, &r, &h)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Exchange::{}()", method))),
        }
    }
}
