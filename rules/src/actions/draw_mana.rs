//! Actions/DrawMana.php: the top n deck cards go to mana (exhausted by default).

use crate::framework::ST_DRAW_MANA;
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::models::action::{get_arg, get_ctx_arg, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DrawMana;

/// `$this->ctx->getSource() ?? null`, else `Cards::getSingle($sourceId)` (the node `source` info is
/// never set on action nodes; only the card lookup is kept)
// PHP: Actions/DrawMana.php:62
pub(crate) fn ctx_source(g: &mut Game, ctx: Ctx) -> R<Option<i64>> {
    let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSource() on null"))?;
    let info = crate::core::node::get_source(g, n);
    if !info.is_null() {
        return Err(PhpError::new(format!("unsupported: node info 'source' = {} used as a card", info)));
    }
    let sid = crate::core::node::get_source_id(g, n);
    if sid.is_null() {
        return Ok(None);
    }
    cards::get_single(g, &sid, true)
}

impl ActionClass for DrawMana {
    fn name(&self) -> &'static str {
        "DrawMana"
    }

    // PHP: Actions/DrawMana.php:51
    fn default_args(&self) -> Value {
        json!({"n": 1, "location": "mana", "tapped": true})
    }

    // PHP: Actions/DrawMana.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DRAW_MANA))
    }

    // PHP: Actions/DrawMana.php:20
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        Ok(json!({"log": "Put ${n} to mana", "args": {"n": get_arg(self, g, ctx, "n")?}}))
    }

    // PHP: Actions/DrawMana.php:30
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/DrawMana.php:35
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/DrawMana.php:40
    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/DrawMana.php:45
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let p = get_ctx_arg(g, ctx, "pId");
        let p = if p.is_null() { json!(players::get_active_id(g)) } else { p };
        Ok(players::get_by_value(g, &p))
    }

    // PHP: Actions/DrawMana.php:57
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let n = get_arg(self, g, ctx, "n")?;
        let p = self.get_player(g, ctx)?;
        let _source = ctx_source(g, ctx)?;
        let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function hasDeckCards() on null"))?;
        if !player::has_deck_cards(g, p)? {
            resolve_action(self, g, ctx, Value::Null, json!(true))?;
            return Ok(None);
        }
        let location = php::strval(&get_arg(self, g, ctx, "location")?);
        let picked = cards::pick_for_location(g, php::intval(&n).max(0) as usize, &format!("deck-{}", p), &location, 0, false)?;
        let tapped = get_arg(self, g, ctx, "tapped")?;
        for c in &picked {
            card::set_property(g, *c, "tapped", tapped.clone(), true)?;
        }
        g.notify("discardMana", json!({"player_id": p, "cardIds": picked, "fromLocation": "deck"}));
        resolve_action(self, g, ctx, Value::Null, json!(true))?;
        Ok(None)
    }
}
