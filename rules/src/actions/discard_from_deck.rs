//! Actions/DiscardFromDeck.php: mills n cards from the deck of me, the opponent or all players.

use crate::actions::draw_mana::ctx_source;
use crate::framework::ST_DISCARD_FROM_DECK;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{get_arg, get_ctx_arg, resolve_action, ActionClass, Ctx};
use crate::models::player;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct DiscardFromDeck;

/// `getPlayer()`: unlike the base method it returns a list of players (only called by
/// `stDiscardFromDeck`, so the trait's `get_player` is not overridden)
// PHP: Actions/DiscardFromDeck.php:43
fn get_players(a: &DiscardFromDeck, g: &mut Game, ctx: Ctx) -> R<Vec<Option<PId>>> {
    let t = get_ctx_arg(g, ctx, "players");
    let target_players = if t.is_null() { get_arg(a, g, ctx, "players")? } else { t };
    let current = players::get_active(g);
    let mut target = vec![];
    if php::loose_eq(&target_players, &json!("me")) {
        target = vec![current];
    } else if php::loose_eq(&target_players, &json!("opponent")) {
        let c = current.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        target = vec![players::get_next(g, c)?];
    } else if php::loose_eq(&target_players, &json!("all")) {
        target = players::get_all(g).into_iter().map(Some).collect();
    }
    Ok(target)
}

impl ActionClass for DiscardFromDeck {
    fn name(&self) -> &'static str {
        "DiscardFromDeck"
    }

    // PHP: Actions/DiscardFromDeck.php:59
    fn default_args(&self) -> Value {
        json!({"players": "me", "n": 1})
    }

    // PHP: Actions/DiscardFromDeck.php:18
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DISCARD_FROM_DECK))
    }

    // PHP: Actions/DiscardFromDeck.php:23
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        Ok(json!({"log": "Discard ${n} from deck", "args": {"n": get_arg(self, g, ctx, "n")?}}))
    }

    // PHP: Actions/DiscardFromDeck.php:33
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(false)
    }

    // PHP: Actions/DiscardFromDeck.php:38
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/DiscardFromDeck.php:64
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let n = get_arg(self, g, ctx, "n")?;
        let ps = get_players(self, g, ctx)?;
        let source = ctx_source(g, ctx)?;
        for p in ps {
            let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function draw() on null"))?;
            player::draw(g, p, php::intval(&n), Some(&format!("deck-{}", p)), Some("discard"), source, false, None)?;
        }
        resolve_action(self, g, ctx, Value::Null, json!(true))?;
        Ok(None)
    }
}
