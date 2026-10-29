//! Actions/EndAfternoon.php: the player passes and ends their Afternoon.

use crate::core::globals;
use crate::framework::ST_END_AFTERNOON;
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::models::action::{insert_as_child, resolve_action, ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct EndAfternoon;

impl ActionClass for EndAfternoon {
    fn name(&self) -> &'static str {
        "EndAfternoon"
    }

    // PHP: Actions/EndAfternoon.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_END_AFTERNOON))
    }

    // PHP: Actions/EndAfternoon.php:17
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!({"log": "End your Afternoon", "args": []}))
    }

    // isOptional (:25) is commented out in PHP: the base class applies

    // PHP: Actions/EndAfternoon.php:30
    fn is_doable(&self, g: &mut Game, _ctx: Ctx, player: Option<PId>) -> R<bool> {
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        Ok(globals::is(g, "dayPhase")? && !php::in_array(&json!(p), &globals::get(g, "skippedPlayers")?))
    }

    // PHP: Actions/EndAfternoon.php:35
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let mut skipped = globals::get(g, "skippedPlayers")?;
        if php::empty(&skipped) {
            globals::set(g, "firstPass", json!(p))?;
        }
        php::push(&mut skipped, json!(p));
        globals::set(g, "skippedPlayers", skipped)?;
        g.notify("passTurn", json!({"player_id": p}));

        // Insert EndTurn reactions as children of this Pass (not root AFTER_FINISHING), so they
        // resolve before remaining parallel siblings (e.g. unique TARGET after 775).
        let base = json!({"pId": p, "type": "action", "action": "EndTurn", "method": "EndTurn"});
        let event = php::array_merge(&[&base, &json!(["pass"])]);
        let reaction = cards::get_reaction(g, &event, true, true)?;
        if !reaction.is_null() {
            // When multiple "when you pass" effects trigger simultaneously, let the player choose
            // the order per game rules
            let n = php::count(&reaction);
            insert_as_child(g, ctx, &json!({"type": "parallel", "noIndependent": n > 1, "childs": reaction}))?;
        }

        resolve_action(self, g, ctx, php::arr(), json!(false))?;
        Ok(None)
    }
}
