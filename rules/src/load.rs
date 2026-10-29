//! `Game::from_bga_tables`: a game from the rows of the original engine's tables, as MariaDB
//! returns them (every value a string)

use crate::core::{engine, globals, node};
use crate::game::{Game, Meeple, PlayerRow, PrefRow};
use crate::managers::cards;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

fn s(row: &Value, k: &str) -> String {
    php::strval(php::get(row, k))
}

fn i(row: &Value, k: &str) -> i64 {
    php::intval(php::get(row, k))
}

fn rows<'a>(tables: &'a Value, t: &str) -> Vec<&'a Value> {
    php::vals(php::get(tables, t))
}

/// Builds the game from the dump of `cards`, `meeples`, `global_variables`, `player`,
/// `user_preferences`, plus (when the dump has them) the framework `global` table, the
/// AUTO_INCREMENT values (`auto_increment`) and the dice cursor (`arena_rng`)
pub fn from_bga_tables(tables: &Value, dice: &[i64]) -> R<Game> {
    if !php::is_arr(tables) || php::empty(php::get(tables, "cards")) {
        return Err(PhpError::new("the dump has no `db` tables (golden file older than the db export)"));
    }
    let mut g = Game::empty();
    for r in rows(tables, "cards") {
        let id = i(r, "card_id");
        let c = cards::card_from_row(id, &s(r, "card_location"), i(r, "card_state"), i(r, "player_id"), &s(r, "properties"))?;
        g.cards.insert(id, c);
    }
    g.cards.sort_keys();
    for r in rows(tables, "meeples") {
        let id = i(r, "meeple_id");
        g.meeples.insert(id, Meeple { id, location: s(r, "meeple_location"), state: s(r, "meeple_state"), kind: s(r, "type"), pid: s(r, "player_id") });
    }
    g.meeples.sort_keys();
    for r in rows(tables, "global_variables") {
        let name = s(r, "name");
        let raw = s(r, "value");
        let v: Value = serde_json::from_str(&raw).map_err(|e| PhpError::new(format!("global {}: bad JSON: {}", name, e)))?;
        g.globals.insert(name, v);
    }
    globals::fetch(&mut g);
    for r in rows(tables, "player") {
        let id = i(r, "player_id");
        g.players.insert(
            id,
            PlayerRow {
                id,
                no: i(r, "player_no"),
                name: s(r, "player_name"),
                color: s(r, "player_color"),
                eliminated: s(r, "player_eliminated"),
                score: i(r, "player_score"),
                score_aux: i(r, "player_score_aux"),
                zombie: s(r, "player_zombie"),
                faction: s(r, "faction"),
                is_multiactive: i(r, "player_is_multiactive") == 1,
            },
        );
    }
    g.players.sort_keys();
    for r in rows(tables, "user_preferences") {
        let v = php::get(r, "pref_value");
        g.prefs.push(PrefRow { id: i(r, "id"), player_id: i(r, "player_id"), pref_id: i(r, "pref_id"), value: if v.is_null() { None } else { Some(php::strval(v)) } });
    }

    // AUTO_INCREMENT of cards / meeples (else the next id after the highest one)
    let ai = php::get(tables, "auto_increment");
    g.next_card_id = if php::isset(ai, "cards") { i(ai, "cards") } else { g.cards.keys().max().copied().unwrap_or(0) + 1 };
    g.next_meeple_id = if php::isset(ai, "meeples") { i(ai, "meeples") } else { g.meeples.keys().max().copied().unwrap_or(0) + 1 };

    g.dice = dice.to_vec();
    g.environment = if dice.is_empty() { "studio".into() } else { "prod".into() };
    let rng = php::get(tables, "arena_rng");
    g.dice_used = php::vals(rng).first().map(|r| i(r, "dice") as usize).unwrap_or(0);

    // Framework `global` table: 1 = state, 2 = active player, 10 = logging
    let fw = rows(tables, "global");
    if !fw.is_empty() {
        for r in fw {
            g.fw_set(i(r, "global_id"), i(r, "global_value"));
        }
    } else {
        infer_framework_state(&mut g)?;
    }

    // The undo log starts here: a revert goes back at most to the loaded position
    g.begin_request(None)?;
    crate::helpers::log::checkpoint(&mut g)?;
    Ok(g)
}

/// Dumps without the framework `global` table: the state is the one of the engine's next action
/// (the dumped positions all wait for a player's engine action) and the active player its pId
fn infer_framework_state(g: &mut Game) -> R<()> {
    g.fw_set(10, 1);
    engine::boot(g)?;
    let n = engine::get_next_unresolved(g)?.ok_or_else(|| PhpError::new("load: no global table and no pending engine node"))?;
    let pid = node::get_pid(g, n)?;
    let pid = if pid.is_null() { globals::get(g, "activePId")? } else { pid };
    g.fw_set(2, php::intval(&pid));
    let state = if node::kind(g, n) == node::NodeKind::Leaf { node::get_state(g, n)? } else { json!(crate::framework::ST_RESOLVE_CHOICE) };
    g.fw_set(1, php::intval(&state));
    Ok(())
}
