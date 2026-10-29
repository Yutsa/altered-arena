//! Actions/UseCounter.php: consume counters of the source card, possibly paying mana.

use crate::framework::ST_USE_COUNTER;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{get_arg, get_source, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct UseCounter;

impl ActionClass for UseCounter {
    fn name(&self) -> &'static str {
        "UseCounter"
    }

    // PHP: Actions/UseCounter.php:39
    fn default_args(&self) -> Value {
        json!({"pay": 0, "consume": 99, "upTo": false})
    }

    // PHP: Actions/UseCounter.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_USE_COUNTER))
    }

    // PHP: Actions/UseCounter.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let consume = get_arg(self, g, ctx, "consume")?;
        let pay = get_arg(self, g, ctx, "pay")?;
        let log = if php::loose_eq(&pay, &json!(0)) { "Use ${consume}{COUNTER}" } else { "Pay ${mana_cost} and use ${consume}{COUNTER}" };
        Ok(json!({"log": log, "args": {"mana_cost": pay, "consume": consume}}))
    }

    // PHP: Actions/UseCounter.php:41
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let pay = get_arg(self, g, ctx, "pay")?;
        Ok(php::le(&pay, &json!(player::get_mana(g, p)?)))
    }

    // PHP: Actions/UseCounter.php:46
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let c = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getExtraDatas() on null"))?;
        let mut extra = card::get(g, c, "extraDatas")?;
        let cost = get_arg(self, g, ctx, "pay")?;
        let consume = get_arg(self, g, ctx, "consume")?;
        if php::lt(&json!(player::get_mana(g, p)?), &cost) {
            return Err(PhpError::new("Cannot pay cost of use Counter. Should not happen"));
        }

        if php::gt(&cost, &json!(0)) {
            player::pay_mana(g, p, php::intval(&cost))?;
        }
        if php::lt(&consume, &json!(0)) {
            return Err(PhpError::new("Consume is negative, should not happen"));
        }
        let counter = php::coalesce(php::get(&extra, "counter"), &json!(0)).clone();
        if php::loose_eq(&get_arg(self, g, ctx, "upTo")?, &json!(false)) && php::lt(&counter, &consume) {
            return Err(PhpError::new("Cannot consume counter. Should not happen"));
        }

        let after = php::intval(php::get(&extra, "counter")) - php::intval(&consume);
        php::set(&mut extra, "counter", json!(after));
        let after = php::intval(php::coalesce(php::get(&extra, "counter"), &json!(0))).max(0);
        php::set(&mut extra, "counter", json!(after));
        card::set_property(g, c, "extraDatas", extra, true)?;

        g.notify("useCounter", json!({"player_id": p, "cardId": c, "consume": consume, "mana_cost": cost}));
        resolve_action(self, g, ctx, json!([cost]), json!(false))?;
        Ok(None)
    }
}
