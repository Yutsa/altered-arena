//! The engine's own replay format: what the arena needs to play a BGA game again, without the
//! BGA log and without any search. Converted once from a BGA replay (`runner::run`), it replays
//! in milliseconds, and its last line checks that the game ends where BGA's did.

use crate::bga::BgaReplay;
use crate::runner::{Outcome, Step};
use crate::{play, setup, sig};
use altered_rules::game::ShuffleHook;
use altered_rules::{Game, PhpError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeReplay {
    pub format: u32,
    /// BGA table id
    pub table: i64,
    /// Player names, in seat order (arena ids 2001, 2002)
    pub names: Vec<String>,
    /// The custom decks the players confirmed (actConfirmAPIDeck), in seat order
    pub decks: Vec<Value>,
    /// Every die rolled, in order
    pub dice: Vec<i64>,
    /// Tumult cards revealed: storm index => (card, rotated)
    pub storm: BTreeMap<i64, (i64, bool)>,
    /// The order of each deck shuffle that BGA's order decided (ids after the shuffle), in order
    pub shuffles: Vec<Vec<i64>>,
    /// Preferences before the first step: (player, preference, value)
    pub prefs: Vec<(i64, i64, i64)>,
    pub steps: Vec<NativeStep>,
    pub end: End,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeStep {
    /// BGA move id
    pub m: i64,
    /// Player
    pub p: u32,
    /// Protocol v1 answer (or an `act` request the protocol has no answer for)
    pub a: Value,
    /// Deck cards put in the order the replay showed them leaving (card => state)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub o: BTreeMap<i64, i64>,
    /// Card / meeple counters BGA had moved on (undone requests)
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub i: Value,
    /// Preferences set before answering: (player, preference, value)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub f: Vec<(i64, i64, i64)>,
}

/// Where the game ends: its state, whether it is over, and a digest of every card's and meeple's
/// place
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct End {
    pub state: i64,
    pub over: bool,
    pub positions: String,
}

pub fn end_of(g: &Game) -> End {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", sig::positions(g)).hash(&mut h);
    End { state: altered_rules::framework::state_id(g), over: g.is_over(), positions: format!("{:016x}", h.finish()) }
}

/// The native replay of a BGA replay that `runner::run` played to the end
pub fn from_outcome(r: &BgaReplay, o: &Outcome) -> Option<NativeReplay> {
    if o.divergence.is_some() {
        return None;
    }
    let g = o.game.as_ref()?;
    let shuffles = match &g.shuffle_hook {
        ShuffleHook::Revealed { applied, .. } => applied.clone(),
        _ => vec![],
    };
    let decks = altered_rules::arena::PLAYER_IDS.iter().map(|p| r.api_deck(*p as i64).unwrap_or(Value::Null)).collect();
    Some(NativeReplay {
        format: FORMAT,
        table: r.table,
        names: r.names.clone(),
        decks,
        dice: r.dice(),
        storm: r.storm(),
        shuffles,
        prefs: o.prefs0.clone(),
        steps: o.steps.iter().map(native_step).collect(),
        end: end_of(g),
    })
}

fn native_step(s: &Step) -> NativeStep {
    NativeStep { m: s.r#move, p: s.player, a: s.answer.clone(), o: s.deck_order.clone(), i: s.next_ids.clone(), f: s.prefs.clone() }
}

/// The game before the first step
pub fn start(n: &NativeReplay) -> Result<Game, PhpError> {
    let hook = ShuffleHook::Injected(n.shuffles.iter().cloned().collect());
    let mut g = setup::create_with(n.table, &n.decks, &n.dice, &n.storm, hook)?;
    for &(p, pref, v) in &n.prefs {
        setup::set_pref(&mut g, p, pref, v);
    }
    Ok(g)
}

/// Plays one step
pub fn apply(g: &mut Game, s: &NativeStep) -> Result<(), PhpError> {
    g.fw_set(3, s.m);
    for &(p, pref, v) in &s.f {
        setup::set_pref(g, p, pref, v);
    }
    if let Some(id) = s.i["cards"].as_i64() {
        g.next_card_id = id;
    }
    if let Some(id) = s.i["meeples"].as_i64() {
        g.next_meeple_id = id;
    }
    for (id, state) in &s.o {
        if let Some(c) = g.cards.get_mut(id) {
            c.state = *state;
        }
    }
    play(g, s.p, &s.a).map(|_| ())
}

/// Plays the whole replay, calling `each` after the start and after every step; checks the end
pub fn replay(n: &NativeReplay, mut each: impl FnMut(&mut Game, Option<&NativeStep>)) -> Result<Game, String> {
    let mut g = start(n).map_err(|e| format!("start: {}", e))?;
    each(&mut g, None);
    for (k, s) in n.steps.iter().enumerate() {
        apply(&mut g, s).map_err(|e| format!("step {} (move {}) {}: {}", k, s.m, s.a, e))?;
        each(&mut g, Some(s));
    }
    let end = end_of(&g);
    if end != n.end {
        return Err(format!("the game ends elsewhere than on BGA: {:?} / {:?}", end, n.end));
    }
    Ok(g)
}
