//! The game of a replay, created by the arena as `explore.php replay` does: both custom decks
//! confirmed, every die the replay rolls in its list, the decks' shuffles forced so that cards
//! leave them in the order the replay shows, and the Tumult cards the replay reveals.

use crate::bga::BgaReplay;
use altered_rules::arena::{self, NewPlayer, PLAYER_IDS};
use altered_rules::core::globals;
use altered_rules::game::{PrefRow, ShuffleHook};
use altered_rules::{php, v1, Game, PhpError};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub fn create(r: &BgaReplay) -> Result<Game, PhpError> {
    let mut decks = vec![];
    for pid in PLAYER_IDS {
        decks.push(r.api_deck(pid as i64).ok_or_else(|| PhpError::new(&format!("no custom deck for player {}", pid)))?);
    }
    let hook = ShuffleHook::Revealed { reveals: Arc::new(r.reveals()), applied: vec![] };
    create_with(r.table, &decks, &r.dice(), &r.storm(), hook)
}

/// The game of a replay from its parts: the two decks confirmed (in seat order), the dice, the
/// Tumult cards revealed (storm index => card, rotated) and how the decks are shuffled
pub fn create_with(table: i64, decks: &[Value], dice: &[i64], storm: &BTreeMap<i64, (i64, bool)>, hook: ShuffleHook) -> Result<Game, PhpError> {
    // A dice list makes the game run in the 'prod' environment, as on BGA
    let dice = if dice.is_empty() { vec![1] } else { dice.to_vec() };
    let players: Vec<NewPlayer> = ["A", "B"].iter().map(|n| NewPlayer { name: n.to_string(), bot: false, deck: None, api_deck: None }).collect();
    let mut g = arena::create_game(table, &players, &dice)?;
    g.shuffle_hook = hook;
    for (pid, deck) in PLAYER_IDS.iter().zip(decks) {
        v1::build(&mut g, Some(*pid), "")?;
        v1::act(&mut g, *pid, "actConfirmAPIDeck", &json!([deck]))?;
    }
    // Tumult cards revealed during the game (the others keep their random draw)
    g.begin_request(None)?;
    let mut tiles = globals::get_storm(&mut g, false);
    let mut rest: Vec<i64> = (1..=3).filter(|i| !storm.values().any(|(c, _)| c == i)).collect();
    for i in 1..=3i64 {
        let key = i.to_string();
        let tile = php::get(&tiles, &key).clone();
        let mut tile = if tile.is_null() { json!({}) } else { tile };
        match storm.get(&i) {
            Some((card, rotated)) => {
                php::set(&mut tile, "cardId", json!(card));
                php::set(&mut tile, "rotated", json!(rotated));
            }
            None => {
                let card = if rest.is_empty() { php::intval(php::get(&tile, "cardId")) } else { rest.remove(0) };
                php::set(&mut tile, "cardId", json!(card));
            }
        }
        php::set(&mut tiles, &key, tile);
    }
    globals::set(&mut g, "storm", tiles)?;
    Ok(g)
}

/// Sets a player's preference row (Preferences::get reads the first row)
pub fn set_pref(g: &mut Game, pid: i64, pref: i64, value: i64) {
    g.prefs.retain(|p| !(p.player_id as i64 == pid && p.pref_id == pref));
    let id = g.prefs.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    g.prefs.push(PrefRow { id, player_id: pid as _, pref_id: pref, value: Some(value.to_string()) });
}

pub fn get_pref(g: &Game, pid: i64, pref: i64) -> Option<i64> {
    g.prefs.iter().find(|p| p.player_id as i64 == pid && p.pref_id == pref).and_then(|p| p.value.as_ref()).and_then(|v| v.parse().ok())
}
