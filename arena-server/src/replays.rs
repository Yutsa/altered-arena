//! Replays of BGA games in the engine's native format (`altered-replay`), read from the
//! ARENA_REPLAYS directory (TABLE.replay.json): the list, and the GameViews of a whole game from
//! one seat, for the client's replay viewer.

use altered_replay::native::{self, NativeReplay};
use serde_json::{json, Value};

pub fn dir() -> std::path::PathBuf {
    std::env::var("ARENA_REPLAYS").unwrap_or_else(|_| "replays".into()).into()
}

fn read(table: &str) -> Result<NativeReplay, String> {
    if !table.chars().all(|c| c.is_ascii_digit()) {
        return Err("Unknown replay".into());
    }
    let text = std::fs::read_to_string(dir().join(format!("{}.replay.json", table))).map_err(|_| "Unknown replay".to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Every replay: table, players (name, faction, hero), number of steps
pub fn list() -> Value {
    let mut out = vec![];
    let mut files: Vec<_> = std::fs::read_dir(dir()).into_iter().flatten().flatten().map(|e| e.path()).collect();
    files.sort();
    for f in files {
        let Some(name) = f.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".replay.json")) else { continue };
        let Ok(n) = read(name) else { continue };
        let players: Vec<Value> = n
            .names
            .iter()
            .zip(&n.decks)
            .map(|(name, d)| json!({"name": name, "faction": d["faction"], "hero": d["hero"]}))
            .collect();
        out.push(json!({"table": n.table, "players": players, "steps": n.steps.len()}));
    }
    Value::Array(out)
}

/// The GameView of `seat` (2001, 2002; None: spectator) at the start and after every step
pub fn views(table: &str, seat: Option<u32>) -> Result<Value, String> {
    let n = read(table)?;
    let id = format!("replay-{}", n.table);
    let mut views = vec![];
    let mut steps = vec![];
    native::replay(&n, |g, s| {
        // On a copy: a view may create preference and global rows
        let mut c = g.clone();
        match c.view(seat, &id) {
            Ok(mut v) => {
                if let Some(ps) = v["players"].as_array_mut() {
                    for p in ps {
                        let i = p["id"].as_i64().unwrap_or(0) - 2001;
                        if let Some(name) = n.names.get(i as usize) {
                            p["name"] = json!(name);
                        }
                    }
                }
                views.push(v);
            }
            Err(e) => views.push(json!({"error": e.0})),
        }
        if let Some(s) = s {
            steps.push(json!({"move": s.m, "player": s.p, "answer": s.a}));
        }
    })?;
    Ok(json!({"table": n.table, "names": n.names, "steps": steps, "views": views}))
}
