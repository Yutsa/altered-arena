//! Actions/Spend.php: spends n boosts (in priority) or counters of a card, then inserts `effect`
//! bound to that card with `updateCardId` (EFFECT placeholders preserved).

use super::gain::{ctx_source, num_op, Source};
use super::loose::php_dec;
use crate::core::{engine, node};
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::action::{self, get_arg, get_ctx_arg, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Spend;

/// `resolveSpendCardId()`: explicit args, event (EFFECT), or the first pick of the closest
/// resolved parent Target
// PHP: Actions/Spend.php:75
fn resolve_spend_card_id(g: &mut Game, ctx: Ctx) -> R<Value> {
    let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    if php::loose_eq(&card_id, &json!("me")) {
        card_id = node::get_source_id(g, n);
    } else if php::loose_eq(&card_id, &json!("EFFECT")) {
        let event = action::get_event_recursive(g, ctx)?;
        if !event.is_null() {
            card_id = php::get(&event, "cardId").clone();
            if card_id.is_null() {
                card_id = php::get(php::get(&event, "gain"), "cardId").clone();
            }
        }
    }

    if !card_id.is_null() {
        return Ok(card_id);
    }

    let mut cur = node::parent(g, n);
    while let Some(c) = cur {
        if php::strict_eq(&node::get_action(g, c), &json!("Target")) && node::is_action_resolved(g, c) {
            let resolved = node::get_action_resolution_args(g, c);
            if php::is_arr(&resolved) && !php::empty(&resolved) {
                let pick = php::geti(&resolved, 0);
                let id = if php::is_arr(pick) { php::geti(pick, 0).clone() } else { pick.clone() };
                if !id.is_null() {
                    return Ok(id);
                }
            }
        }
        cur = node::parent(g, c);
    }
    Ok(Value::Null)
}

/// `getCard()`
// PHP: Actions/Spend.php:62
fn get_card(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let card_id = resolve_spend_card_id(g, ctx)?;
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Spend). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

fn card_req(c: Option<CardId>, method: &str) -> R<CardId> {
    c.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `Spend::hasEnoughToSpend($card, $n)`: boosts first, numeric counters only without boosts
// PHP: Actions/Spend.php:127
pub fn has_enough_to_spend(g: &mut Game, c: CardId, n: &Value) -> R<bool> {
    if card::count_token(g, c, "boost")? > 0 {
        return Ok(php::ge(&json!(card::count_token(g, c, "boost")?), n));
    }
    let data = card::get(g, c, "extraDatas")?;
    Ok(php::ge(php::coalesce(php::get(&data, "counter"), &json!(0)), n))
}

/// `Engine::buildTree($this->getCtxArg('effect'))->getDescription()`
fn effect_desc(g: &mut Game, ctx: Ctx) -> R<Value> {
    let effect = get_ctx_arg(g, ctx, "effect");
    let t = engine::build_tree(g, &effect)?;
    node::get_description(g, t)
}

/// `actSpend($n)`
// PHP: Actions/Spend.php:148
fn act_spend(a: &Spend, g: &mut Game, ctx: Ctx, n: &Value) -> R<()> {
    let player = players::get_active(g);
    let _source = ctx_source(g, ctx)?;
    let c = card_req(get_card(g, ctx)?, "countToken")?;
    let mut amount = n.clone();
    // Boost tokens take priority over numeric counters when both are present.
    if card::count_token(g, c, "boost")? > 0 {
        let mut deleted: Vec<i64> = vec![];
        for m in card::get_of_type(g, c, &json!("boost"))? {
            if php::loose_eq(&amount, &json!(0)) {
                break;
            }
            deleted.push(m);
            amount = php_dec(&amount)?;
        }
        if !deleted.is_empty() {
            meeples::delete(g, &json!(deleted))?;
            g.notify("looseMeeples", json!({"cardId": c, "power": "boost", "meeples": deleted, "n": deleted.len()}));
        }
    } else {
        // we consume a counter
        let mut extra = card::get(g, c, "extraDatas")?;
        if !php::isset(&extra, "counter") || php::loose_eq(php::get(&extra, "counter"), &json!(0)) {
            return Err(PhpError::new("No counter to spend"));
        }
        if php::lt(php::coalesce(php::get(&extra, "counter"), &json!(0)), &amount) {
            return Err(PhpError::new("Cannot consume counter. Should not happen"));
        }
        let left = num_op(php::get(&extra, "counter"), &amount, "-")?;
        php::set(&mut extra, "counter", left.clone());
        card::set_property(g, c, "extraDatas", extra, true)?;

        // Notifications::spendCounter($player, $card, $amount, $this->getSource())
        let source = ctx_source(g, ctx)?;
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getTotalMana() on null"))?;
        let total = player::get_total_mana(g, p)?;
        let mana = player::get_mana(g, p)?;
        let src = match source {
            Source::Card(s) => json!(s),
            _ => Value::Null,
        };
        g.notify("useCounter", json!({"player_id": p, "consume": amount, "cardId": c, "value": left, "decrease": amount, "totalMana": total, "mana": mana, "source": src}));
    }

    let effect = get_arg(a, g, ctx, "effect")?;
    if !effect.is_null() {
        // Keep GAIN(EFFECT)/etc. referring to the trigger event card, not the card boosts were spent from.
        let loc = card::location(g, c)?;
        let sid = action::get_source_id(g, ctx)?;
        let owner = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let mut effect = action::update_card_id(g, &effect, &json!(c), &json!(loc), &sid, &json!(owner), true)?;
        if php::loose_eq(&get_arg(a, g, ctx, "updateN")?, &json!(true)) {
            php::set(php::get_mut(&mut effect, "args"), "n", amount);
        }
        action::insert_as_child(g, ctx, &effect)?;
    }
    action::resolve_action(a, g, ctx, Value::Null, Value::Null)
}

impl ActionClass for Spend {
    fn name(&self) -> &'static str {
        "Spend"
    }

    fn default_args(&self) -> Value {
        json!({"n": 1, "effect": [], "automatic": true, "updateN": false})
    }

    // PHP: Actions/Spend.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPEND))
    }

    // PHP: Actions/Spend.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        let desc = effect_desc(g, ctx)?;
        Ok(json!({"log": "Spend ${n} to ${effect_desc}", "args": {"n": n, "effect_desc": desc, "i18n": ["effect_desc"]}}))
    }

    // PHP: Actions/Spend.php:39
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Spend.php:135
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let c = card_req(get_card(g, ctx)?, "countToken")?;
        let n = get_arg(self, g, ctx, "n")?;
        has_enough_to_spend(g, c, &n)
    }

    // PHP: Actions/Spend.php:113
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let source = ctx_source(g, ctx)?.to_value();
        let manual = php::loose_eq(&get_arg(self, g, ctx, "automatic")?, &json!(false));
        let n = if manual {
            let c = card_req(get_card(g, ctx)?, "countCounters")?;
            json!(card::count_counters(g, c)?)
        } else {
            get_arg(self, g, ctx, "n")?
        };
        let suffix = if php::loose_eq(&get_arg(self, g, ctx, "automatic")?, &json!(false)) { "choice" } else { "" };
        let desc = effect_desc(g, ctx)?;
        Ok(Some(json!({"source": source, "n": n, "descSuffix": suffix, "effect_desc": desc})))
    }

    // PHP: Actions/Spend.php:141
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        if php::strict_eq(&get_arg(self, g, ctx, "automatic")?, &json!(true)) {
            let n = get_arg(self, g, ctx, "n")?;
            act_spend(self, g, ctx, &n)?;
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            // PHP: Actions/Spend.php:148
            "actspend" => {
                let n = action::param_req(args, 0, "n", "ALT\\Actions\\Spend::actSpend")?;
                act_spend(self, g, ctx, &n)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Spend::{}()", method))),
        }
    }
}
