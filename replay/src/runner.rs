//! Rebuilds the requests of a BGA replay on the engine. For each request of the replay, every
//! answer the position allows is tried on a copy of the game, and the one whose effect matches
//! what BGA recorded (`sig::diff`) is kept; when several do, the next requests decide.

use crate::bga::{BgaReplay, Move};
use crate::{play, setup, sig};
use altered_ai::legal;
use altered_rules::arena::PLAYER_IDS;
use altered_rules::Game;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// States of the turn confirmation (constants.inc.php)
const ST_CONFIRM_TURN: i64 = 93;
const ST_CONFIRM_PARTIAL_TURN: i64 = 94;
const ST_GENERIC_NEXT_PLAYER: i64 = 97;
/// Preferences (constants.inc.php)
const OPTION_CONFIRM: i64 = 103;
const OPTION_CONFIRM_UNDOABLE: i64 = 104;
const OPTION_PLAYER_UNDO: i64 = 152;
const ST_RESOLVE_STACK: i64 = 90;

/// Requests replayed ahead to tell apart answers that a request alone cannot
const LOOKAHEAD: usize = 6;
/// Candidates tried per request at most
const MAX_CANDIDATES: usize = 400;
/// Backtracking: how many requests back, and how many retries in a whole replay
const BACKTRACK: usize = 40;
const MAX_BACKTRACKS: usize = 400;

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    /// BGA move id
    pub r#move: i64,
    pub player: u32,
    /// Protocol v1 answer (without decision id)
    pub answer: Value,
    /// How it was found: "match", "lookahead", "erased" (an undo removed the request from the log:
    /// only the state it led to is known), "closest" (no answer matched exactly: the best one)
    pub how: &'static str,
    /// Deck cards whose order the request needed (card id => state), set before answering
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub deck_order: BTreeMap<i64, i64>,
    /// AUTO_INCREMENT of cards / meeples set before answering (ids BGA consumed in undone requests)
    #[serde(skip_serializing_if = "Value::is_null")]
    pub next_ids: Value,
    /// Preferences set before answering: (player, preference, value)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub prefs: Vec<(i64, i64, i64)>,
}

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub table: i64,
    pub steps: Vec<Step>,
    /// Requests of the replay (after deck selection)
    pub requests: usize,
    pub replayed: usize,
    /// Where it stopped, when it did not reach the end
    pub divergence: Option<String>,
    pub closest: usize,
    /// Requests played again with another answer
    pub backtracks: usize,
    pub game_over: bool,
    /// The game after the last step
    #[serde(skip)]
    pub game: Option<Game>,
    /// Preferences set before the first step: (player, preference, value)
    #[serde(skip)]
    pub prefs0: Vec<(i64, i64, i64)>,
}

struct Ctx<'a> {
    r: &'a BgaReplay,
    moves: Vec<&'a Move>,
    first_new_ids: BTreeMap<i64, (Option<i64>, Option<i64>)>,
    reveals: Vec<(i64, i64)>,
    /// Whether a request with no exact match may take the closest answer
    near: bool,
    /// Undo preference inferred for the whole game (a player who never confirmed a turn: off)
    undo_default: BTreeMap<i64, i64>,
}

/// The game and what the runner tracks along with it
#[derive(Clone)]
struct State {
    g: Game,
    /// An undo ran: the next new ids the replay shows tell where BGA's counters were
    after_undo: bool,
}

pub fn run(r: &BgaReplay) -> Outcome {
    let mut out = Outcome { table: r.table, steps: vec![], requests: 0, replayed: 0, divergence: None, closest: 0, backtracks: 0, game_over: false, game: None, prefs0: vec![] };
    let g = match setup::create(r) {
        Ok(g) => g,
        Err(e) => {
            out.divergence = Some(format!("setup: {}", e));
            return out;
        }
    };
    // Deck selection ends with setupPlayer: the requests after it
    let start = r.moves.iter().rposition(|m| m.notifs.iter().any(|n| n.kind == "setupPlayer")).map(|i| i + 1).unwrap_or(0);
    // Requests that left nothing in the log but a reflexion time were erased by an undo: kept
    let moves: Vec<&Move> = r.moves[start..].iter().collect();
    let mut undo_default = BTreeMap::new();
    for pid in PLAYER_IDS {
        let confirms = r.moves.iter().any(|m| m.states.iter().any(|s| s.1 == pid as i64 && (s.0 == ST_CONFIRM_TURN || s.0 == ST_CONFIRM_PARTIAL_TURN)));
        undo_default.insert(pid as i64, if confirms { 1 } else { 2 });
    }
    let mut ctx = Ctx { r, moves, first_new_ids: r.first_new_ids(), reveals: r.reveals(), undo_default, near: false };
    out.requests = ctx.moves.len();
    let mut st = State { g, after_undo: false };
    for pid in PLAYER_IDS {
        setup::set_pref(&mut st.g, pid as i64, OPTION_PLAYER_UNDO, ctx.undo_default[&(pid as i64)]);
        // explore.php writes 0 for the preferences the runner cannot infer
        setup::set_pref(&mut st.g, pid as i64, OPTION_CONFIRM, 0);
        setup::set_pref(&mut st.g, pid as i64, OPTION_CONFIRM_UNDOABLE, 0);
        out.prefs0.extend([(pid as i64, OPTION_PLAYER_UNDO, ctx.undo_default[&(pid as i64)]), (pid as i64, OPTION_CONFIRM, 0), (pid as i64, OPTION_CONFIRM_UNDOABLE, 0)]);
    }
    // Exact answers only (with backtracking) first; the closest answers only if that fails
    let first = replay_from(&ctx, st.clone());
    let res = if first.divergence.is_none() {
        first
    } else {
        ctx.near = true;
        let second = replay_from(&ctx, st);
        if second.divergence.is_none() || second.replayed > first.replayed { second } else { first }
    };
    out.steps = res.steps;
    out.replayed = res.replayed;
    out.divergence = res.divergence;
    out.closest = res.closest;
    out.backtracks = res.backtracks;
    out.game_over = res.game_over;
    out.game = res.game;
    out
}

struct Run {
    steps: Vec<Step>,
    replayed: usize,
    divergence: Option<String>,
    closest: usize,
    backtracks: usize,
    game_over: bool,
    game: Option<Game>,
}

fn replay_from(ctx: &Ctx, mut st: State) -> Run {
    let mut out = Run { steps: vec![], replayed: 0, divergence: None, closest: 0, backtracks: 0, game_over: false, game: None };
    // Each request takes the answer `step` prefers. When one cannot be played, the requests
    // before it are played again with their other answers, latest first (a choice the replay
    // does not show, made several requests earlier, only shows later)
    struct Frame {
        i: usize,
        before: State,
        alts: Vec<(State, Vec<Step>)>,
        k: usize,
        expanded: bool,
    }
    let mut frames: Vec<Frame> = vec![];
    let mut i = 0;
    let (mut furthest, mut furthest_state) = (0, st.clone());
    // The answers of the furthest path, kept for the report when the replay diverges
    let mut furthest_steps: Vec<Step> = vec![];
    let mut backtracks = 0;
    while i < ctx.moves.len() && !st.g.is_over() {
        if let Some(res) = step(ctx, &st, i, 0) {
            frames.push(Frame { i, before: st.clone(), alts: vec![res.clone()], k: 0, expanded: false });
            st = res.0;
            i += 1;
            if i > furthest {
                furthest = i;
                furthest_state = st.clone();
                furthest_steps = frames.iter().flat_map(|f| f.alts[f.k].1.iter().cloned()).collect();
            }
            continue;
        }
        // Back to the latest request with another answer, at most BACKTRACK requests back
        let failed = i;
        if std::env::var("DEBUG_BT").is_ok() {
            eprintln!("failed at {}: {}", ctx.moves[i].id, &explain(ctx, &st, i)[..800.min(explain(ctx, &st, i).len())]);
        }
        let mut resumed = false;
        while backtracks < MAX_BACKTRACKS {
            let Some(f) = frames.last_mut() else { break };
            if f.i + BACKTRACK < failed {
                break;
            }
            if !f.expanded {
                f.expanded = true;
                let first = f.alts[0].1.iter().map(|s| s.answer.clone()).collect::<Vec<_>>();
                for alt in alternatives(ctx, &f.before, f.i) {
                    let order = |st: &Vec<Step>| st.first().map(|s| s.deck_order.clone());
                    if alt.1.iter().map(|s| s.answer.clone()).collect::<Vec<_>>() != first || order(&alt.1) != order(&f.alts[0].1) {
                        f.alts.push(alt);
                    }
                }
            }
            if std::env::var("DEBUG_BT").is_ok() {
                eprintln!("backtrack from {} to move {} ({} alternatives, at {}): {:?}", ctx.moves[failed].id, ctx.moves[f.i].id, f.alts.len(), f.k, f.alts.iter().map(|a| a.1.iter().map(|s| s.answer.to_string()).collect::<Vec<_>>()).collect::<Vec<_>>());
            }
            if f.k + 1 < f.alts.len() {
                f.k += 1;
                backtracks += 1;
                st = f.alts[f.k].0.clone();
                i = f.i + 1;
                resumed = true;
                break;
            }
            frames.pop();
        }
        if !resumed {
            out.divergence = Some(explain(ctx, &furthest_state, furthest));
            break;
        }
    }
    for f in &frames {
        out.steps.extend(f.alts[f.k].1.iter().cloned());
    }
    if out.divergence.is_some() {
        out.steps = furthest_steps;
    }
    out.closest = out.steps.iter().filter(|s| s.how == "closest").count();
    out.replayed = if out.divergence.is_some() { furthest } else { frames.last().map(|f| f.i + 1).unwrap_or(0) };
    out.backtracks = backtracks;
    out.game_over = st.g.is_over();
    out.game = Some(st.g);
    out
}

/// Plays request `i` (and, for an ambiguous one, checks the next ones): the state after it and
/// its steps. None: no answer leads to what the replay shows.
fn step(ctx: &Ctx, st: &State, i: usize, depth: usize) -> Option<(State, Vec<Step>)> {
    let m = ctx.moves[i];
    // Exact matches with every order of the cards leaving the decks first, then the closest
    for near_ok in [false, true] {
        if near_ok && (depth > 0 || !ctx.near) {
            break;
        }
        let mut variant = 0;
        loop {
            let mut base = st.clone();
            let prep = prepare(ctx, &mut base, m, variant);
            if let Some(res) = step_with(ctx, &base, &prep, i, depth, near_ok) {
                return Some(res);
            }
            variant += 1;
            if variant >= prep.variants {
                break;
            }
        }
    }
    None
}

/// The answers to request `i` from `base`: those whose effect matches the move (best order
/// first; for a request an undo erased, the sequences that reach its state), and those that come
/// close (sig::close), best first
fn options(base: &State, prep: &Prep, m: &Move, all: bool) -> (Vec<(usize, State, Vec<Step>)>, Vec<(usize, State, Vec<Step>)>) {
    // A request with no notification left: an ordinary one (a choice, a confirmation), or one an
    // undo erased from the log (then only the state it led to is known)
    let erased = m.notifs.is_empty();
    let mut matches: Vec<(usize, State, Vec<Step>)> = vec![];
    let mut near: Vec<(usize, State, Vec<Step>)> = vec![];
    for (pid, a) in candidates(&base.g, m) {
        if erased && a["type"] == "undo" {
            continue; // an undo with nothing left to compare would match anything
        }
        let mut next = base.clone();
        let before = sig::positions(&next.g);
        let Ok(notifs) = play(&mut next.g, pid, &a) else { continue };
        let after = sig::positions(&next.g);
        next.after_undo = next.after_undo || a["type"] == "undo";
        let s = prep.step(m.id, pid, a.clone());
        if sig::diff(&next.g, m, &before, &after, &notifs).is_none() {
            matches.push((sig::order_score(m, &notifs), next, vec![s]));
        } else if !erased && sig::close(m, &notifs) {
            near.push((sig::score(m, &after, &notifs), next, vec![s]));
        }
    }
    if matches.is_empty() && erased {
        // Requests erased by an undo: the shortest sequences of answers that reach the state
        matches = erased_sequences(base, m, prep, all).into_iter().map(|(s, st)| (0, s, st)).collect();
    }
    // The answers whose notifications follow the replay's order first
    matches.sort_by(|a, b| b.0.cmp(&a.0));
    near.sort_by(|a, b| b.0.cmp(&a.0));
    (matches, near)
}

/// Every way to play request `i` from `st` (all deck orders), best first: for backtracking
fn alternatives(ctx: &Ctx, st: &State, i: usize) -> Vec<(State, Vec<Step>)> {
    let m = ctx.moves[i];
    let (mut exact, mut close) = (vec![], vec![]);
    let mut variant = 0;
    loop {
        let mut base = st.clone();
        let prep = prepare(ctx, &mut base, m, variant);
        let (a, b) = options(&base, &prep, m, true);
        exact.extend(a.into_iter().map(|(_, s, st)| (s, st)));
        close.extend(b.into_iter().take(6).map(|(_, s, mut st)| {
            st[0].how = "closest";
            (s, st)
        }));
        variant += 1;
        if variant >= prep.variants {
            break;
        }
    }
    // Close answers only when nothing matches (else every request would have alternatives)
    if exact.is_empty() {
        return close;
    }
    exact
}

fn step_with(ctx: &Ctx, base: &State, prep: &Prep, i: usize, depth: usize, near_ok: bool) -> Option<(State, Vec<Step>)> {
    let m = ctx.moves[i];
    let (matches, near) = options(base, prep, m, false);
    if matches.is_empty() {
        // Nothing matches exactly: the closest answer after which the next requests replay
        // (only at the top: a lookahead takes exact matches only)
        if !near_ok {
            return None;
        }
        for (_, next, mut steps) in near.into_iter().take(6) {
            if replayed_after(ctx, &next, i + 1, depth + 1) >= LOOKAHEAD {
                steps[0].how = "closest";
                return Some((next, steps));
            }
        }
        return None;
    }
    if std::env::var("DEBUG_MOVE").ok().and_then(|v| v.parse::<i64>().ok()) == Some(m.id) {
        eprintln!("move {} depth {} matches {:?}", m.id, depth, matches.iter().map(|x| (x.0, x.2[0].answer.to_string())).collect::<Vec<_>>());
    }
    // Nothing to replay (an erased request whose state is already reached) comes first; the
    // other answers are left to backtracking
    if matches.len() == 1 || depth >= LOOKAHEAD || matches[0].2.is_empty() {
        return matches.into_iter().next().map(|(_, s, st)| (s, st));
    }
    // Ambiguous: keep the answer after which the next requests replay too, else the one after
    // which the most of them do
    let mut best: Option<(usize, State, Vec<Step>)> = None;
    for (_, next, mut steps) in matches {
        let n = replayed_after(ctx, &next, i + 1, depth + 1);
        if let Some(s) = steps.first_mut().filter(|s| s.how == "match") {
            s.how = "lookahead";
        }
        if n >= LOOKAHEAD {
            return Some((next, steps));
        }
        if best.as_ref().is_none_or(|b| n > b.0) {
            best = Some((n, next, steps));
        }
    }
    best.map(|(_, s, st)| (s, st))
}

/// How many of the next requests that show something (requests with no notification check
/// little) replay after this state, up to LOOKAHEAD (also when the game or the replay ends)
fn replayed_after(ctx: &Ctx, st: &State, from: usize, depth: usize) -> usize {
    let mut st = st.clone();
    let mut checked = 0;
    for i in from..(from + 20).min(ctx.moves.len()) {
        if st.g.is_over() || checked >= LOOKAHEAD {
            return LOOKAHEAD;
        }
        match step(ctx, &st, i, depth) {
            Some((next, _)) => st = next,
            None => return checked,
        }
        if !ctx.moves[i].notifs.is_empty() {
            checked += 1;
        }
    }
    LOOKAHEAD
}

/// What the runner set before a request, to put in its steps
struct Prep {
    /// Interleavings of the cards leaving the decks in this request (see `prepare`)
    variants: usize,
    deck_order: BTreeMap<i64, i64>,
    next_ids: Value,
    prefs: Vec<(i64, i64, i64)>,
}

impl Prep {
    fn step(&self, mv: i64, player: u32, answer: Value) -> Step {
        Step { r#move: mv, player, answer, how: "match", deck_order: self.deck_order.clone(), next_ids: self.next_ids.clone(), prefs: self.prefs.clone() }
    }
}

/// Before a request: the move id (global 3, which the shuffle hook reads), the turn confirmation
/// preferences the replay shows, the ids BGA consumed in undone requests, and the decks' order
fn prepare(ctx: &Ctx, st: &mut State, m: &Move, variant: usize) -> Prep {
    st.g.fw_set(3, m.id);
    let mut prep = Prep { deck_order: BTreeMap::new(), next_ids: Value::Null, prefs: vec![], variants: 1 };
    // The turn confirmation depends on two preferences that players may change at any time
    // (Engine::proceed, EngineTrait::stConfirmTurn), and the states show what they were: a player
    // whose turn ended (ST_GENERIC_NEXT_PLAYER) or who handed over (ST_RESOLVE_STACK) with no
    // confirmation had the undo off (or made no choice: then it does not matter), one who stopped
    // in ST_CONFIRM_TURN had the confirmation on, one who went through it had it off, and a partial
    // confirmation needs the undo on. Each state carries the player it is about.
    for pid in PLAYER_IDS {
        let pid = pid as i64;
        let of: Vec<i64> = m.states.iter().filter(|s| s.1 == pid).map(|s| s.0).collect();
        let undo = if of.contains(&ST_CONFIRM_TURN) {
            let confirm = if m.states.last() == Some(&(ST_CONFIRM_TURN, pid)) { 2 } else { 0 };
            prep.prefs.push((pid, OPTION_CONFIRM, confirm));
            1
        } else if of.contains(&ST_CONFIRM_PARTIAL_TURN) {
            1
        } else if of.contains(&ST_GENERIC_NEXT_PLAYER) || of.contains(&ST_RESOLVE_STACK) {
            2
        } else {
            ctx.undo_default[&pid]
        };
        prep.prefs.push((pid, OPTION_PLAYER_UNDO, undo));
    }
    for &(p, pref, v) in &prep.prefs {
        setup::set_pref(&mut st.g, p, pref, v);
    }
    // Ids BGA consumed in requests an undo erased, or for rows created and removed in a request:
    // the next new id the replay shows tells where its counters were
    {
        if let Some(&(cards, meeples)) = ctx.first_new_ids.get(&m.id) {
            let max_card = st.g.cards.values().map(|c| c.id as i64).max().unwrap_or(0);
            let max_meeple = st.g.meeples.values().map(|c| c.id).max().unwrap_or(0);
            let mut ids = json!({});
            if let Some(id) = cards.filter(|&id| id > st.g.next_card_id && id > max_card) {
                st.g.next_card_id = id;
                ids["cards"] = json!(id);
            }
            if let Some(id) = meeples.filter(|&id| id > st.g.next_meeple_id && id > max_meeple) {
                st.g.next_meeple_id = id;
                ids["meeples"] = json!(id);
            }

            if ids.as_object().is_some_and(|o| !o.is_empty()) {
                prep.next_ids = ids;
            }
        }
    }
    // Cards can go back into a deck without a shuffle: before each request, the cards the
    // replay shows leaving a deck next go on its top, in that order. Within the request the order
    // is only known per channel: `variant` picks one interleaving of the channels' lists.
    let mut variants = 1;
    for (pid, lists) in deck_lists(ctx, &st.g, m) {
        let loc = format!("deck-{}", pid);
        let mut deck: Vec<(i64, i64)> = st.g.cards.values().filter(|c| c.location == loc).map(|c| (c.state, c.id as i64)).collect();
        deck.sort();
        let ids: Vec<i64> = deck.iter().map(|d| d.1).collect();
        let orders = interleavings(&lists);
        let now = &orders[(variant / variants) % orders.len()];
        variants *= orders.len();
        let mut known: Vec<i64> = now.clone();
        for &(mv, id) in ctx.reveals.iter() {
            if mv > m.id && ids.contains(&id) && !known.contains(&id) {
                known.push(id);
            }
        }
        if known.is_empty() {
            continue;
        }
        let mut order: Vec<i64> = ids.iter().copied().filter(|id| !known.contains(id)).collect();
        order.extend(known.iter().rev());
        for (state, id) in order.iter().enumerate() {
            let card = st.g.cards.get_mut(id).unwrap();
            if card.state != state as i64 {
                card.state = state as i64;
                prep.deck_order.insert(*id, state as i64);
            }
        }
    }
    prep.variants = variants.min(24);
    prep
}

/// Per player, the cards of their deck the move shows, one list per channel (in that channel's order)
fn deck_lists(ctx: &Ctx, g: &Game, m: &Move) -> Vec<(i64, Vec<Vec<i64>>)> {
    let _ = ctx;
    let mut out = vec![];
    for pid in PLAYER_IDS {
        let loc = format!("deck-{}", pid);
        let in_deck = |id: i64| g.cards.get(&id).is_some_and(|c| c.location == loc);
        let mut lists: BTreeMap<Option<i64>, Vec<i64>> = BTreeMap::new();
        for n in &m.notifs {
            crate::bga::visit_cards(&n.args, &mut |c| {
                let l = c["location"].as_str().unwrap_or("");
                let id = c["id"].as_i64().unwrap_or(0);
                if l.starts_with("board-hero") || l.starts_with("deck") || !in_deck(id) {
                    return;
                }
                let list = lists.entry(n.channel).or_default();
                if !list.contains(&id) {
                    list.push(id);
                }
            });
        }
        // A card shown on several channels counts in the first one only
        let mut seen = HashSet::new();
        let lists: Vec<Vec<i64>> = lists.into_values().map(|l| l.into_iter().filter(|id| seen.insert(*id)).collect::<Vec<_>>()).filter(|l| !l.is_empty()).collect();
        out.push((pid as i64, lists));
    }
    out
}

/// Every merge of the lists that keeps the order within each list (at most 24)
fn interleavings(lists: &[Vec<i64>]) -> Vec<Vec<i64>> {
    let lists: Vec<&Vec<i64>> = lists.iter().filter(|l| !l.is_empty()).collect();
    if lists.len() <= 1 {
        return vec![lists.first().map(|l| (*l).clone()).unwrap_or_default()];
    }
    let mut out = vec![];
    for i in 0..lists.len() {
        let mut rest: Vec<Vec<i64>> = lists.iter().map(|l| (*l).clone()).collect();
        let head = rest[i].remove(0);
        for tail in interleavings(&rest) {
            let mut v = vec![head];
            v.extend(tail);
            if !out.contains(&v) {
                out.push(v);
            }
            if out.len() >= 24 {
                return out;
            }
        }
    }
    out
}

/// The answers of every player who has a decision, the cards the move shows first (so that the
/// right selection is among the candidates kept)
fn candidates(g: &Game, m: &Move) -> Vec<(u32, Value)> {
    let shown: HashSet<i64> = sig::shown(m).keys().filter_map(|p| if let sig::Piece::Card(id) = p { Some(*id) } else { None }).collect();
    let mut out = vec![];
    // A first-day Mana choice taken back (the replay shows the player's choice cleared)
    for n in m.notifs.iter().filter(|n| n.kind == "updateFirstDayManaSelection" && n.args["args"]["_private"]["selection"].is_null()) {
        if let Some(p) = n.channel {
            out.push((p as u32, json!({"type": "act", "action": "actCancelFirstDayManaSelection", "args": []})));
        }
    }
    for pid in g.waiting_for() {
        let Some(mut d) = g.decision(pid) else { continue };
        guide(&mut d, &shown);
        for a in legal::candidates(&d).into_iter().take(MAX_CANDIDATES) {
            out.push((pid, a));
        }
        if d["kind"] != "confirm" {
            out.push((pid, json!({"type": "undo"})));
        }
    }
    out
}

/// Puts the card ids the move shows first in every list of the decision
fn guide(v: &mut Value, shown: &HashSet<i64>) {
    match v {
        Value::Array(a) if a.iter().all(|x| x.is_i64()) && !a.is_empty() => {
            a.sort_by_key(|x| !shown.contains(&x.as_i64().unwrap()));
        }
        Value::Array(a) => a.iter_mut().for_each(|x| guide(x, shown)),
        Value::Object(o) => o.values_mut().for_each(|x| guide(x, shown)),
        _ => {}
    }
}

/// The state (and active player) the replay shows at the end of the move
fn reached(g: &Game, m: &Move) -> bool {
    let Some(&(state, active)) = m.states.last() else { return true };
    altered_rules::framework::state_id(g) == state && (active == 0 || g.fw_get(2) == Some(active))
}

/// Sequences of up to 3 answers that reach the state the replay shows (requests an undo erased)
fn erased_sequences(st: &State, m: &Move, prep: &Prep, all: bool) -> Vec<(State, Vec<Step>)> {
    if reached(&st.g, m) {
        // Nothing to do, or one answer that comes back to the same state (or, when the replay
        // kept no state at all, any answer): alternatives for backtracking
        let mut out = vec![(st.clone(), vec![])];
        for (pid, a) in candidates(&st.g, m) {
            if a["type"] == "undo" {
                continue;
            }
            let mut n = st.clone();
            if play(&mut n.g, pid, &a).is_err() || !reached(&n.g, m) {
                continue;
            }
            let mut stp = prep.step(m.id, pid, a);
            stp.how = "erased";
            n.after_undo = true;
            out.push((n, vec![stp]));
        }
        return out;
    }
    let mut found = vec![];
    let mut frontier: Vec<(State, Vec<Step>)> = vec![(st.clone(), vec![])];
    for _ in 0..3 {
        let mut next_frontier = vec![];
        for (s, steps) in &frontier {
            for (pid, a) in candidates(&s.g, m) {
                if a["type"] == "undo" {
                    continue;
                }
                let mut n = s.clone();
                if play(&mut n.g, pid, &a).is_err() {
                    continue;
                }
                let mut steps = steps.clone();
                let mut stp = prep.step(m.id, pid, a);
                stp.how = "erased";
                steps.push(stp);
                n.after_undo = true;
                if reached(&n.g, m) {
                    if all {
                        next_frontier.push((n.clone(), steps.clone()));
                    }
                    found.push((n, steps));
                } else {
                    next_frontier.push((n, steps));
                }
            }
        }
        // The shortest sequences, or (for backtracking) the longer ones too
        if !found.is_empty() && !all {
            break;
        }
        next_frontier.truncate(200);
        frontier = next_frontier;
    }
    found
}

fn explain(ctx: &Ctx, st: &State, i: usize) -> String {
    let m = ctx.moves[i];
    let mut base = st.clone();
    prepare(ctx, &mut base, m, 0);
    let mut best: Option<(usize, String)> = None;
    let cands = candidates(&base.g, m);
    for (pid, a) in &cands {
        let mut next = base.clone();
        let before = sig::positions(&next.g);
        let res = play(&mut next.g, *pid, a);
        if std::env::var("CANDS").is_ok() {
            if let Ok(ns) = &res {
                eprintln!("CAND {} {} -> {:?}", pid, a, ns.iter().map(|n| format!("{} {}", n["name"].as_str().unwrap_or(""), &n["args"].to_string()[..n["args"].to_string().len().min(120)])).collect::<Vec<_>>());
            }
            if let Some(d) = base.g.decision(*pid) { eprintln!("DECISION {}", &d.to_string()[..d.to_string().len().min(800)]); }
        }
        let (score, why) = match res {
            Err(e) => (0, format!("refused: {}", e)),
            Ok(notifs) => {
                let after = sig::positions(&next.g);
                (sig::score(m, &after, &notifs) + 1, sig::diff(&next.g, m, &before, &after, &notifs).unwrap_or_else(|| "matches, but the next requests do not replay".into()))
            }
        };
        if best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, format!("{} {}: {}", pid, a, why)));
        }
    }
    // Debugging: the answer a golden file (explore.php replay) gives for this move
    let mut golden = String::new();
    if let Some(path) = std::env::var("GOLDEN_DIR").ok().map(|d| format!("{}/{}.json", d, ctx.r.table)) {
        if let Some(data) = std::fs::read(&path).ok().and_then(|b| {
            let mut de = serde_json::Deserializer::from_slice(&b);
            de.disable_recursion_limit();
            <Value as serde::Deserialize>::deserialize(&mut de).ok()
        }) {
            let mut node = &data["tree"];
            while let Some(next) = node["next"].as_array().and_then(|n| n.first()) {
                if node["move"].as_i64() == Some(m.id) {
                    // The zones of the position against the golden's
                    let os = base.g.oracle_state();
                    let mut zones: BTreeMap<String, Vec<(i64, i64)>> = BTreeMap::new();
                    for c in os["cards"].as_array().into_iter().flatten() {
                        zones.entry(format!("{}:{}", c["owner"], c["zone"].as_str().unwrap_or(""))).or_default().push((c["order"].as_i64().unwrap_or(0), c["id"].as_i64().unwrap_or(0)));
                    }
                    for (k, v) in zones.iter_mut() {
                        if k.ends_with(":deck") { v.sort(); } else { v.sort_by_key(|x| x.1); }
                    }
                    for (k, want) in node["state"]["zones"].as_object().into_iter().flatten() {
                        let got: Vec<i64> = zones.get(k).map(|v| v.iter().map(|x| x.1).collect()).unwrap_or_default();
                        let want: Vec<i64> = want.as_array().into_iter().flatten().filter_map(|x| x.as_i64()).collect();
                        if got != want {
                            golden.push_str(&format!("; zone {} golden {:?} / here {:?}", k, want, got));
                        }
                    }
                    golden.push_str(&format!("; marks golden {} ", node["state"]["marks"]));
                    let pid = node["player"].as_u64().unwrap_or(0) as u32;
                    let mut next_g = base.clone();
                    let before = sig::positions(&next_g.g);
                    golden += &match play(&mut next_g.g, pid, &next["answer"]) {
                        Err(e) => format!("; golden {} {}: refused: {}", pid, next["answer"], e),
                        Ok(notifs) => {
                            let after = sig::positions(&next_g.g);
                            format!("; golden {} {}: {}", pid, next["answer"], sig::diff(&next_g.g, m, &before, &after, &notifs).unwrap_or_else(|| "matches".into()))
                        }
                    };
                    break;
                }
                node = &next["node"];
            }
        }
    }
    // Debugging: the expeditions and the meeples on their cards
    if std::env::var("BOARD").is_ok() {
        let mut board = String::new();
        for c in base.g.cards.values().filter(|c| c.location.starts_with("storm") || c.location == "landmark") {
            let tokens: Vec<String> = base.g.meeples.values().filter(|mp| mp.location == format!("card-{}", c.id)).map(|mp| mp.kind.clone()).collect();
            board.push_str(&format!(" {}@{}:{}{:?}", c.id, c.pid, c.location, tokens));
        }
        golden.push_str(&format!("; board{}", board));
        if let (Ok(cond), Ok(card)) = (std::env::var("COND"), std::env::var("CARD")) {
            let mut g = base.g.clone();
            g.begin_request(None).ok();
            let card: i64 = card.parse().unwrap_or(0);
            let r = altered_rules::helpers::conditions::check(&mut g, &json!({"conditions": [cond]}), Some(card), &json!({}));
            golden.push_str(&format!("; condition {:?}", r));
            let played = altered_rules::models::player::get_played_cards(&mut g, 2001, &Value::Null);
            golden.push_str(&format!("; played {:?}", played));
        }
    }
    let kinds: Vec<&str> = m.notifs.iter().map(|n| n.kind.as_str()).collect();
    format!(
        "move {} (request {}/{}), {} candidates, replay {:?} states {:?}; closest: {}{}",
        m.id,
        i + 1,
        ctx.moves.len(),
        cands.len(),
        kinds,
        m.states,
        best.map(|b| b.1).unwrap_or_else(|| "no decision".into()),
        golden
    )
}
