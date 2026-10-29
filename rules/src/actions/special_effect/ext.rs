//! Player methods SpecialEffect needs that are not in `models::player` yet, with the signatures
//! of the branch that translates them: port-storm (`models/player.rs`: `has_resupply_if_ascended`,
//! `has_boost_if_ascended`, `has_protect_*_in_expedition`). Once merged, delete them here and call
//! those.

use crate::game::{CardId, Game, PId};
use crate::managers::cards;
use crate::models::{card, player};
use crate::R;
use serde_json::{json, Value};

/// `$player->hasResupplyIfAscended()`
// PHP: Models/Player.php:913
pub fn has_resupply_if_ascended(g: &mut Game, pid: PId) -> R<bool> {
    for c in player::get_played_cards(g, pid, &Value::Null)? {
        if card::is(g, c, "resupplyIfAscended")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `$player->hasBoostIfAscended()`
// PHP: Models/Player.php:924
pub fn has_boost_if_ascended(g: &mut Game, pid: PId) -> R<bool> {
    for c in player::get_played_cards(g, pid, &Value::Null)? {
        if card::is(g, c, "boostIfAscended")? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn played_in(g: &mut Game, pid: PId, expedition: &str) -> R<Vec<CardId>> {
    let played = player::get_played_cards(g, pid, &Value::Null)?;
    Ok(cards::where_location(g, played, &json!(expedition)))
}

fn has_protect_in_expedition(g: &mut Game, pid: PId, expedition: &str, gigantic: bool, prop: &str) -> R<bool> {
    let other = if expedition == "stormLeft" { "stormRight" } else { "stormLeft" };
    for c in played_in(g, pid, expedition)? {
        if card::is(g, c, prop)? {
            return Ok(true);
        }
    }
    if gigantic {
        for c in played_in(g, pid, other)? {
            if card::is(g, c, prop)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `$player->hasProtectAnchoredInExpedition($expedition, $gigantic)`
// PHP: Models/Player.php:980
pub fn has_protect_anchored_in_expedition(g: &mut Game, pid: PId, expedition: &str, gigantic: bool) -> R<bool> {
    has_protect_in_expedition(g, pid, expedition, gigantic, "protectAnchoredInExpedition")
}

/// `$player->hasProtectBoostedInExpedition($expedition, $gigantic)`
// PHP: Models/Player.php:1000
pub fn has_protect_boosted_in_expedition(g: &mut Game, pid: PId, expedition: &str, gigantic: bool) -> R<bool> {
    has_protect_in_expedition(g, pid, expedition, gigantic, "protectBoostedInExpedition")
}
