//! Actions/Ready.php: readies a card (or the first tapped mana orb of the player).

use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::cards;
use crate::models::action::{self, get_ctx_arg, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Ready;

/// `getCard()`
// PHP: Actions/Ready.php:40
fn get_card(a: &Ready, g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    if php::loose_eq(&card_id, &json!("me")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
        card_id = node::get_source_id(g, n);
    } else if php::loose_eq(&card_id, &json!("EFFECT")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))?;
        card_id = php::get(php::get(&node::to_array(g, n), "event"), "cardId").clone();
    } else if php::loose_eq(&card_id, &json!("mana")) {
        let p = a.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getManaCards() on null"))?;
        return Ok(player::get_mana_cards(g, p, Some(true))?.first().copied());
    }
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Ready). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

impl ActionClass for Ready {
    fn name(&self) -> &'static str {
        "Ready"
    }

    // PHP: Actions/Ready.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_READY))
    }

    // PHP: Actions/Ready.php:20
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        if php::loose_eq(&get_ctx_arg(g, ctx, "cardId"), &json!("mana")) {
            Ok(json!("Ready a mana orb"))
        } else {
            Ok(json!("Ready the card"))
        }
    }

    // PHP: Actions/Ready.php:29 (returns true before the commented-out checks)
    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Ready.php:57
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let c = get_card(self, g, ctx)?;

        if let Some(c) = c {
            if !card::is_tapped(g, c)? && get_ctx_arg(g, ctx, "optionalExhaust").is_null() {
                return Err(PhpError::new("Card is not tapped. Should not happen"));
            }
            card::set_property(g, c, "tapped", json!(false), true)?;
            let source = action::get_source(g, ctx)?;
            // Notifications::readyEffect
            if card::location(g, c)? == "mana" {
                let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getMana() on null"))?;
                let mana = player::get_mana(g, p)?;
                g.notify("publicReadyMana", json!({"player_id": p, "card2": source, "mana": mana}));
                let total = player::get_total_mana(g, p)?;
                let mana = player::get_mana(g, p)?;
                g.notify("privateReadyMana", json!({"player_id": p, "cardId": c, "card2": source, "totalMana": total, "mana": mana}));
            } else {
                g.notify("ready", json!({"player_id": player, "cardId": c, "card2": source}));
            }
        }
        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
