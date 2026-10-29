//! Actions/Pay.php: pay mana (a fixed amount, or X chosen by the player).

use crate::framework::ST_PAY;
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::models::action::{get_arg, get_ctx_arg, param_req, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Pay;

/// `is_int($v) && $v > 0`
fn positive_int(v: &Value) -> bool {
    v.is_i64() && php::gt(v, &json!(0))
}

/// `actPay($mana)`
// PHP: Actions/Pay.php:92
fn act_pay(a: &Pay, g: &mut Game, ctx: Ctx, mana: &Value) -> R<()> {
    let p = a.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function payMana() on null"))?;

    if php::gt(mana, &json!(0)) {
        player::pay_mana(g, p, php::intval(mana))?;
        g.notify("pay", json!({"player_id": p, "mana_cost": mana}));
    }

    resolve_action(a, g, ctx, json!([mana]), json!(false))
}

impl ActionClass for Pay {
    fn name(&self) -> &'static str {
        "Pay"
    }

    // PHP: Actions/Pay.php:19
    fn default_args(&self) -> Value {
        json!({"X": 1, "maximum": 1000})
    }

    // PHP: Actions/Pay.php:14
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_PAY))
    }

    // PHP: Actions/Pay.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let pay = php::coalesce(&get_ctx_arg(g, ctx, "pay"), &json!(0)).clone();
        Ok(json!({"log": "Pay ${mana_cost}", "args": {"mana_cost": pay}}))
    }

    // PHP: Actions/Pay.php:35
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        if player::get_mana(g, p)? == 0 {
            return Ok(false);
        }

        if php::loose_eq(&get_arg(self, g, ctx, "pay")?, &json!("X")) {
            return Ok(true);
        }
        let pay = php::coalesce(&get_ctx_arg(g, ctx, "pay"), &json!(0)).clone();
        Ok(php::ge(&json!(player::get_mana(g, p)?), &pay))
    }

    // PHP: Actions/Pay.php:47
    fn is_mandatory(&self, _g: &mut Game, _ctx: Ctx) -> R<bool> {
        // we must do it to get the effect
        Ok(true)
    }

    // PHP: Actions/Pay.php:53
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let pay = php::coalesce(&get_arg(self, g, ctx, "pay")?, &json!(0)).clone();
        let mut maximum = get_arg(self, g, ctx, "maximum")?;
        if positive_int(&pay) {
            return Ok(Some(php::arr()));
        }

        if php::loose_eq(&maximum, &json!("exhaustedReserve")) || php::loose_eq(&maximum, &json!("exhaustedReserveAndPermanent")) {
            let mut exhausted: i64 = 0;
            for pp in players::get_all(g) {
                for c in player::get_reserve_cards(g, pp)? {
                    if card::is_tapped(g, c)? {
                        exhausted += 1;
                    }
                }
            }

            if php::loose_eq(&maximum, &json!("exhaustedReserveAndPermanent")) {
                let mut n: i64 = 0;
                for c in cards::get_played_cards(g, &json!(p), &Value::Null)? {
                    let ty = card::get(g, c, "type")?;
                    let perm = php::loose_eq(&ty, &json!("permanent")) || php::in_array(&json!("permanent"), &card::get(g, c, "additionalType")?);
                    if perm && card::is_tapped(g, c)? {
                        n += 1;
                    }
                }
                // exclude the card itself
                exhausted += n - 1;
            }
            maximum = json!(exhausted);
        }

        let mana = player::get_mana(g, p)?;
        Ok(Some(json!({
            "pay": get_ctx_arg(g, ctx, "pay"),
            "mana": mana,
            "maximum": maximum,
        })))
    }

    // PHP: Actions/Pay.php:85
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let pay = get_ctx_arg(g, ctx, "pay");
        if positive_int(&pay) {
            act_pay(self, g, ctx, &pay)?;
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actpay" => {
                let mana = param_req(args, 0, "mana", "actPay")?;
                act_pay(self, g, ctx, &mana)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Pay::{}()", method))),
        }
    }
}
