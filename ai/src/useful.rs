//! Plays that do nothing: a card played (from hand or Reserve) whose resolution changes nothing
//! but the card's own place and the Mana Orbs paid for it, e.g. a spell without a valid target.
//!
//! The play is resolved on a copy of the game redealt from the player's point of view, every
//! answer the player may give during the resolution is tried (up to a budget), and the outcome
//! is compared with the position before: the player's view (cards, their stats, boosts and
//! statuses, counts of every zone) and the engine's pending effects (`nextSpellIsFree`, the
//! "next Character" boosts...). The play is useful when one way of resolving it changes
//! something else: a target hit, a card drawn, a hero's power (Afanas's boost after a spell,
//! Moyo's power after a spell of 4 or more), a Unique's trigger, an opponent's reaction...
//! A Character or a Permanent always does something: it lands on the board.

use crate::legal;
use crate::play::Observation;
use altered_rules::core::globals;
use altered_rules::game::PId;
use altered_rules::{framework, Game};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Engine globals holding an effect waiting for a later card or moment: a change to one of them
/// is something the play did
const PENDING_EFFECTS: &[&str] = &[
    "costReduction",
    "nextCharacterBoost",
    "nextCharacterBoostOccurence",
    "nextReserveCharacterBoost",
    "nextCharacterCost3Anchored",
    "nextCharacterAnchored",
    "nextCharacterFleeting",
    "additionalEffect",
    "afterRest",
    "afterNightCleanup",
    "nextSpellIsFree",
    "removeFleetingIfSpellPlayedHand",
    "removeFleetingSpellPlayed",
    "removeFleetingCharacterPlayed",
    "nextTokenAnchored",
    "removeFleetingCharacterStat0Played",
    "removeFleetingSongArtistPlayed",
    "nextCharacterBoostV",
    "globalTough",
    "nextTokenAsleep",
    "nextCharacterInExpeditionBoost",
    "nextCharacterBaseCost3Anchored",
    "nextCharacterAsleep",
    "nextAnimalBoost",
    "nextAnimalBoostOccurence",
    "smokeThemOutArmed",
];

/// Most resolutions tried per play (answers of the player during the resolution)
const BUDGET: usize = 32;
/// Most answers in one resolution
const DEPTH: usize = 12;

/// Turns the filter off (measurements of the AIs without it)
pub static DISABLED: AtomicBool = AtomicBool::new(false);

/// The turn candidates of `d` without the plays that do nothing (the others are kept in order)
pub fn drop_useless_plays(obs: &Observation, d: &Value, cands: Vec<Value>) -> Vec<Value> {
    if d["kind"] != "turn" || !cands.iter().any(|a| is_spell_play(obs, a)) || DISABLED.load(Ordering::Relaxed) {
        return cands;
    }
    let (base, before) = redeal(obs, d);
    cands.into_iter().filter(|a| useful(obs, d, &base, &before, a)).collect()
}

/// Whether the answer `a` to the turn decision `d` plays a card that does nothing (whether or not
/// the filter is on)
pub fn is_useless_play(obs: &Observation, d: &Value, a: &Value) -> bool {
    if d["kind"] != "turn" || !is_spell_play(obs, a) {
        return false;
    }
    let (base, before) = redeal(obs, d);
    !useful(obs, d, &base, &before, a)
}

/// The copy of the game the plays are resolved on: a fixed redeal per decision, so that the check
/// does not use the agent's random stream
fn redeal(obs: &Observation, d: &Value) -> (Game, Snapshot) {
    let seed = d["id"].as_str().unwrap_or("").bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let base = obs.determinize(&mut ChaCha8Rng::seed_from_u64(seed));
    let before = Snapshot::of(&base, obs.me);
    (base, before)
}

/// A play of a Spell: a Character or a Permanent lands on the board, which is always something
fn is_spell_play(obs: &Observation, a: &Value) -> bool {
    a["type"] == "play" && a["cardId"].as_i64().and_then(|id| obs.card_prop(id, "type")).is_some_and(|t| t == "spell")
}

fn useful(obs: &Observation, d: &Value, base: &Game, before: &Snapshot, a: &Value) -> bool {
    if !is_spell_play(obs, a) {
        return true;
    }
    let mut s = base.clone();
    if s.answer(obs.me as u32, &legal::with_id(d, a)).is_err() {
        return true;
    }
    let mut budget = BUDGET;
    does_something(&s, obs.me, a["cardId"].as_i64().unwrap_or(-1), before, 0, &mut budget)
}

/// Whether one way of resolving the play from `g` changes something (true when unsure)
fn does_something(g: &Game, me: PId, card: i64, before: &Snapshot, depth: usize, budget: &mut usize) -> bool {
    let state = framework::state(g).map(|s| s.name).unwrap_or("");
    let waiting = g.waiting_for();
    let done = g.is_over()
        || depth >= DEPTH
        || matches!(state, "chooseAssignment" | "confirmTurn" | "confirmPartialTurn")
        || waiting.first().map(|p| *p as PId) != Some(me);
    if done {
        // Waiting for the opponent elsewhere than at their turn: the play asked them something
        if !g.is_over() && depth < DEPTH && waiting.first().is_some_and(|p| *p as PId != me) && state != "chooseAssignment" {
            if std::env::var("USEFUL_TRACE").is_ok() {
                eprintln!("useful {}: opponent asked in {}", card, state);
            }
            return true;
        }
        return Snapshot::of(g, me).differs(before, card);
    }
    let Ok(Some(d)) = g.try_decision(me as u32) else { return true };
    let cands = legal::candidates(&d);
    if cands.is_empty() {
        return true;
    }
    for a in cands {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        let mut s = g.clone();
        if s.answer(me as u32, &legal::with_id(&d, &a)).is_err() {
            continue;
        }
        if does_something(&s, me, card, before, depth + 1, budget) {
            return true;
        }
    }
    false
}

/// What a play may change: the player's view, flattened (cards keyed by id), and the pending
/// effects
struct Snapshot {
    view: BTreeMap<String, Value>,
    pending: Vec<Value>,
    me: PId,
}

impl Snapshot {
    fn of(g: &Game, me: PId) -> Snapshot {
        let mut g = g.clone();
        let mut view = BTreeMap::new();
        let v = g.board_view(Some(me as u32)).unwrap_or(Value::Null);
        for k in ["cards", "players"] {
            flatten(&v[k], format!(".{}", k), &mut view);
        }
        let pending = PENDING_EFFECTS.iter().map(|n| globals::get(&mut g, n).unwrap_or(Value::Null)).collect();
        Snapshot { view, pending, me }
    }

    /// Whether `self` (after the play of `card`) differs from `before` by more than the play
    /// itself: the card leaving the hand or Reserve for the Reserve or the discard, the Mana
    /// Orbs exhausted to pay, the counts of hand, Reserve, discard and available Mana, and the
    /// positions of cards in their zones
    fn differs(&self, before: &Snapshot, card: i64) -> bool {
        let trace = std::env::var("USEFUL_TRACE").is_ok();
        if self.pending != before.pending {
            if trace {
                for (k, n) in PENDING_EFFECTS.iter().enumerate() {
                    if self.pending[k] != before.pending[k] {
                        eprintln!("useful {}: pending {} {} -> {}", card, n, before.pending[k], self.pending[k]);
                    }
                }
            }
            return true;
        }
        let me = self.me;
        let mine = format!(".players[{}].", me);
        let own = format!(".cards[{}].", card);
        let zone = |s: &Snapshot, id: &str| s.view.get(&format!(".cards[{}].zone", id)).cloned().unwrap_or(Value::Null);
        let keys: std::collections::BTreeSet<&String> = self.view.keys().chain(before.view.keys()).collect();
        for k in keys {
            let (x, y) = (before.view.get(k), self.view.get(k));
            if x == y || k.ends_with(".position") {
                continue;
            }
            if let Some(field) = k.strip_prefix(&own) {
                let from = x.filter(|_| field == "zone").and_then(|v| v.as_str());
                let to = y.filter(|_| field == "zone").and_then(|v| v.as_str());
                match field {
                    "zone" if matches!((from, to), (Some("hand"), Some("reserve" | "discard")) | (Some("reserve"), Some("discard"))) => continue,
                    "zone" => return true,
                    // The card is gone from the board either way: its own state does not count
                    _ => continue,
                }
            }
            if let Some(field) = k.strip_prefix(&mine) {
                if matches!(field, "handCount" | "reserveCount" | "discardCount" | "manaAvailable") {
                    continue;
                }
            }
            // A Mana Orb of the player exhausted (paying)
            if let Some(rest) = k.strip_prefix(".cards[") {
                let id = rest.split(']').next().unwrap_or("");
                if rest.ends_with("].exhausted")
                    && zone(self, id) == "mana"
                    && x == Some(&Value::Bool(false))
                    && y == Some(&Value::Bool(true))
                    && self.view.get(&format!(".cards[{}].owner", id)).and_then(|v| v.as_i64()) == Some(me)
                {
                    continue;
                }
            }
            if trace {
                eprintln!("useful {}: {} {:?} -> {:?}", card, k, x, y);
            }
            return true;
        }
        false
    }
}

fn flatten(v: &Value, path: String, out: &mut BTreeMap<String, Value>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                flatten(x, format!("{}.{}", path, k), out);
            }
        }
        Value::Array(a) if !a.is_empty() && a.iter().all(|x| x.get("id").is_some()) => {
            for x in a {
                flatten(x, format!("{}[{}]", path, x["id"]), out);
            }
        }
        _ => {
            out.insert(path, v.clone());
        }
    }
}
