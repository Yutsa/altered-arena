//! Actions/Draw.php: me / opponent / owner / all players draw n cards (or put the top card in
//! mana), firing the Draw listeners.

use crate::actions::draw_mana::ctx_source;
use crate::framework::ST_DRAW;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{check_after_listeners, get_arg, get_ctx_arg, resolve_action, ActionClass, Ctx};
use crate::models::player;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Draw;

/// `getDrawTargetPlayers()`
// PHP: Actions/Draw.php:78
fn get_draw_target_players(a: &Draw, g: &mut Game, ctx: Ctx) -> R<Vec<Option<PId>>> {
    let who = php::coalesce(&get_ctx_arg(g, ctx, "players"), &json!("all")).clone();
    if php::loose_eq(&who, &json!("me")) {
        return Ok(vec![players::get_active(g)]);
    }
    if php::loose_eq(&who, &json!("all")) {
        return Ok(players::get_all(g).into_iter().map(Some).collect());
    }
    if php::loose_eq(&who, &json!("owner")) {
        let owner = get_arg(a, g, ctx, "ownerId")?;
        if owner.is_null() {
            return Err(PhpError::new("No owner of card. Should not happen"));
        }
        return Ok(vec![players::get_by_value(g, &owner)]);
    }
    if php::loose_eq(&who, &json!("opponent")) {
        let active = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        return Ok(vec![players::get_next(g, active)?]);
    }
    Ok(vec![players::get_active(g)])
}

impl ActionClass for Draw {
    fn name(&self) -> &'static str {
        "Draw"
    }

    // PHP: Actions/Draw.php:109
    fn default_args(&self) -> Value {
        json!({"n": 1, "players": "all", "location": "hand", "tapped": false, "ownerId": null})
    }

    // PHP: Actions/Draw.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_DRAW))
    }

    // PHP: Actions/Draw.php:20
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        let players_v = php::coalesce(&get_ctx_arg(g, ctx, "players"), &json!("all")).clone();
        let location = get_arg(self, g, ctx, "location")?;
        let log = if php::loose_eq(&players_v, &json!("all")) {
            if php::loose_eq(&location, &json!("mana")) {
                "All players puts the top deck card in their mana one"
            } else {
                "All players draw ${n} card(s)"
            }
        } else if php::loose_eq(&players_v, &json!("opponent")) {
            "The opponent draws ${n} card(s)"
        } else if php::loose_eq(&players_v, &json!("owner")) {
            "The owner of the card draws ${n} card(s)"
        } else {
            "Draw ${n} card(s)"
        };
        Ok(json!({"log": log, "args": {"n": n}}))
    }

    // PHP: Actions/Draw.php:68
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Draw.php:73
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Draw.php:101
    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Draw.php:117
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        let source = ctx_source(g, ctx)?;
        let ps = get_draw_target_players(self, g, ctx)?;
        for p in ps {
            let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function hasDeckCards() on null"))?;
            if !player::has_deck_cards(g, p)? {
                continue;
            }
            let location = get_arg(self, g, ctx, "location")?;
            let drawn = if php::loose_eq(&location, &json!("mana")) {
                let hero = player::get_hero(g, p)?;
                let tapped = php::truthy(&get_arg(self, g, ctx, "tapped")?);
                player::draw(g, p, 1, None, Some("mana"), hero, tapped, None)?
            } else {
                player::draw(g, p, php::intval(&n), None, None, source, false, None)?
            };
            let location = get_arg(self, g, ctx, "location")?;
            if !drawn.is_empty() && php::loose_ne(&location, &json!("mana")) {
                check_after_listeners(self, g, ctx, p, &json!({"draw": n, "location": location}), true, None)?;
            }
        }
        resolve_action(self, g, ctx, Value::Null, json!(true))?;
        Ok(None)
    }
}
