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
/// AUTO_INCREMENT values (`auto_increment`) and the dice cursor and seed (`arena_rng`)
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
        std::sync::Arc::make_mut(&mut g.globals).insert(name, crate::game::GlobalSlot::new(v));
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

    g.dice = std::sync::Arc::new(dice.to_vec());
    g.environment = if dice.is_empty() { "studio".into() } else { "prod".into() };
    // arena_rng: dice cursor, and the seed of the next act (game id, acts so far; dumps older than
    // seeded randomness have neither)
    if let Some(r) = php::vals(php::get(tables, "arena_rng")).first() {
        g.dice_used = i(r, "dice") as usize;
        g.rng_game = i(r, "game");
        g.rng_acts = i(r, "acts");
    }

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

/// The game's tables in the format of `from_bga_tables` (explore.php `dumpTables`: every value
/// a string, JSON columns as JSON text)
pub fn to_bga_tables(g: &Game) -> Value {
    let s = |v: &dyn std::fmt::Display| json!(v.to_string());
    let mut cards: Vec<&crate::game::CardRow> = g.cards.values().collect();
    cards.sort_by_key(|c| c.id);
    let cards: Vec<Value> = cards
        .into_iter()
        .map(|c| json!({"card_id": s(&c.id), "card_location": c.location, "card_state": s(&c.state), "player_id": s(&c.pid), "properties": serde_json::to_string(&*c.db_props).unwrap()}))
        .collect();
    let mut meeples: Vec<&Meeple> = g.meeples.values().collect();
    meeples.sort_by_key(|m| m.id);
    let meeples: Vec<Value> = meeples
        .into_iter()
        .map(|m| json!({"meeple_id": s(&m.id), "meeple_location": m.location, "meeple_state": m.state, "type": m.kind, "player_id": m.pid}))
        .collect();
    let mut names: Vec<&String> = g.globals.keys().collect();
    names.sort();
    let globals: Vec<Value> = names.into_iter().map(|n| json!({"name": n, "value": serde_json::to_string(g.globals[n].db_value()).unwrap()})).collect();
    let players: Vec<Value> = g
        .players
        .values()
        .map(|p| {
            json!({
                "player_id": s(&p.id), "player_no": s(&p.no), "player_name": p.name, "player_color": p.color,
                "player_score": s(&p.score), "player_score_aux": s(&p.score_aux), "player_zombie": p.zombie,
                "player_eliminated": p.eliminated, "player_is_multiactive": if p.is_multiactive { "1" } else { "0" },
                "faction": if p.faction.is_empty() { Value::Null } else { json!(p.faction) },
            })
        })
        .collect();
    let prefs: Vec<Value> = g.prefs.iter().map(|p| json!({"id": s(&p.id), "player_id": s(&p.player_id), "pref_id": s(&p.pref_id), "pref_value": p.value})).collect();
    let fw: Vec<Value> = g.fw.iter().map(|(k, v)| json!({"global_id": s(k), "global_value": s(v)})).collect();
    json!({
        "cards": cards,
        "meeples": meeples,
        "global_variables": globals,
        "player": players,
        "user_preferences": prefs,
        "global": fw,
        "arena_rng": [{"dice": s(&g.dice_used), "game": s(&g.rng_game), "acts": s(&g.rng_acts)}],
        "auto_increment": {"cards": s(&g.next_card_id), "meeples": s(&g.next_meeple_id)},
    })
}

/// The differences between two table dumps (`to_bga_tables` / explore.php `dumpTables`): values
/// compared as PHP reads them (numeric strings as numbers, JSON columns decoded, null faction as
/// empty), columns the translation does not keep (player_row, avatar, canal) ignored
pub fn diff_tables(a: &Value, b: &Value) -> Vec<String> {
    fn canon(v: &Value, json_col: bool) -> Value {
        match v {
            Value::String(s) if json_col => {
                let mut x: Value = serde_json::from_str(s).unwrap_or(Value::String(s.clone()));
                php::normalize(&mut x);
                x
            }
            Value::String(s) => s.parse::<i64>().map(|n| json!(n)).unwrap_or_else(|_| v.clone()),
            Value::Null => json!(""),
            _ => v.clone(),
        }
    }
    let mut out = vec![];
    let tables = ["cards", "meeples", "player", "user_preferences", "global", "arena_rng"];
    for t in tables {
        let ra = php::vals(php::get(a, t));
        let rb = php::vals(php::get(b, t));
        if ra.len() != rb.len() {
            out.push(format!("{}: {} rows vs {}", t, ra.len(), rb.len()));
        }
        for (x, y) in ra.iter().zip(rb.iter()) {
            let keys: Vec<String> = php::keys_str(x).into_iter().chain(php::keys_str(y)).filter(|k| !["player_row", "player_avatar", "player_canal"].contains(&k.as_str())).collect();
            for k in keys {
                let j = k == "properties";
                let (va, vb) = (canon(php::get(x, &k), j), canon(php::get(y, &k), j));
                if va != vb {
                    out.push(format!("{} row {}: {} = {} vs {}", t, php::strval(&canon(&php::vals(x).first().cloned().cloned().unwrap_or(Value::Null), false)), k, va, vb));
                    break;
                }
            }
        }
    }
    let globals = |v: &Value| -> Vec<(String, Value)> { php::vals(php::get(v, "global_variables")).into_iter().map(|r| (php::strval(php::get(r, "name")), canon(php::get(r, "value"), true))).collect() };
    let (ga, gb) = (globals(a), globals(b));
    for (k, v) in &ga {
        match gb.iter().find(|(n, _)| n == k) {
            Some((_, w)) if w != v => out.push(format!("global {}: {} vs {}", k, v, w)),
            None => out.push(format!("global {} only in the first dump: {}", k, v)),
            _ => {}
        }
    }
    for (k, v) in &gb {
        if !ga.iter().any(|(n, _)| n == k) {
            out.push(format!("global {} only in the second dump: {}", k, v));
        }
    }
    for k in ["cards", "meeples"] {
        let (x, y) = (canon(php::get(php::get(a, "auto_increment"), k), false), canon(php::get(php::get(b, "auto_increment"), k), false));
        if x != y {
            out.push(format!("auto_increment {}: {} vs {}", k, x, y));
        }
    }
    out
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
