//! Actions/MoveCard.php: moves characters to the opposite expedition (Floral Tent protections
//! turn the move into a Loose), fires the listeners.
//!
//! PHP defects kept (PHP_ENGINE.md §8.1): line 99 inserts `['action' => BOOST]` ('boost' is not
//! an action name: the node throws when it is reached), and line 100 reads `$player`, undefined
//! unless `cards == ALL` (Error on null). With `cards == ALL` the cards are a Collection
//! (ArrayObject): unsetting the current card during the foreach skips the next one.

use crate::core::{globals, node};
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::collection;
use crate::managers::{cards, players};
use crate::models::action::{self, get_arg, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveCard;

/// `getCard()`
// PHP: Actions/MoveCard.php:44
fn get_card(g: &mut Game, ctx: Ctx) -> R<CardId> {
    let args = get_ctx_args(g, ctx);
    let mut card_id = php::get(&args, "cardId").clone();
    if card_id.is_null() {
        return Err(PhpError::new("no card in args. Should not happen"));
    } else if php::loose_eq(&card_id, &json!("EFFECT")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))?;
        let event = php::get(&node::to_array(g, n), "event").clone();
        card_id = php::get(&event, "cardId").clone();
        if card_id.is_null() {
            card_id = php::get(php::get(&event, "gain"), "cardId").clone();
        }
    } else if php::loose_eq(&card_id, &json!("me")) {
        card_id = action::get_source_id(g, ctx)?;
    }
    cards::get(g, &card_id)
}

/// `$map[$location]` of the opposite expeditions
fn opposite(loc: &str) -> Value {
    match loc {
        "stormLeft" => json!("stormRight"),
        "stormRight" => json!("stormLeft"),
        _ => Value::Null,
    }
}

impl ActionClass for MoveCard {
    fn name(&self) -> &'static str {
        "MoveCard"
    }

    fn default_args(&self) -> Value {
        json!({"location": "opposite", "player": "me", "cards": 1})
    }

    // PHP: Actions/MoveCard.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_CARD))
    }

    // PHP: Actions/MoveCard.php:21
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        if php::loose_eq(&get_arg(self, g, ctx, "cards")?, &json!("all")) {
            return Ok(json!("move all characters to opposite expedition"));
        }
        let cid = get_ctx_arg(g, ctx, "cardId");
        if php::loose_eq(&cid, &Value::Null) || php::loose_eq(&cid, &json!("EFFECT")) {
            return Ok(json!("move a character to opposite expedition"));
        }
        let c = get_card(g, ctx)?;
        let name = card::get(g, c, "name")?;
        Ok(json!({"log": "move ${card_name} to opposite expedition", "args": {"card_name": name, "i18n": ["card_name"]}}))
    }

    // PHP: Actions/MoveCard.php:37
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/MoveCard.php:61
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        // `$player` is only defined when all the cards move
        let mut player: Option<PId> = None;
        let mut list: Vec<CardId>;
        let is_collection;
        if php::loose_eq(&get_arg(self, g, ctx, "cards")?, &json!("all")) {
            // we must switch all
            let active = players::get_active(g);
            let p = if php::loose_eq(&get_arg(self, g, ctx, "player")?, &json!("me")) {
                active
            } else {
                let a = active.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
                players::get_next(g, a)?
            };
            let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function getPlayedCards() on null"))?;
            player = Some(p);
            let chars = player::get_played_cards(g, p, &json!("character"))?;
            let tokens = player::get_played_cards(g, p, &json!("token"))?;
            list = collection::merge(&chars, &tokens);
            is_collection = true;
        } else {
            list = vec![get_card(g, ctx)?];
            is_collection = false;
        }
        let source = action::get_source(g, ctx)?;

        let mut i = 0;
        while i < list.len() {
            let c = list[i];
            let from = card::location(g, c)?;
            let in_storms = from == "stormLeft" || from == "stormRight";
            let is_unit = php::in_list(&card::get(g, c, "type")?, &["token", "character"]);

            // Floral Tent
            let mut protected: Option<(&str, Value)> = None;
            if globals::is(g, "dayPhase")? && in_storms && is_unit {
                let cp = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function hasProtectAnchoredInExpedition() on null"))?;
                if player::has_protect_in_expedition(g, cp, "protectAnchoredInExpedition", &from, false)? && card::has_token(g, c, "anchored")? {
                    protected = Some(("anchored", json!({"action": "Loose", "args": {"cardId": c, "type": "anchored"}})));
                }
            }
            // Floral tent bravos
            if protected.is_none() && globals::is(g, "dayPhase")? && in_storms && is_unit {
                let cp = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function hasProtectBoostedInExpedition() on null"))?;
                if player::has_protect_in_expedition(g, cp, "protectBoostedInExpedition", &from, false)? && card::has_token(g, c, "boost")? {
                    protected = Some(("boost", json!({"action": "Loose", "args": {"cardId": c, "type": "boost", "n": 99}})));
                }
            }
            if let Some((token, flow)) = protected {
                // unset($cards[$cId]) inside the foreach
                list.remove(i);
                g.notify("mediumMessage", json!({"log": format!("${{card_name}} is not discarded but loose <{}> instead", token.to_uppercase()), "cardId": c}));
                action::insert_as_child(g, ctx, &flow)?;
                if is_collection {
                    // ArrayObject iterator: the card after the removed one is skipped
                    i += 1;
                }
                continue;
            }

            if php::loose_eq(&get_arg(self, g, ctx, "location")?, &json!("opposite")) {
                let to = opposite(&card::location(g, c)?);
                if to.is_null() {
                    return Err(PhpError::new("Column 'card_location' cannot be null (setLocation(null))"));
                }
                card::set_location(g, c, &php::strval(&to))?;
            }

            let s = source.ok_or_else(|| PhpError::new("Error: Call to a member function getPlayer() on null"))?;
            let sp = card::get_player(g, s)?;
            g.notify("moveCard", json!({"player_id": sp, "cardId": c, "card2": s}));

            let mut boosts = globals::get(g, "nextCharacterInExpeditionBoost")?;
            let back = php::key_of(&opposite(&card::location(g, c)?));
            let cpid = card::pid(g, c)?.to_string();
            if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) && php::isset(php::get(&boosts, &cpid), &back) && !php::truthy(&card::is_gigantic(g, c, false)?) {
                let n = php::get(php::get(&boosts, &cpid), &back).clone();
                action::insert_as_child(g, ctx, &json!({"action": "boost", "args": {"cardId": c, "type": "boost", "n": n}}))?;
                let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
                let pk = p.to_string();
                if php::is_arr(php::get(&boosts, &pk)) {
                    php::unset(php::get_mut(&mut boosts, &pk), &back);
                }
                globals::set(g, "nextCharacterInExpeditionBoost", boosts)?;
            }

            let sp = card::get_player(g, s)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            let args = json!({
                "cardId": c,
                "playCard": true,
                "cardType": card::get(g, c, "type")?,
                "additionalType": card::get_additional_type(g, c)?,
                "cardSubtypes": card::get_subtypes(g, c)?,
                "from": from,
                "to": card::location(g, c)?,
                "locationPId": card::pid(g, c)?,
                "token": card::is_token(g, c)?,
            });
            action::check_after_listeners(self, g, ctx, sp, &args, true, None)?;
            i += 1;
        }

        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
