//! Holding a removal for a better target: a card whose effect destroys an opponent's card (discards
//! it from the board or the Reserve, as Sabotage does) is not played on a weak target when a
//! clearly stronger one will be there at the player's next turn.
//!
//! Only lasting losses count: a Character sent back to Reserve or to hand (Hooked, Avalanche) is
//! a tempo play for this day's Dusk, which holding would give up; and a Character played is worth
//! its place on the board, so Characters are never held.
//!
//! For each turn play of a card that is not a Character and whose effects can discard (a cheap
//! test on its effect trees, so that the other plays cost nothing):
//! 1. the best hit now: the play is resolved on a redealt copy, every answer the player may give
//!    during the resolution is tried, and the hit is the value of the opponent's cards it sent to
//!    the discard pile (`card_value`);
//! 2. the best hit at the next turn: on redealt copies, the player holds the card (ends the turn
//!    and keeps the card in hand or Reserve through the Night), the opponent is played by the fast
//!    policy except at the Night cleanup, where it keeps its most valuable cards, and at the
//!    player's first turn decision of a later day the same card is tried again;
//! 3. the play is left out when the hit at the next turn is clearly better (`MARGIN`) on average.
//!
//! The rollout sees everything the player knows (the opponent's board, who plays first, which
//! cards are Fleeting); the opponent's hidden cards are redealt.

use crate::legal;
use crate::play::Observation;
use crate::search::bot_step;
use altered_rules::core::globals;
use altered_rules::game::PId;
use altered_rules::Game;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Turns the rule off (measurements of the AIs without it)
pub static DISABLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static OFF_HERE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with the rule off on this thread (one agent without it, the other with it)
pub fn without<T>(f: impl FnOnce() -> T) -> T {
    let before = OFF_HERE.with(|c| c.replace(true));
    let r = f();
    OFF_HERE.with(|c| c.set(before));
    r
}

/// Redeals of the hidden cards for the next turn
const COPIES: u64 = 2;
/// Held when the next turn's hit is worth at least `RATIO` x the hit now + `MARGIN`
const RATIO: f64 = 1.25;
const MARGIN: f64 = 0.5;
/// Most resolutions tried per play, most answers in one resolution
const BUDGET: usize = 24;
const DEPTH: usize = 10;
/// Most moves of the rollout to the next turn
const ROLLOUT: usize = 400;

/// How strong a card is, for choosing what to remove: its Hand Cost, its rarity (Rares and Uniques
/// carry stronger abilities), its abilities (an ongoing one, which keeps giving value, counts most)
/// and its statistics
pub fn card_value(g: &Game, id: i64) -> f64 {
    let Some(c) = g.cards.get(&id) else { return 0.0 };
    let num = |k: &str| c.props.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
    let has = |k: &str| c.props.get(k).is_some_and(|v| !v.is_null());
    let rarity = match c.props.get("rarity").and_then(|v| v.as_i64()) {
        Some(2) => 3.0,
        Some(1) => 1.0,
        _ => 0.0,
    };
    let passive = if has("effectPassive") { 2.0 } else { 0.0 };
    let others = ["effectPlayed", "effectHand", "effectReserve", "effectExhaust", "effectActivated"].iter().filter(|k| has(k)).count() as f64 * 0.5;
    num("costHand") + rarity + passive + others + 0.25 * (num("forest") + num("mountain") + num("ocean"))
}

/// The turn candidates of `d` without the removals worth keeping for a better target
pub fn drop_premature_removals(obs: &Observation, d: &Value, cands: Vec<Value>) -> Vec<Value> {
    if d["kind"] != "turn" || DISABLED.load(Ordering::Relaxed) || OFF_HERE.with(|c| c.get()) || !cands.iter().any(|a| a["type"] == "play") {
        return cands;
    }
    let me = obs.me;
    let seed = d["id"].as_str().unwrap_or("").bytes().fold(0x9e3779b97f4a7c15u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let base = obs.determinize(&mut ChaCha8Rng::seed_from_u64(seed));
    let trace = std::env::var("HOLD_TRACE").is_ok();
    // The next turn of each copy, computed once for every card that may be held
    let mut futures: HashMap<i64, Vec<Option<Game>>> = HashMap::new();
    let mut held: Vec<i64> = vec![];
    for a in &cands {
        let Some(card) = a["cardId"].as_i64() else { continue };
        if a["type"] != "play" || held.contains(&card) || obs.card_prop(card, "type") == Some(Value::from("character")) || !may_discard(&base, card) {
            continue;
        }
        let now = hit_after(&base, me, d, a);
        if now <= 0.0 {
            continue;
        }
        let nexts = futures
            .entry(card)
            .or_insert_with(|| (0..COPIES).map(|k| next_turn(&obs.determinize(&mut ChaCha8Rng::seed_from_u64(seed ^ (k + 1))), me, card)).collect());
        let later: f64 = nexts.iter().map(|g| g.as_ref().map(|g| best_play_hit(g, me, card)).unwrap_or(0.0)).sum::<f64>() / COPIES as f64;
        if trace {
            for g in nexts.iter() {
                match g {
                    None => eprintln!("  next turn: none (card lost or game over)"),
                    Some(g) => {
                        let opp: Vec<String> = g
                            .cards
                            .iter()
                            .filter(|(_, c)| c.pid != me && c.pid > 0 && c.location == "reserve")
                            .map(|(id, _)| format!("{} {:.1}", crate::positions::card_name(g, &Value::from(*id)), card_value(g, *id)))
                            .collect();
                        eprintln!(
                            "  next turn: card at {}, best hit {:.2}, opponent Reserve: {}",
                            g.cards[&card].location,
                            best_play_hit(g, me, card),
                            opp.join(", ")
                        );
                    }
                }
            }
        }
        if trace {
            eprintln!("hold {} ({}): hit now {:.2}, next turn {:.2}", card, crate::positions::card_name(&base, &a["cardId"]), now, later);
        }
        if later >= RATIO * now + MARGIN {
            held.push(card);
            if trace {
                eprintln!(
                    "HELD {} day {}: now {:.2} later {:.2}",
                    crate::positions::card_name(&base, &a["cardId"]),
                    globals::get_int(&mut base.clone(), "day").unwrap_or(0),
                    now,
                    later
                );
            }
        }
    }
    cands.into_iter().filter(|a| !(a["type"] == "play" && a["cardId"].as_i64().is_some_and(|c| held.contains(&c)))).collect()
}

/// Effect fields of a card (its texts' effect trees)
const EFFECTS: &[&str] = &["effectPlayed", "effectHand", "effectReserve", "effectPassive", "effectExhaust", "effectActivated"];

/// Whether the card's own effects can send a card to the discard pile (a `Discard` action to the
/// discard pile in one of its effect trees): the other cards are not simulated. A discard caused by another card's
/// trigger is not seen.
fn may_discard(g: &Game, card: i64) -> bool {
    fn walk(v: &Value) -> bool {
        match v {
            // A Discard elsewhere than the discard pile (Avalanche: to Reserve) does not count
            Value::Object(m) => {
                let to_pile = m.get("args").and_then(|a| a.get("destination")).and_then(|d| d.as_str()).is_none_or(|d| d == "discard");
                (m.get("action").and_then(|a| a.as_str()).is_some_and(|a| a.starts_with("Discard")) && to_pile) || m.values().any(walk)
            }
            Value::Array(a) => a.iter().any(walk),
            _ => false,
        }
    }
    g.cards.get(&card).is_some_and(|c| EFFECTS.iter().any(|k| c.props.get(k).is_some_and(walk)))
}

/// The best hit of playing `card` at the turn decision of `g` (0 when it cannot be played)
fn best_play_hit(g: &Game, me: PId, card: i64) -> f64 {
    let Ok(Some(d)) = g.try_decision(me as u32) else { return 0.0 };
    legal::candidates(&d).iter().filter(|a| a["type"] == "play" && a["cardId"].as_i64() == Some(card)).map(|a| hit_after(g, me, &d, a)).fold(0.0, f64::max)
}

/// The best hit of answering `a` to `d` on `g`, over the player's answers during the resolution
fn hit_after(g: &Game, me: PId, d: &Value, a: &Value) -> f64 {
    let before: HashMap<i64, String> = opponent_cards(g, me);
    let mut s = g.clone();
    if s.answer(me as u32, &legal::with_id(d, a)).is_err() {
        return 0.0;
    }
    let mut budget = BUDGET;
    best_hit(&s, me, &before, 0, &mut budget)
}

/// The places of the opponent's cards (outside their deck)
fn opponent_cards(g: &Game, me: PId) -> HashMap<i64, String> {
    g.cards.iter().filter(|(_, c)| c.pid != me && c.pid > 0 && !c.location.starts_with("deck")).map(|(id, c)| (*id, c.location.clone())).collect()
}

fn best_hit(g: &Game, me: PId, before: &HashMap<i64, String>, depth: usize, budget: &mut usize) -> f64 {
    let state = altered_rules::framework::state(g).map(|s| s.name).unwrap_or("");
    let waiting = g.waiting_for();
    if g.is_over()
        || depth >= DEPTH
        || matches!(state, "chooseAssignment" | "confirmTurn" | "confirmPartialTurn")
        || waiting.first().map(|p| *p as PId) != Some(me)
    {
        // The opponent's cards sent to the discard pile (a lasting loss)
        return before
            .iter()
            .filter(|(id, loc)| *loc != "discard" && g.cards.get(*id).is_some_and(|c| c.location == "discard"))
            .map(|(id, _)| card_value(g, *id))
            .sum();
    }
    let Ok(Some(d)) = g.try_decision(me as u32) else { return 0.0 };
    let mut best = 0.0f64;
    for a in legal::candidates(&d) {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let mut s = g.clone();
        if s.answer(me as u32, &legal::with_id(&d, &a)).is_ok() {
            best = best.max(best_hit(&s, me, before, depth + 1, budget));
        }
    }
    best
}

/// The game at the player's first turn decision of a later day, the player holding `card`: it
/// ends its turn now, keeps the card through every decision (Mana, Night cleanup) when it can, and
/// the opponent plays the fast policy but keeps its most valuable cards at the Night cleanup.
/// None: the game ends first or the card was lost.
fn next_turn(g: &Game, me: PId, card: i64) -> Option<Game> {
    let mut g = g.clone();
    let day = |g: &mut Game| globals::get_int(g, "day").unwrap_or(0);
    let start = day(&mut g);
    let kept = |g: &Game| g.cards.get(&card).is_some_and(|c| c.location == "hand" || c.location == "reserve");
    for _ in 0..ROLLOUT {
        if g.is_over() || !kept(&g) {
            return None;
        }
        let p = *g.waiting_for().first()? as PId;
        if p != me {
            // The opponent keeps its best cards at the Night cleanup (the fast policy may throw a
            // strong card away); the fast policy otherwise
            if let Ok(Some(d)) = g.try_decision(p as u32) {
                if d["kind"] == "cleanup" {
                    let value = |a: &Value| -> f64 {
                        a["reserve"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .chain(a["landmarks"].as_array().into_iter().flatten())
                            .filter_map(|id| id.as_i64())
                            .map(|id| card_value(&g, id))
                            .sum()
                    };
                    let mut cands: Vec<(f64, Value)> = legal::candidates(&d).into_iter().map(|a| (value(&a), a)).collect();
                    // A cleanup answer lists the cards discarded: the least valuable ones
                    cands.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some(s) = cands.iter().find_map(|(_, a)| {
                        let mut s = g.clone();
                        s.answer(p as u32, &legal::with_id(&d, a)).is_ok().then_some(s)
                    }) {
                        g = s;
                        continue;
                    }
                }
            }
            if !bot_step(&mut g, p) {
                return None;
            }
            continue;
        }
        let d = g.try_decision(me as u32).ok()??;
        if d["kind"] == "turn" && day(&mut g) > start {
            return Some(g);
        }
        let cands = legal::candidates(&d);
        // Turn: end it (the card is held); other decisions: the first answer that keeps the card
        let order: Vec<&Value> = if d["kind"] == "turn" {
            cands.iter().filter(|a| a["type"] == "endTurn" || a["type"] == "pass").chain(cands.iter()).collect()
        } else {
            cands.iter().collect()
        };
        let mut done = false;
        let mut fallback: Option<Game> = None;
        for a in order {
            let mut s = g.clone();
            if s.answer(me as u32, &legal::with_id(&d, a)).is_ok() {
                if kept(&s) {
                    g = s;
                    done = true;
                    break;
                }
                fallback.get_or_insert(s);
            }
        }
        if !done {
            g = fallback?;
        }
    }
    None
}
