//! A BGA replay as the "logs" request of a finished table returns it (both players' channels):
//! the notifications of every request, grouped by move id. The players' BGA ids are replaced by
//! the arena's (2001, 2002, in seat order) everywhere in the file.

use altered_rules::arena::PLAYER_IDS;
use serde_json::Value;
use std::collections::BTreeMap;

/// Notifications that depend on the BGA plumbing, not on the game: gameStateChange (states are
/// kept apart), newUndoableStep (removed from the log once a turn is confirmed), and what the
/// framework sends (simpleNode, simpleNote: player colours, wakeupPlayers, timers)
pub const IGNORED: &[&str] = &[
    "gameStateChange",
    "gameStateMultipleActiveUpdate",
    "updateReflexionTime",
    "newUndoableStep",
    "simpleNode",
    "simpleNote",
    "wakeupPlayers",
];

#[derive(Clone, Debug)]
pub struct Notif {
    /// None: the table channel; a player id: that player's private channel
    pub channel: Option<i64>,
    pub kind: String,
    pub args: Value,
}

#[derive(Clone, Debug, Default)]
pub struct Move {
    pub id: i64,
    pub notifs: Vec<Notif>,
    /// States sent on the table channel, with their active player
    pub states: Vec<(i64, i64)>,
}

#[derive(Clone, Debug)]
pub struct BgaReplay {
    pub table: i64,
    /// BGA ids of the players, in seat order (arena ids 2001, 2002)
    pub real_ids: Vec<i64>,
    pub names: Vec<String>,
    /// Requests in order (moves with no notification and no state are left out)
    pub moves: Vec<Move>,
}

fn int(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

impl BgaReplay {
    pub fn parse(text: &str) -> Result<BgaReplay, String> {
        let raw: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let data = &raw["data"];
        let players = data["players"].as_array().ok_or("no players")?;
        let real_ids: Vec<i64> = players.iter().map(|p| int(&p["id"])).collect();
        let names: Vec<String> = players.iter().map(|p| p["name"].as_str().unwrap_or("").to_string()).collect();
        if real_ids.len() != 2 {
            return Err("not a two-player game".into());
        }
        // Every occurrence of a player id (numbers, strings, "deck-<id>", "/player/p<id>"...)
        let mut text = text.to_string();
        for (i, id) in real_ids.iter().enumerate() {
            text = text.replace(&id.to_string(), &PLAYER_IDS[i].to_string());
        }
        let mut de = serde_json::Deserializer::from_str(&text);
        de.disable_recursion_limit();
        let raw: Value = serde::Deserialize::deserialize(&mut de).map_err(|e| e.to_string())?;
        let table = int(&raw["meta"]["table"]);
        let mut logs: Vec<&Value> = raw["data"]["logs"].as_array().ok_or("no logs")?.iter().collect();
        logs.sort_by_key(|p| int(&p["packet_id"]));
        let mut moves: BTreeMap<i64, Move> = BTreeMap::new();
        for packet in logs {
            if packet["move_id"].is_null() {
                continue; // framework packets outside any request
            }
            let id = int(&packet["move_id"]);
            let channel = packet["channel"].as_str().unwrap_or("");
            let channel = channel.strip_prefix("/player/p").map(|p| p.parse::<i64>().unwrap_or(0));
            let m = moves.entry(id).or_insert_with(|| Move { id, ..Default::default() });
            for n in packet["data"].as_array().into_iter().flatten() {
                let kind = n["type"].as_str().unwrap_or("").to_string();
                if kind == "gameStateChange" && channel.is_none() && !n["args"]["id"].is_null() {
                    m.states.push((int(&n["args"]["id"]), int(&n["args"]["active_player"])));
                }
                if IGNORED.contains(&kind.as_str()) {
                    continue;
                }
                m.notifs.push(Notif { channel, kind, args: n["args"].clone() });
            }
        }
        Ok(BgaReplay { table, real_ids, names, moves: moves.into_values().collect() })
    }

    fn all(&self) -> impl Iterator<Item = (i64, &Notif)> {
        self.moves.iter().flat_map(|m| m.notifs.iter().map(move |n| (m.id, n)))
    }

    /// The custom deck a player confirmed (`_private.API` of updateInitialPrecoDeckSelection)
    pub fn api_deck(&self, pid: i64) -> Option<Value> {
        self.all()
            .find(|(_, n)| n.channel == Some(pid) && n.kind == "updateInitialPrecoDeckSelection")
            .map(|(_, n)| n.args["args"]["_private"]["API"].clone())
            .filter(|d| !d.is_null())
    }

    /// Every die rolled, in order
    pub fn dice(&self) -> Vec<i64> {
        let mut out = vec![];
        for (_, n) in self.all() {
            if n.channel.is_none() && n.kind == "roll" {
                match &n.args["rolls"] {
                    Value::Array(a) => out.extend(a.iter().map(int)),
                    Value::Object(o) => out.extend(o.values().map(int)),
                    v if !v.is_null() => out.push(int(v)),
                    _ => {}
                }
            }
        }
        out
    }

    /// Tumult cards revealed: storm index (1..3) => (card id, rotated)
    pub fn storm(&self) -> BTreeMap<i64, (i64, bool)> {
        let mut out = BTreeMap::new();
        for (_, n) in self.all() {
            if n.kind == "moveStormToken" && n.args["revealed"].is_object() {
                let r = &n.args["revealed"];
                out.insert(int(&n.args["stormIndex"]), (int(&r["cardId"]), r["rotated"].as_bool().unwrap_or(int(&r["rotated"]) != 0)));
            }
        }
        out
    }

    /// (move, card id) each time a card of a player shows up out of its deck, in the replay's
    /// order: the order cards leave the decks in
    pub fn reveals(&self) -> Vec<(i64, i64)> {
        let mut out = vec![];
        let mut seen: std::collections::HashSet<(i64, i64)> = Default::default();
        for (m, n) in self.all() {
            visit_cards(&n.args, &mut |c| {
                let loc = c["location"].as_str().unwrap_or("");
                if loc.starts_with("board-hero") || loc.starts_with("deck") {
                    return;
                }
                let id = int(&c["id"]);
                if seen.insert((m, id)) {
                    out.push((m, id));
                }
            });
        }
        out
    }

    /// Smallest new card / meeple id that shows up in each move: after an undo, where BGA's
    /// counters were (rows an undo removed still consumed their ids)
    pub fn first_new_ids(&self) -> BTreeMap<i64, (Option<i64>, Option<i64>)> {
        let mut seen_cards = std::collections::HashSet::new();
        let mut seen_meeples = std::collections::HashSet::new();
        let mut out: BTreeMap<i64, (Option<i64>, Option<i64>)> = BTreeMap::new();
        for (m, n) in self.all() {
            visit(&n.args, &mut |node| {
                if node.get("id").is_none() || node.get("location").is_none() {
                    return;
                }
                let id = int(&node["id"]);
                let e = out.entry(m).or_default();
                if node.get("properties").is_some() {
                    if seen_cards.insert(id) {
                        e.0 = Some(e.0.map_or(id, |x: i64| x.min(id)));
                    }
                } else if node.get("type").is_some() && seen_meeples.insert(id) {
                    e.1 = Some(e.1.map_or(id, |x: i64| x.min(id)));
                }
            });
        }
        out
    }
}

/// Calls `f` on every object of the tree
pub fn visit(v: &Value, f: &mut dyn FnMut(&serde_json::Map<String, Value>)) {
    match v {
        Value::Object(o) => {
            f(o);
            for c in o.values() {
                visit(c, f);
            }
        }
        Value::Array(a) => {
            for c in a {
                visit(c, f);
            }
        }
        _ => {}
    }
}

/// Calls `f` on every card object (id, location, pId, properties) of the tree
pub fn visit_cards(v: &Value, f: &mut dyn FnMut(&Value)) {
    match v {
        Value::Object(o) => {
            if o.contains_key("id") && o.contains_key("location") && o.contains_key("pId") && o.contains_key("properties") {
                f(v);
            }
            for c in o.values() {
                visit_cards(c, f);
            }
        }
        Value::Array(a) => {
            for c in a {
                visit_cards(c, f);
            }
        }
        _ => {}
    }
}
