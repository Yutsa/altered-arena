//! Actions/PlayCard.php: plays a given card (cost-reduction alternatives, effects that play a
//! card); the play itself is ChooseAssignment::playCard with this node as ctx.

use crate::actions::choose_assignment;
use crate::framework::ST_PLAY_CARD;
use crate::game::{CardId, Game, PId};
use crate::managers::{cards, players};
use crate::models::action::{get_arg, get_ctx_arg, param_req, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct PlayCard;

/// `getCard($description)`: null only when there is no card id and `$description`
// PHP: Actions/PlayCard.php:49
fn get_card(g: &mut Game, ctx: Ctx, description: bool) -> R<Option<CardId>> {
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    if php::loose_eq(&card_id, &json!("me")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
        card_id = crate::core::node::get_source_id(g, n);
    }

    if card_id.is_null() && description {
        return Ok(None);
    } else if card_id.is_null() {
        return Err(PhpError::new("no card in args (play card). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

fn get_card_req(g: &mut Game, ctx: Ctx, method: &str) -> R<CardId> {
    get_card(g, ctx, false)?.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `argsPlayCard()`
// PHP: Actions/PlayCard.php:85
fn args_play_card(a: &PlayCard, g: &mut Game, ctx: Ctx) -> R<Value> {
    let c = get_card_req(g, ctx, "getId")?;
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let free = php::truthy(&get_arg(a, g, ctx, "free")?);
    if !free && !card::can_be_played(g, c, p, false, false)? {
        return Err(PhpError::new("Card cannot be played. Should not happen"));
    }

    let mut locations = php::arr();
    let locs = card::get_playable_location(g, c, p, &Value::Null, free)?;
    php::set(&mut locations, &c.to_string(), locs);

    let c2 = get_card_req(g, ctx, "getName")?;
    let name = card::get(g, c2, "name")?;
    Ok(json!({
        "card_name": name,
        "i18n": ["card_name"],
        "cardId": c,
        "_private": {"active": {"play": locations}},
        "descSuffix": if free { "free" } else { "" },
    }))
}

impl ActionClass for PlayCard {
    fn name(&self) -> &'static str {
        "PlayCard"
    }

    // PHP: Actions/PlayCard.php:64
    fn default_args(&self) -> Value {
        json!({
            "n": 1,
            "free": false,
            "effectHand": true,
            "location": "",
            "cost": 0,
            "costReduction": 0,
            "reallyPlayed": true,
            "stealOwnership": false,
            // null = infer in playCard (free + cost 0 => not a turn play); true for cost-reduction paths
            "countsAsTurnPlay": null,
        })
    }

    // PHP: Actions/PlayCard.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_PLAY_CARD))
    }

    // PHP: Actions/PlayCard.php:20
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let _player = self.get_player(g, ctx)?;
        let c = get_card(g, ctx, true)?;
        let mut msg = "Play ${card_name}";

        let Some(c) = c else {
            return Ok(json!("Play a card"));
        };
        if php::gt(&get_arg(self, g, ctx, "cost")?, &json!(0)) {
            msg = "Play ${card_name} for ${mana_cost}";
        }

        let name = card::get(g, c, "name")?;
        let cost = get_arg(self, g, ctx, "cost")?;
        Ok(json!({
            "log": msg,
            "args": {
                "card_name": name,
                "mana_cost": cost,
                "i18n": ["card_name"],
            },
        }))
    }

    // PHP: Actions/PlayCard.php:43
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let pid = get_ctx_arg(g, ctx, "pId");
        let pid = if pid.is_null() { json!(players::get_active_id(g)) } else { pid };
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/PlayCard.php:77
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let c = get_card_req(g, ctx, "isExhaustedReservePlayBlocked")?;
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        if card::is_exhausted_reserve_play_blocked(g, c, p)? {
            return Ok(false);
        }
        let free = php::truthy(&get_arg(self, g, ctx, "free")?);
        if php::empty(&card::get_playable_location(g, c, p, &Value::Null, free)?) {
            return Ok(false);
        }
        let orbs = card::get(g, c, "minManaOrbs")?;
        Ok(php::le(&orbs, &json!(player::get_total_mana(g, p)?)))
    }

    // PHP: Actions/PlayCard.php:85
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_play_card(self, g, ctx)?))
    }

    // PHP: Actions/PlayCard.php:116
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let location = get_arg(self, g, ctx, "location")?;
        if php::loose_ne(&location, &json!("")) {
            let c = get_card_req(g, ctx, "getId")?;
            return Ok(Some(json!([c, location])));
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actplaycard" => {
                let card_id = param_req(args, 0, "cardId", "actPlayCard")?;
                let location = param_req(args, 1, "location", "actPlayCard")?;
                act_play_card(self, g, ctx, &card_id, &location)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\PlayCard::{}()", method))),
        }
    }
}

/// `actPlayCard($cardId, $location)`
// PHP: Actions/PlayCard.php:123
fn act_play_card(a: &PlayCard, g: &mut Game, ctx: Ctx, card_id: &Value, location: &Value) -> R<()> {
    let args = args_play_card(a, g, ctx)?;
    let play = php::get(php::get(php::get(&args, "_private"), "active"), "play").clone();
    let locations = php::get(&play, &php::key_of(card_id)).clone();
    if locations.is_null() {
        return Err(PhpError::new("This card cannot be played. Should not happen"));
    }
    if !php::in_array(location, &locations) {
        return Err(PhpError::new("Invalid location to play a card. Should not happen"));
    }
    let c = cards::get(g, card_id)?;
    let active = players::get_active_id(g);
    if php::truthy(&get_arg(a, g, ctx, "stealOwnership")?) && card::pid(g, c)? != active {
        let mut extra = card::get(g, c, "extraDatas")?;
        if !php::isset(&extra, "pId") {
            let pid = card::pid(g, c)?;
            php::set(&mut extra, "pId", json!(pid));
            card::set_property(g, c, "extraDatas", extra, true)?;
        }
        card::set_pid(g, c, &json!(active))?;
    }

    let free = get_arg(a, g, ctx, "free")?;
    let effect_hand = get_arg(a, g, ctx, "effectHand")?;
    let cost = get_arg(a, g, ctx, "cost")?;
    let really_played = get_arg(a, g, ctx, "reallyPlayed")?;
    let steal = get_arg(a, g, ctx, "stealOwnership")?;
    let counts = get_arg(a, g, ctx, "countsAsTurnPlay")?;
    choose_assignment::play_card(g, ctx, card_id, location, &free, &effect_hand, &cost, &really_played, false, &steal, &counts)
}
