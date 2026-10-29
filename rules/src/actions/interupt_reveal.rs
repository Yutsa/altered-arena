//! Actions/InteruptReveal.php: the revealed cards (in limbo) go back to the hand once seen.
//! Its `argsMarkRegion()` is not `argsInteruptReveal()`: the state has no args.

use crate::framework::*;
use crate::game::Game;
use crate::managers::cards;
use crate::models::action::{ActionClass, Ctx};
use crate::models::card;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct InteruptReveal;

impl ActionClass for InteruptReveal {
    fn name(&self) -> &'static str {
        "InteruptReveal"
    }

    // PHP: Actions/InteruptReveal.php:27
    fn default_args(&self) -> Value {
        json!({"create": false, "regionType": null})
    }

    // PHP: Actions/InteruptReveal.php:17
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_INTERRUPT_REVEAL))
    }

    // PHP: Actions/InteruptReveal.php:22
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Confirm display of card"))
    }

    fn act(&self, g: &mut Game, _ctx: Ctx, method: &str, _args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actinteruptreveal" => {
                act_interupt_reveal(g)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\InteruptReveal::{}()", method))),
        }
    }
}

/// `actInteruptReveal()`
// PHP: Actions/InteruptReveal.php:39
fn act_interupt_reveal(g: &mut Game) -> R<()> {
    let revealed = cards::get_in_location(g, "limbo", None)?;
    let mut player = None;
    for c in &revealed {
        card::set_location(g, *c, "hand")?;
        player = card::get_player(g, *c)?;
    }
    // Notifications::endReveal: `$player->getId()` (undefined when nothing was revealed)
    let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    g.notify("endReveal", json!({"player_id": p, "cardsRevealed": revealed}));
    Ok(())
}
