//! Actions/Loose.php: removes meeples of a type from a card (a `counter` becomes a boost, or a
//! UseCounter child), fires the Loose listeners.

use super::gain::{blocks_automatic_action, ctx_source, resources_to_str};
use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::ft;
use crate::managers::{cards, meeples, players};
use crate::models::action::{self, get_arg, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Loose;

/// `getCard()`
// PHP: Actions/Loose.php:72
fn get_card(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    if php::loose_eq(&card_id, &json!("me")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
        card_id = node::get_source_id(g, n);
    }
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Loose). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

/// `getLoose()`
// PHP: Actions/Loose.php:90
fn get_loose(a: &Loose, g: &Game, ctx: Ctx) -> R<(Value, Value)> {
    Ok((get_arg(a, g, ctx, "type")?, get_arg(a, g, ctx, "n")?))
}

impl ActionClass for Loose {
    fn name(&self) -> &'static str {
        "Loose"
    }

    fn default_args(&self) -> Value {
        json!({"n": 1, "upTo": false})
    }

    // PHP: Actions/Loose.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_LOOSE))
    }

    // PHP: Actions/Loose.php:21
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let p = self.get_player(g, ctx)?;
        let (ty, n) = get_loose(self, g, ctx)?;
        let desc = resources_to_str(&ty, &n);
        let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        if p == players::get_active_id(g) {
            Ok(json!({"log": "loose ${resources_desc}", "args": {"resources_desc": desc}}))
        } else {
            Ok(json!({"log": "Let ${player_name} loose ${resources_desc}", "args": {"player_name": player::get_name(g, p)?, "resources_desc": desc}}))
        }
    }

    // PHP: Actions/Loose.php:47
    fn is_automatic(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(php::loose_eq(&get_arg(self, g, ctx, "upTo")?, &json!(false)))
    }

    // PHP: Actions/Loose.php:52
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let gain = get_arg(self, g, ctx, "type")?;
        if blocks_automatic_action(g, "Loose", &gain)? {
            return Ok(false);
        }
        Ok(php::loose_eq(&get_arg(self, g, ctx, "upTo")?, &json!(false)))
    }

    // PHP: Actions/Loose.php:65
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let args = get_ctx_args(g, ctx);
        let pid = php::get(&args, "pId").clone();
        let pid = if pid.is_null() { json!(players::get_active_id(g)) } else { pid };
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/Loose.php:100
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let args = get_ctx_args(g, ctx);
        let _source = ctx_source(g, ctx)?;
        let c = get_card(g, ctx)?;
        let mut deleted: Vec<i64> = vec![];

        // Increase resource and notify
        let (mut resource, amount) = get_loose(self, g, ctx)?;
        let mut amount = amount;

        // Loose one counter
        if php::loose_eq(&resource, &json!("counter")) {
            let c = c.ok_or_else(|| PhpError::new("Error: Call to a member function countToken() on null"))?;
            if card::count_token(g, c, "boost")? > 0 {
                resource = json!("boost");
            } else {
                // we need to remove counter
                let up_to = get_arg(self, g, ctx, "upTo")?;
                action::insert_as_child(g, ctx, &ft::action("UseCounter", json!({"cardId": c, "consume": amount, "upTo": up_to}), json!({"sourceId": c})))?;
                action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
                return Ok(None);
            }
        }

        let c = c.ok_or_else(|| PhpError::new("Error: Call to a member function getOfType() on null"))?;
        let ms = card::get_of_type(g, c, &resource)?;

        for m in ms {
            if php::loose_eq(&amount, &json!(0)) {
                break;
            }
            deleted.push(m);
            amount = php_dec(&amount)?;
        }
        if !deleted.is_empty() {
            meeples::delete(g, &json!(deleted))?;
            g.notify("looseMeeples", json!({"cardId": c, "power": resource, "meeples": deleted, "n": deleted.len()}));
            let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            let tok = card::is_token(g, c)?;
            action::check_after_listeners(self, g, ctx, p, &json!({"loose": args, "token": tok}), true, None)?;
        }
        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}

/// `$amount--` (ints decrement; numeric strings become numbers; other values keep PHP 8.2's
/// behaviour: null stays null, non-numeric strings are left unchanged)
fn php_dec(v: &Value) -> R<Value> {
    Ok(match v {
        Value::Number(n) => match n.as_i64() {
            Some(i) => json!(i - 1),
            None => json!(n.as_f64().unwrap_or(0.0) - 1.0),
        },
        Value::String(s) if php::is_numeric_str(s) => {
            let f = php::floatval(v);
            if f.fract() == 0.0 && !s.contains(['.', 'e', 'E']) {
                json!(php::intval(v) - 1)
            } else {
                json!(f - 1.0)
            }
        }
        Value::Bool(_) => v.clone(),
        Value::Null => Value::Null,
        _ => v.clone(),
    })
}
