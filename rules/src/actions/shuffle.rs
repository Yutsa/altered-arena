//! Actions/Shuffle.php: reforms the active player's deck from the discard pile.

use crate::framework::ST_SHUFFLE;
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::models::action::{resolve_action, ActionClass, Ctx};
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Shuffle;

impl ActionClass for Shuffle {
    fn name(&self) -> &'static str {
        "Shuffle"
    }

    // PHP: Actions/Shuffle.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SHUFFLE))
    }

    // PHP: Actions/Shuffle.php:20
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Shuffle the discard pile"))
    }

    // PHP: Actions/Shuffle.php:25
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    // PHP: Actions/Shuffle.php:30
    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    // PHP: Actions/Shuffle.php:35
    fn is_optional(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Shuffle.php:40
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Shuffle.php:47
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        cards::reform_deck_from_discard(g, &format!("deck-{}", player))?;
        resolve_action(self, g, ctx, Value::Null, json!(true))?;
        Ok(None)
    }
}
