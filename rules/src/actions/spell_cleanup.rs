//! Actions/SpellCleanup.php: after a spell resolved, it leaves Limbo (discarded if fleeting, else
//! to Reserve, exhausted if cooldown) and the play listeners run.

use crate::framework::ST_SPELL_CLEANUP;
use crate::game::{CardId, Game, PId};
use crate::helpers::ft;
use crate::managers::cards;
use crate::models::action::{check_after_listeners, check_immediate_listeners, get_ctx_args, insert_as_child, resolve_action, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct SpellCleanup;

/// `getCard()`
// PHP: Actions/SpellCleanup.php:32
fn get_card(g: &mut Game, ctx: Ctx) -> R<CardId> {
    let args = get_ctx_args(g, ctx);
    let card_id = php::coalesce(php::get(&args, "cardId"), &Value::Null).clone();
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (spell clenaup). Should not happen"));
    }
    cards::get(g, &card_id)
}

/// The ctx event with `token` set to false
fn event(g: &Game, ctx: Ctx) -> Value {
    let mut e = php::get(&get_ctx_args(g, ctx), "event").clone();
    php::set(&mut e, "token", json!(false));
    e
}

impl ActionClass for SpellCleanup {
    fn name(&self) -> &'static str {
        "SpellCleanup"
    }

    // PHP: Actions/SpellCleanup.php:12
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPELL_CLEANUP))
    }

    // PHP: Actions/SpellCleanup.php:17
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Spell cleanup"))
    }

    // PHP: Actions/SpellCleanup.php:27
    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/SpellCleanup.php:22
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/SpellCleanup.php:42
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let c = get_card(g, ctx)?;

        if card::location(g, c)? != "limbo" {
            let e = event(g, ctx);
            // the 4th argument of checkImmediateListeners does not exist: the event is
            // ImmediateSpellCleanup
            check_immediate_listeners(self, g, player, &e)?;
            check_after_listeners(self, g, ctx, player, &e, true, Some("ChooseAssignment"))?;
            resolve_action(self, g, ctx, php::arr(), json!(false))?;
            return Ok(None);
        }
        // Card must be discarded
        let deleted;
        if card::has_token(g, c, "fleeting")? {
            deleted = card::discard(g, c, &php::arr(), &php::arr())?;
            let token = php::loose_eq(&card::get(g, c, "type")?, &json!("token"));
            check_after_listeners(
                self,
                g,
                ctx,
                player,
                &json!({
                    "discardCard": true,
                    "cardId": c,
                    "token": token,
                    "from": "limbo",
                    "to": "discard",
                    "sourceId": c,
                }),
                true,
                Some("Discard"),
            )?;
        } else {
            // moved to reserve
            deleted = card::move_to_reserve(g, c, &php::arr(), &php::arr())?;
            if card::is(g, c, "cooldown")? {
                // Must use $this->insertAsChild so Action::$ctx is updated by reference
                insert_as_child(g, ctx, &ft::action("Exhaust", json!({"cardId": c}), json!({"pId": player, "sourceId": c})))?;
            }
        }

        g.notify("spellCleanup", json!({"cardId": c, "deleted": deleted}));
        let e = event(g, ctx);
        check_immediate_listeners(self, g, player, &e)?;

        check_after_listeners(self, g, ctx, player, &e, true, Some("ChooseAssignment"))?;

        resolve_action(self, g, ctx, php::arr(), json!(false))?;
        Ok(None)
    }
}
