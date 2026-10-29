//! Actions/Exhaust.php: exhausts a card in play or in Reserve (an effect, not a cost), fires the
//! Exhaust listeners.

use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::cards;
use crate::models::action::{self, get_ctx_arg, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Exhaust;

/// `getCard()`
// PHP: Actions/Exhaust.php:29
fn get_card(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
    if php::loose_eq(&card_id, &json!("me")) {
        card_id = node::get_source_id(g, n);
    } else if php::loose_eq(&card_id, &json!("EFFECT")) {
        card_id = php::get(php::get(&node::to_array(g, n), "event"), "cardId").clone();
    }
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Gain). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

impl ActionClass for Exhaust {
    fn name(&self) -> &'static str {
        "Exhaust"
    }

    // PHP: Actions/Exhaust.php:14
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_EXHAUST))
    }

    // PHP: Actions/Exhaust.php:19
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("{T}"))
    }

    // PHP: Actions/Exhaust.php:24
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let c = get_card(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function isTapped() on null"))?;
        Ok(!card::is_tapped(g, c)?)
    }

    // PHP: Actions/Exhaust.php:44
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let c = get_card(g, ctx)?;
        // if the card was a token or has been discarded / put to hand
        let c = match c {
            Some(c) if ["stormLeft", "stormRight", "landmark", "reserve"].contains(&card::location(g, c)?.as_str()) => c,
            _ => {
                action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
                return Ok(None);
            }
        };
        if card::is_tapped(g, c)? {
            return Err(PhpError::new("Card is already tapped. Should not happen"));
        }
        card::set_property(g, c, "tapped", json!(true), true)?;

        let source = action::get_source(g, ctx)?;
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getTotalMana() on null"))?;
        let total = player::get_total_mana(g, p)?;
        let mana = player::get_mana(g, p)?;
        g.notify("tap", json!({"player_id": p, "cardId": c, "card2": source, "totalMana": total, "mana": mana}));
        // Check listener
        let loc = card::location(g, c)?;
        let sid = action::get_source_id(g, ctx)?;
        let tok = card::is_token(g, c)?;
        action::check_after_listeners(self, g, ctx, p, &json!({"cardId": c, "cardLocation": loc, "sourceId": sid, "token": tok}), true, None)?;

        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
