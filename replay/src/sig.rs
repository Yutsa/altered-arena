//! Whether a request had the effect BGA recorded. A replay's notifications show cards and meeples
//! (id and location) as they are after each step; the engine's request must leave every one of
//! them there, move nothing else, and go through the same game states.

use crate::bga::Move;
use altered_rules::Game;
use serde_json::Value;
use std::collections::BTreeMap;

fn int(v: &Value) -> i64 {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Piece {
    Card(i64),
    Meeple(i64),
}

/// Every place the replay shows each piece at in this move. Packets of different channels are
/// not in chronological order, so which one is last is not known.
pub fn shown(m: &Move) -> BTreeMap<Piece, Vec<String>> {
    let mut out: BTreeMap<Piece, Vec<String>> = BTreeMap::new();
    for n in &m.notifs {
        // Cards named by id only (cleanupCards): somewhere the replay does not say
        for id in n.args["cardIds"].as_array().into_iter().flatten().map(int) {
            let locs = out.entry(Piece::Card(id)).or_default();
            if !locs.iter().any(|l| l == "*") {
                locs.push("*".into());
            }
        }
        crate::bga::visit(&n.args, &mut |o| {
            let (Some(id), Some(loc)) = (o.get("id"), o.get("location").and_then(|l| l.as_str())) else { return };
            let piece = if o.contains_key("properties") {
                Piece::Card(int(id))
            } else if o.contains_key("type") {
                Piece::Meeple(int(id))
            } else {
                return;
            };
            // Where, and on whose side (a token or a card can change sides); a meeple's state
            // (a terrain marker's region) too
            // An exhausted card (tapped) is a different position
            let tapped = o.get("properties").and_then(|p| p.get("tapped")).is_some_and(|t| t.as_bool() == Some(true) || int(t) == 1);
            let loc = if tapped { format!("{}!", loc) } else { loc.to_string() };
            let loc = loc.as_str();
            let loc = match (&piece, o.get("state")) {
                (Piece::Meeple(_), Some(st)) => format!("{}#{}:{}", loc, int(st), o.get("type").and_then(|t| t.as_str()).unwrap_or("")),
                _ => loc.to_string(),
            };
            let loc = match o.get("pId").map(int) {
                Some(p) if p != 0 => format!("{}@{}", loc, p),
                _ => loc,
            };
            let locs = out.entry(piece).or_default();
            if !locs.iter().any(|l| *l == loc) {
                locs.push(loc);
            }
        });
    }
    out
}

/// Location of every piece of a game
pub fn positions(g: &Game) -> BTreeMap<Piece, String> {
    let mut out = BTreeMap::new();
    let at = |loc: &str, p: i64| if p != 0 { format!("{}@{}", loc, p) } else { loc.to_string() };
    for c in g.cards.values() {
        let tapped = c.props.get("tapped").is_some_and(|t| t.as_bool() == Some(true) || int(t) == 1);
        let loc = if tapped { format!("{}!", c.location) } else { c.location.clone() };
        out.insert(Piece::Card(c.id as i64), at(&loc, c.pid));
    }
    for m in g.meeples.values() {
        let state = m.state.parse::<i64>().unwrap_or(0);
        out.insert(Piece::Meeple(m.id), at(&format!("{}#{}:{}", m.location, state, m.kind), m.pid.parse().unwrap_or(0)));
    }
    out
}

/// States the engine went through in a request (its gameStateChange notifications)
pub fn engine_states(notifs: &[Value]) -> Vec<i64> {
    notifs.iter().filter(|n| n["name"] == "gameStateChange").map(|n| int(&n["args"]["id"])).collect()
}

/// None when the request (engine positions before and after, its notifications) matches the move
/// Global firstDayManaSelection: the cards a player chose
fn mana_selection(g: &Game, pid: i64) -> Vec<i64> {
    let v = g.globals.get("firstDayManaSelection").map(|v| (**v).clone()).unwrap_or(Value::Null);
    let v = match v {
        Value::String(s) => serde_json::from_str(&s).unwrap_or(Value::Null),
        v => v,
    };
    v[pid.to_string()].as_array().map(|a| a.iter().map(int).collect()).unwrap_or_default()
}

/// Normal form of a value for comparing PHP outputs: numeric strings as numbers, empty arrays as
/// empty objects, objects with sorted keys
fn norm(v: &Value) -> Value {
    match v {
        Value::String(s) => s.parse::<i64>().map(Value::from).unwrap_or_else(|_| v.clone()),
        Value::Bool(b) => Value::from(*b as i64),
        Value::Array(a) if a.is_empty() => Value::Object(Default::default()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        Value::Object(o) => {
            let mut m: Vec<(String, Value)> = o.iter().map(|(k, v)| (k.clone(), norm(v))).collect();
            m.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(m.into_iter().collect())
        }
        v => v.clone(),
    }
}

/// The values Notifications::updateIfNeeded sends (updateInformations) as the game is now
fn listener(g: &mut Game, name: &str) -> Option<Value> {
    use altered_rules::managers::players;
    match name {
        "biomes" => players::get_biome_totals(g).ok(),
        "movements" => players::compute_storm(g, false).ok(),
        "blockedExpeditions" => players::get_blocked_expeditions(g).ok(),
        "powersBlockedExpeditions" => players::get_powers_blocked_expeditions(g).ok(),
        "defenders" => players::get_defenders(g, true).ok(),
        "reserveSlots" => players::get_reserve_slots(g).ok(),
        "landmarkSlots" => players::get_landmark_slots(g).ok(),
        _ => None,
    }
}

/// The last value of each listener the move sent must be the engine's at the end of the request
fn listeners_diff(g: &Game, m: &Move) -> Option<String> {
    let mut last: BTreeMap<String, Value> = BTreeMap::new();
    for n in m.notifs.iter().filter(|n| n.kind == "updateInformations" && n.channel.is_none()) {
        for (k, v) in n.args.as_object().into_iter().flatten() {
            last.insert(k.clone(), v.clone());
        }
    }
    if last.is_empty() {
        return None;
    }
    let mut g = g.clone();
    g.begin_request(None).ok()?;
    for (k, want) in last {
        let Some(got) = listener(&mut g, &k) else { continue };
        if norm(&want) != norm(&got) {
            return Some(format!("{}: bga {} / engine {}", k, &want.to_string()[..want.to_string().len().min(200)], &got.to_string()[..got.to_string().len().min(200)]));
        }
    }
    None
}

pub fn diff(g: &Game, m: &Move, before: &BTreeMap<Piece, String>, after: &BTreeMap<Piece, String>, notifs: &[Value]) -> Option<String> {
    let want_states: Vec<i64> = m.states.iter().map(|s| s.0).collect();
    let got_states = engine_states(notifs);
    if want_states != got_states {
        return Some(format!("states: bga {:?} / engine {:?}", want_states, got_states));
    }
    pieces_diff(g, m, before, after).or_else(|| targets_diff(m, notifs)).or_else(|| values_diff(m, notifs)).or_else(|| mana_diff(g, m)).or_else(|| listeners_diff(g, m))
}

/// Numbers the notifications carry (a die selected, a player's Mana): every one BGA sent on the
/// table channel must be among the engine's
const VALUES: &[&str] = &["dieValue"];

/// A player's Mana orbs: the last totalMana the move shows for them must be the number of cards
/// in their Mana zone
fn mana_diff(g: &Game, m: &Move) -> Option<String> {
    let mut last: BTreeMap<i64, i64> = BTreeMap::new();
    for n in m.notifs.iter().filter(|n| n.channel.is_none()) {
        if let (Some(t), Some(p)) = (n.args.get("totalMana").filter(|v| !v.is_null()), n.args.get("player_id")) {
            last.insert(int(p), int(t));
        }
    }
    // Cards the move shows going to Mana after the last total was sent may add to it
    let into_mana: BTreeMap<i64, i64> = {
        let mut out = BTreeMap::new();
        for n in &m.notifs {
            crate::bga::visit_cards(&n.args, &mut |c| {
                if c["location"].as_str() == Some("mana") {
                    *out.entry(int(&c["pId"])).or_insert(0) += 1;
                }
            });
        }
        out
    };
    for (p, want) in last {
        let got = g.cards.values().filter(|c| c.location == "mana" && c.pid == p).count() as i64;
        if got < want || got > want + into_mana.get(&p).copied().unwrap_or(0) {
            return Some(format!("Mana of {}: bga {} / engine {}", p, want, got));
        }
    }
    None
}

fn values_diff(m: &Move, notifs: &[Value]) -> Option<String> {
    let collect = |args: &Value, out: &mut Vec<(String, i64)>| {
        for k in VALUES {
            if let Some(v) = args.get(*k).filter(|v| !v.is_null()) {
                out.push((k.to_string(), int(v)));
            }
        }
    };
    let mut want = vec![];
    for n in m.notifs.iter().filter(|n| n.channel.is_none()) {
        collect(&n.args, &mut want);
    }
    let mut got = vec![];
    for n in notifs {
        collect(&n["args"], &mut got);
    }
    for w in &want {
        match got.iter().position(|g| g == w) {
            Some(i) => {
                got.remove(i);
            }
            None => return Some(format!("value {} = {}: not in the engine's {:?}", w.0, w.1, got)),
        }
    }
    None
}

/// Targeting moves no card: the cards each targetCards names must be the same, in order
fn targets_diff(m: &Move, notifs: &[Value]) -> Option<String> {
    let ids = |v: &Value| -> Vec<i64> {
        let mut out: Vec<i64> = match v {
            Value::Object(o) => o.values().map(|c| int(&c["id"])).collect(),
            Value::Array(a) => a.iter().map(|c| if c.is_object() { int(&c["id"]) } else { int(c) }).collect(),
            _ => vec![],
        };
        out.sort();
        out
    };
    let want: Vec<Vec<i64>> = m.notifs.iter().filter(|n| n.kind == "targetCards" && n.channel.is_none()).map(|n| ids(&n.args["cards"])).collect();
    let got: Vec<Vec<i64>> = notifs.iter().filter(|n| n["name"] == "targetCards").map(|n| ids(&n["args"]["cardIds"])).collect();
    (want != got).then(|| format!("targets: bga {:?} / engine {:?}", want, got))
}

/// Whether the request could be the move although it does not match it exactly: BGA's log can
/// lose states (an undo erases notifications, among them state changes) and show a card where it
/// was when the notification was built. Same last state, the replay's states in the engine's.
pub fn close(m: &Move, notifs: &[Value]) -> bool {
    let want: Vec<i64> = m.states.iter().map(|s| s.0).collect();
    same_states(&want, &engine_states(notifs))
}

fn pieces_diff(g: &Game, m: &Move, before: &BTreeMap<Piece, String>, after: &BTreeMap<Piece, String>) -> Option<String> {
    // A first-day Mana choice moves no card: the choice stored for the player must be the one shown
    for n in m.notifs.iter().filter(|n| n.kind == "updateFirstDayManaSelection") {
        let mut want: Vec<i64> = n.args["args"]["_private"]["selection"].as_array().map(|a| a.iter().map(int).collect()).unwrap_or_default();
        let mut got: Vec<i64> = n.channel.map(|p| mana_selection(g, p)).unwrap_or_default();
        want.sort();
        got.sort();
        if want != got {
            return Some(format!("first-day Mana of {:?}: bga {:?} / engine {:?}", n.channel, want, got));
        }
    }
    let shown = shown(m);
    // Meeples: their ids drift from BGA's after an undo (BGA's counter went on), so only where the
    // engine put meeples is checked: on cards or places the replay shows meeples at
    let bare = |l: &str| l.split('@').next().unwrap_or("").to_string();
    let meeple_places: std::collections::HashSet<String> =
        shown.iter().filter(|(p, _)| matches!(p, Piece::Meeple(_))).flat_map(|(_, ls)| ls.iter().map(|l| bare(l))).collect();
    for (p, loc) in after {
        if matches!(p, Piece::Meeple(_)) && before.get(p) != Some(loc) && !meeple_places.contains(&bare(loc)) {
            return Some(format!("{:?}: engine moved it to {} (no meeple there in the replay)", p, loc));
        }
    }
    for (p, locs) in &shown {
        if matches!(p, Piece::Meeple(_)) {
            continue;
        }
        match after.get(p) {
            Some(l) if locs.contains(l) => {}
            Some(_) if locs.iter().any(|l| l == "*") => {}
            // Removed by the request (a token leaving play, a boost spent): BGA shows where it went
            None => {}
            // Revealed from hand or deck: a place of the client, not of the game
            Some(_) if locs.iter().all(|l| l.starts_with("reveal-")) => {}
            // Shown with no owner (a meeple of the board)
            Some(l) if locs.iter().any(|w| !w.contains('@') && l.split('@').next() == Some(w.as_str())) => {}
            Some(l) => return Some(format!("{:?}: bga {:?} / engine {}", p, locs, l)),
        }
    }
    for (p, loc) in after {
        let from = before.get(p);
        // Exhausting or readying is not always notified (Mana spent to pay a cost)
        let plain = |l: &str| l.replace('!', "");
        if from.is_some_and(|f| plain(f) == plain(loc)) {
            continue;
        }
        // A revealed card going back where it was is not notified
        let unrevealed = loc.starts_with("hand") && from.is_some_and(|f| f.starts_with("reveal-") || f.starts_with("deck"));
        if from != Some(loc) && !shown.contains_key(p) && !unrevealed && !matches!(p, Piece::Meeple(_)) {
            return Some(format!("{:?}: engine moved it to {} (not in the replay)", p, loc));
        }
    }
    None
}

/// The states BGA kept for a request: the last one, and those before it unless an undo erased
/// them from the log, in the engine's order
fn same_states(want: &[i64], got: &[i64]) -> bool {
    if want.is_empty() || got.is_empty() {
        return want.len() == got.len();
    }
    if want.last() != got.last() {
        return false;
    }
    let mut it = got.iter();
    want.iter().all(|w| it.any(|g| g == w))
}

/// How much of the move a request got right (to rank candidates that do not match)
pub fn score(m: &Move, after: &BTreeMap<Piece, String>, notifs: &[Value]) -> usize {
    let want_states: Vec<i64> = m.states.iter().map(|s| s.0).collect();
    let states = want_states.iter().zip(engine_states(notifs)).take_while(|(a, b)| *a == b).count();
    states + shown(m).iter().filter(|(p, l)| after.get(p).is_some_and(|a| l.contains(a))).count()
}

/// Card ids in the order the table channel first mentions them (BGA: card objects)
pub fn bga_order(m: &Move) -> Vec<i64> {
    let mut out = vec![];
    for n in m.notifs.iter().filter(|n| n.channel.is_none()) {
        crate::bga::visit(&n.args, &mut |o| {
            if o.contains_key("properties") {
                if let Some(id) = o.get("id").map(int) {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        });
    }
    out
}

/// The same for the engine's notifications (they name cards by id)
pub fn engine_order(notifs: &[Value]) -> Vec<i64> {
    let mut out = vec![];
    for n in notifs.iter().filter(|n| n.get("target").is_none_or(|t| t.is_null())) {
        if let Some(o) = n["args"].as_object() {
            for (k, v) in o {
                let ids: Vec<i64> = match v {
                    Value::Array(a) if k.to_lowercase().contains("card") => a.iter().filter_map(|x| x.as_i64()).collect(),
                    v if k.to_lowercase().starts_with("card") => v.as_i64().into_iter().collect(),
                    _ => vec![],
                };
                for id in ids {
                    if id > 100 && !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
    }
    out
}

/// Longest common subsequence of the two orders: how well a request follows the replay's order
pub fn order_score(m: &Move, notifs: &[Value]) -> usize {
    let (a, b) = (bga_order(m), engine_order(notifs));
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..a.len() {
        for j in 0..b.len() {
            dp[i + 1][j + 1] = if a[i] == b[j] { dp[i][j] + 1 } else { dp[i][j + 1].max(dp[i + 1][j]) };
        }
    }
    dp[a.len()][b.len()]
}
