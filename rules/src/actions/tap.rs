//! Actions/Tap.php: exhausts a card as the cost of its {T} ability (with an optional mana
//! `pay`), records the ability activation, fires the Exhaust listeners.

use crate::core::globals;
use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::cards;
use crate::models::action::{self, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Tap;

/// `getCard()`
// PHP: Actions/Tap.php:34
fn get_card(g: &mut Game, ctx: Ctx) -> R<CardId> {
    let args = get_ctx_args(g, ctx);
    let mut card_id = php::get(&args, "cardId").clone();
    if card_id.is_null() {
        card_id = action::get_source_id(g, ctx)?;
    }
    if php::loose_eq(&card_id, &json!("me")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
        card_id = node::get_source_id(g, n);
    }
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (tap). Should not happen"));
    }
    cards::get(g, &card_id)
}

/// The ability activation bookkeeping of a `{T}` / `{D}` ability (`abilityActivatedThisTurn`,
/// `...Count`, `...TypeCount` for the player)
// PHP: Actions/Tap.php:62-74
pub fn record_ability_activated(g: &mut Game, pid: PId, kind: &str) -> R<()> {
    let key = pid.to_string();
    let mut activated = globals::get(g, "abilityActivatedThisTurn")?;
    let merged = php::array_merge(&[php::coalesce(php::get(&activated, &key), &php::arr()), &php::map(vec![(kind, json!(true))])]);
    php::set(&mut activated, &key, merged);
    globals::set(g, "abilityActivatedThisTurn", activated)?;

    let mut count = globals::get(g, "abilityActivatedThisTurnCount")?;
    let n = php::intval(php::coalesce(php::get(&count, &key), &json!(0))) + 1;
    php::set(&mut count, &key, json!(n));
    globals::set(g, "abilityActivatedThisTurnCount", count)?;

    let mut type_count = globals::get(g, "abilityActivatedThisTurnTypeCount")?;
    let mine = php::coalesce(php::get(&type_count, &key), &php::arr()).clone();
    php::set(&mut type_count, &key, mine);
    let n = php::intval(php::coalesce(php::get(php::get(&type_count, &key), kind), &json!(0))) + 1;
    php::set(php::get_mut(&mut type_count, &key), kind, json!(n));
    globals::set(g, "abilityActivatedThisTurnTypeCount", type_count)?;
    Ok(())
}

impl ActionClass for Tap {
    fn name(&self) -> &'static str {
        "Tap"
    }

    // PHP: Actions/Tap.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TAP))
    }

    // PHP: Actions/Tap.php:20
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let description = get_ctx_arg(g, ctx, "description");
        if !description.is_null() {
            return Ok(description);
        }
        Ok(json!("Exhaust this card ({T})"))
    }

    // PHP: Actions/Tap.php:29
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let c = get_card(g, ctx)?;
        if card::is_tapped(g, c)? {
            return Ok(false);
        }
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
        let mana = player::get_mana(g, p)?;
        let pay = php::coalesce(&get_ctx_arg(g, ctx, "pay"), &json!(0)).clone();
        Ok(php::ge(&json!(mana), &pay))
    }

    // PHP: Actions/Tap.php:47
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let c = get_card(g, ctx)?;

        let pay = php::coalesce(&get_ctx_arg(g, ctx, "pay"), &json!(0)).clone();
        if php::gt(&pay, &json!(0)) {
            let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function payMana() on null"))?;
            player::pay_mana(g, p, php::intval(&pay))?;
        }

        if card::is_tapped(g, c)? {
            return Err(PhpError::new("Card is already tapped. Should not happen"));
        }
        card::set_property(g, c, "tapped", json!(true), true)?;
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getTotalMana() on null"))?;
        let total = player::get_total_mana(g, p)?;
        let mana = player::get_mana(g, p)?;
        g.notify("tap", json!({"player_id": p, "cardId": c, "mana_cost": pay, "totalMana": total, "mana": mana}));
        record_ability_activated(g, p, "tap")?;
        // Check listener
        let loc = card::location(g, c)?;
        let sid = action::get_source_id(g, ctx)?;
        let tok = card::is_token(g, c)?;
        action::check_after_listeners(self, g, ctx, p, &json!({"cardId": c, "cardLocation": loc, "sourceId": sid, "token": tok}), true, Some("Exhaust"))?;

        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
