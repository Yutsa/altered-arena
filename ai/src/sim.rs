//! Simulation copies of a game, as one player may imagine it (determinisation).
//!
//! The cards a player cannot see are dealt again at random among the places they cannot see:
//! the opponent's hand (cards not revealed), deck and Mana, and the player's own deck order.
//! The pool is sorted canonically before the deal, so the copy depends on what is hidden only
//! through the multiset of hidden cards (which follows from the open decklist and what was
//! seen), never through where each hidden card really is. Future dice and the engine's random
//! seed are drawn again too.

use altered_rules::game::{Game, PId};
use rand::seq::SliceRandom;
use rand::{Rng, RngCore};
use serde_json::Value;
use std::sync::Arc;

pub fn other(g: &Game, me: PId) -> PId {
    g.seat_order.iter().copied().find(|p| *p != me).unwrap_or(me)
}

fn revealed(props: &altered_rules::game::PropMap) -> bool {
    match props.get("revealed") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64() != Some(0),
        _ => false,
    }
}

/// Card ids whose content `me` cannot see, by group (contents are dealt within a group)
pub fn hidden_groups(g: &Game, me: PId) -> Vec<Vec<i64>> {
    let opp = other(g, me);
    let opp_deck = format!("deck-{}", opp);
    let my_deck = format!("deck-{}", me);
    let mut theirs = vec![];
    let mut mine = vec![];
    for (id, c) in g.cards.iter() {
        if c.pid == opp && ((c.location == "hand" && !revealed(&c.props)) || c.location == opp_deck || c.location == "mana") {
            theirs.push(*id);
        } else if c.pid == me && c.location == my_deck {
            mine.push(*id);
        }
    }
    vec![theirs, mine]
}

/// Deals again the contents of the cards `ids` (each keeps its id, place and state)
pub fn deal(g: &mut Game, ids: &[i64], rng: &mut impl RngCore) {
    let mut contents: Vec<_> = ids.iter().map(|id| {
        let c = &g.cards[id];
        (c.props.clone(), c.db_props.clone(), c.reload)
    }).collect();
    // Canonical order first: the deal must not depend on where each card was
    contents.sort_by_cached_key(|(_, db, _)| serde_json::to_string(&**db).unwrap_or_default());
    contents.shuffle(rng);
    for (id, (props, db, reload)) in ids.iter().zip(contents) {
        let c = g.cards.get_mut(id).unwrap();
        c.props = props;
        c.db_props = db;
        c.reload = reload;
    }
}

/// A copy of `g` as `me` may imagine it
pub fn determinize(g: &Game, me: PId, rng: &mut impl RngCore) -> Game {
    let mut s = g.clone();
    for group in hidden_groups(g, me) {
        deal(&mut s, &group, rng);
    }
    let used = s.dice_used.min(s.dice.len());
    let mut dice: Vec<i64> = s.dice[..used].to_vec();
    dice.extend((0..200).map(|_| rng.random_range(1..=6)));
    s.dice = Arc::new(dice);
    s.rng_game = rng.random_range(1..i64::MAX / 4);
    s
}
