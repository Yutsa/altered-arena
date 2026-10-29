//! Player / Players methods SpecialEffect needs that are not in `models::player` /
//! `managers::players` yet, with the signatures of the branches that translate them: port-storm
//! (`models/player.rs`: `has_resupply_if_ascended`, `has_boost_if_ascended`,
//! `has_protect_*_in_expedition`) and port-cards (`managers/players.rs`: `filter_biomes`,
//! `get_biomes_in_storm`). Once merged, delete them here and call those.

use crate::game::{CardId, Game, PId};
use crate::managers::{cards, players};
use crate::models::{card, player};
use crate::php;
use crate::R;
use serde_json::{json, Value};

/// `Players::getBiomesInStorm($excludeMoveInfo)`: pId => expedition => biomes (keyed by biome)
// PHP: Managers/Players.php:511
pub fn get_biomes_in_storm(g: &mut Game, exclude_move_info: bool) -> R<Value> {
    let mut all = php::arr();
    for p in players::get_all(g) {
        let storm = player::get_biome_in_storms(g, p)?;
        for (side, biomes) in php::entries(&storm) {
            let expedition = if side == "hero" { "stormLeft" } else { "stormRight" };
            let mut nb = php::arr();
            for b in php::vals(biomes) {
                php::set(&mut nb, &php::key_of(b), b.clone());
            }
            players::biomes_modifier(g, &mut nb, p, expedition, false, exclude_move_info)?;
            php::set(php::get_mut(&mut all, &p.to_string()), expedition, nb);
        }
    }
    Ok(all)
}

/// `Players::filterBiomes($expeditionAttributes)`: pId => expeditions having one of the biomes
// PHP: Managers/Players.php:531
pub fn filter_biomes(g: &mut Game, attributes: &Value) -> R<Value> {
    let storms = get_biomes_in_storm(g, true)?;
    let mut out = php::arr();
    for (pid, ps) in php::entries(&storms) {
        let mut locs = php::arr();
        if attributes.is_null() {
            php::set(&mut out, &pid, Value::Array(php::keys(ps)));
            continue;
        }
        for a in php::vals(attributes) {
            let k = php::key_of(a);
            if php::isset(php::get(ps, "stormLeft"), &k) {
                php::push(&mut locs, json!("stormLeft"));
            }
            if php::isset(php::get(ps, "stormRight"), &k) {
                php::push(&mut locs, json!("stormRight"));
            }
        }
        php::set(&mut out, &pid, locs);
    }
    Ok(out)
}

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
