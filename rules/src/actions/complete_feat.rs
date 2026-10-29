//! Actions/CompleteFeat.php: puts one `featCompleted` meeple on the target Feat (once).

use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::action::{self, get_ctx_args, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct CompleteFeat;

/// `getTargetCard()`: `cardId` (ME / 'source' = the source, EFFECT = the event card)
// PHP: Actions/CompleteFeat.php:41
fn get_target_card(g: &mut Game, ctx: Ctx) -> R<CardId> {
    let args = get_ctx_args(g, ctx);
    let mut card_id = php::get(&args, "cardId").clone();
    if php::strict_eq(&card_id, &json!("me")) || php::strict_eq(&card_id, &json!("source")) {
        card_id = action::get_source_id(g, ctx)?;
    }
    if php::strict_eq(&card_id, &json!("EFFECT")) {
        let e = php::get(&action::get_event(g, ctx)?, "cardId").clone();
        card_id = if e.is_null() { action::get_source_id(g, ctx)? } else { e };
    }
    if card_id.is_null() {
        return Err(PhpError::new("CompleteFeat: missing cardId"));
    }
    cards::get(g, &card_id)
}

impl ActionClass for CompleteFeat {
    fn name(&self) -> &'static str {
        "CompleteFeat"
    }

    // PHP: Actions/CompleteFeat.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_COMPLETE_FEAT))
    }

    // PHP: Actions/CompleteFeat.php:17
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Complete Feat"))
    }

    // PHP: Actions/CompleteFeat.php:27
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/CompleteFeat.php:63
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let c = get_target_card(g, ctx)?;
        if meeples::count_meeples(g, &format!("card-{}", c), &json!("featCompleted"))? >= 1 {
            action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
            return Ok(None);
        }

        let pid = card::pid(g, c)?;
        let p = players::get(g, pid).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let m = meeples::single_create(g, &json!({"type": "featCompleted", "location": format!("card-{}", c), "player_id": p}))?;

        g.notify("addMeeples", json!({"player_id": p, "cardId": c, "meeples": [m]}));
        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
