//! Actions/BoostExchange.php: swaps the boosts of two cards, firing the Gain listeners on the
//! card that ends up with more.

use super::gain::{ctx_source, Source};
use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::action::{self, get_arg, get_ctx_arg, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct BoostExchange;

/// `isResolvedCardId($cid)`: a numeric scalar, not a placeholder
// PHP: Actions/BoostExchange.php:145
fn is_resolved_card_id(cid: &Value) -> bool {
    !cid.is_null() && !php::strict_eq(cid, &json!("EFFECT")) && !php::strict_eq(cid, &json!("me")) && !php::is_arr(cid) && php::is_numeric(cid)
}

/// `findPreviousTargetCardId($excludeId)`: the first ancestor whose args hold another card id
// PHP: Actions/BoostExchange.php:130
fn find_previous_target_card_id(g: &Game, ctx: Ctx, exclude_id: &Value) -> Value {
    let mut parent = ctx.and_then(|n| node::parent(g, n));
    while let Some(p) = parent {
        let args = node::get_args(g, p);
        let cid = php::get(&args, "cardId").clone();
        if is_resolved_card_id(&cid) && php::loose_ne(&cid, exclude_id) {
            return cid;
        }
        parent = node::parent(g, p);
    }
    Value::Null
}

/// `resolveSwapIds($allIds)`: two ids given at once (Sap Duende), or the picked id and the
/// previous pick / the source (nested TARGET)
// PHP: Actions/BoostExchange.php:101
fn resolve_swap_ids(g: &Game, ctx: Ctx, all_ids: &Value) -> R<Option<Vec<Value>>> {
    let mut all_ids = all_ids.clone();
    if php::is_arr(&all_ids) {
        let ids: Vec<Value> = php::vals(&all_ids).into_iter().cloned().collect();
        if ids.len() == 2 {
            return Ok(Some(ids));
        }
        if ids.len() != 1 {
            return Ok(None);
        }
        all_ids = ids[0].clone();
    }

    if !is_resolved_card_id(&all_ids) {
        return Ok(None);
    }

    let picked = all_ids;
    let mut other = find_previous_target_card_id(g, ctx, &picked);
    if other.is_null() {
        other = action::get_source_id(g, ctx)?;
    }
    if !is_resolved_card_id(&other) || php::loose_eq(&other, &picked) {
        return Ok(None);
    }
    Ok(Some(vec![picked, other]))
}

impl ActionClass for BoostExchange {
    fn name(&self) -> &'static str {
        "BoostExchange"
    }

    fn default_args(&self) -> Value {
        json!({"cardId": null})
    }

    // PHP: Actions/BoostExchange.php:14
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_BOOST_EXCHANGE))
    }

    // PHP: Actions/BoostExchange.php:19
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Exchange boosts between 2 cards"))
    }

    // PHP: Actions/BoostExchange.php:24
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/BoostExchange.php:29
    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/BoostExchange.php:34
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let pid = get_ctx_arg(g, ctx, "pId");
        let pid = if pid.is_null() { json!(players::get_active_id(g)) } else { pid };
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/BoostExchange.php:44
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let arg = get_arg(self, g, ctx, "cardId")?;
        let Some(all_ids) = resolve_swap_ids(g, ctx, &arg)? else {
            return Err(PhpError::new("No Ids to switch in boostExchange, should not happen"));
        };

        let source = ctx_source(g, ctx)?;
        let source_id = action::get_source_id(g, ctx)?;
        let (card1, card2) = (all_ids[0].clone(), all_ids[1].clone());

        // `$allCards = Cards::get($allIds)`, then indexed by id
        let all_cards = match cards::get_raw(g, &json!(all_ids), true)? {
            cards::Got::Many(v) => v,
            cards::Got::One(_) => return Err(PhpError::new("Error: Cannot use object of type ALT\\Models\\Card as array")),
        };
        let pick = |id: &Value| -> R<CardId> {
            all_cards
                .iter()
                .copied()
                .find(|c| php::loose_eq(&json!(c), id))
                .ok_or_else(|| PhpError::new("Error: Call to a member function countToken() on null"))
        };
        let c1 = pick(&card1)?;
        let c2 = pick(&card2)?;
        // $existingBoost[$card2] = boosts of card1, $existingBoost[$card1] = boosts of card2
        let boost_for_c2 = card::count_token(g, c1, "boost")? as i64;
        let boost_for_c1 = card::count_token(g, c2, "boost")? as i64;

        let meeples1 = card::get_of_type(g, c1, &json!("boost"))?;
        let meeples2 = card::get_of_type(g, c2, &json!("boost"))?;

        let p = |player: Option<PId>| player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"));
        // Notifications::gainMeeple: `card2` must be a card (or null)
        let src = |source: &Source| -> R<Value> {
            match source {
                Source::Other(v) => Err(PhpError::new(format!("Error: Call to a member function getId() on {}", php::type_name(v)))),
                s => Ok(s.to_value()),
            }
        };

        if !meeples1.is_empty() {
            meeples::delete(g, &json!(meeples1))?;
            g.notify("looseMeeples", json!({"cardId": c1, "power": "boost", "meeples": meeples1, "n": meeples1.len()}));
        }
        if boost_for_c2 > 0 {
            let owner = p(card::get_player(g, c2)?)?;
            let tokens = meeples::create_on_card(g, "boost", c2, &json!(owner), boost_for_c2)?;
            g.notify("addMeeples", json!({"cardId": c2, "power": "boost", "meeples": tokens, "card2": src(&source)?, "n": tokens.len()}));
            if boost_for_c2 > boost_for_c1 {
                let pl = p(player)?;
                let args = json!({"gain": {"cardId": c2, "type": "boost", "n": boost_for_c2}, "token": card::is_token(g, c2)?, "cardId": c2, "location": card::location(g, c2)?, "initialBoost": boost_for_c1, "cardType": card::get(g, c2, "type")?, "additionalType": card::get_additional_type(g, c2)?, "sourceId": source_id});
                action::check_after_listeners(self, g, ctx, pl, &args, true, Some("Gain"))?;
            }
        }

        if !meeples2.is_empty() {
            meeples::delete(g, &json!(meeples2))?;
            g.notify("looseMeeples", json!({"cardId": c2, "power": "boost", "meeples": meeples2, "n": meeples2.len()}));
        }
        if boost_for_c1 > 0 {
            let owner = p(card::get_player(g, c1)?)?;
            let tokens = meeples::create_on_card(g, "boost", c1, &json!(owner), boost_for_c1)?;
            g.notify("addMeeples", json!({"cardId": c1, "power": "boost", "meeples": tokens, "card2": src(&source)?, "n": tokens.len()}));
            if boost_for_c1 > boost_for_c2 {
                let pl = p(player)?;
                let args = json!({"gain": {"cardId": c1, "type": "boost", "n": boost_for_c1}, "token": card::is_token(g, c1)?, "cardId": c1, "location": card::location(g, c1)?, "initialBoost": boost_for_c2, "cardType": card::get(g, c1, "type")?, "additionalType": card::get_additional_type(g, c1)?, "sourceId": source_id});
                action::check_after_listeners(self, g, ctx, pl, &args, true, Some("Gain"))?;
            }
        }

        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
