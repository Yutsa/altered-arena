//! Cards whose effect depends on the Base Cost of their target (Angry Crowd: a Manaseed if it was
//! {2} or less; Partial Referee: its controller draws if it was {3} or more) ask for the target
//! right away, then read its Base Cost (Reserve Cost if Fleeting, else Hand Cost) before it is
//! discarded. They used to ask first which kind of target the player would pick (an XOR of two
//! Target actions, one per cost range). Played on the recorded rich board of each card.

use altered_rules::models::card;
use altered_rules::Game;
use serde_json::{json, Value};

mod common;
use common::rich as board;

fn answer(g: &mut Game, player: u32, mut a: Value) {
    let d = g.decision(player).expect("a decision");
    a["decisionId"] = d["id"].clone();
    g.answer(player, &a).unwrap_or_else(|e| panic!("answer {} refused: {}", a, e));
}

/// Base Cost of a card in play: Reserve Cost if Fleeting, else Hand Cost
fn base_cost(g: &mut Game, id: i64) -> i64 {
    let field = if card::has_token(g, id, "fleeting").unwrap() { "costReserve" } else { "costHand" };
    card::get(g, id, field).unwrap().as_i64().unwrap()
}

fn count(g: &Game, pid: i64, location: &str, name: Option<&str>) -> usize {
    g.cards.values().filter(|c| c.pid == pid && c.location == location && name.is_none_or(|n| c.props.get("name").and_then(Value::as_str) == Some(n))).count()
}

/// Plays the card; the next decision must be the choice of the target among cards of both cost
/// ranges. Returns the game at that point and the targets.
fn play_then_targets(class: &str, threshold: i64) -> Option<(Game, Vec<i64>)> {
    let (mut g, played) = board(class)?;
    let turn = g.decision(2001).expect("turn");
    let destination = turn["play"].as_array().unwrap().iter().find(|p| p["cardId"] == json!(played)).expect("playable")["destinations"][0].clone();
    answer(&mut g, 2001, json!({"type": "play", "cardId": played, "destination": destination}));
    let d = g.decision(2001).expect("a decision after playing");
    assert_eq!(d["kind"], "select", "{}: the target is asked first, not a choice ({})", class, d);
    let targets: Vec<i64> = d["cardIds"].as_array().unwrap().iter().filter_map(Value::as_i64).collect();
    let costs: Vec<i64> = targets.iter().map(|t| base_cost(&mut g, *t)).collect();
    assert!(
        costs.iter().any(|c| *c < threshold) && costs.iter().any(|c| *c >= threshold),
        "{}: targets of both cost ranges in one choice ({:?})",
        class,
        costs
    );
    Some((g, targets))
}

/// Answers every decision left until the turn: the first option of a select (the token's
/// location...), Pass on a reaction of another card
fn finish(g: &mut Game) {
    for _ in 0..10 {
        let Some(d) = g.decision(2001) else { return };
        match d["kind"].as_str() {
            Some("turn") => return,
            Some("select") => answer(g, 2001, json!({"type": "select", "cardIds": [d["cardIds"][0]]})),
            Some("choice") => {
                let pass = d["choices"].as_array().unwrap().iter().find(|c| c["id"] == json!(99)).unwrap_or(&d["choices"][0])["id"].clone();
                answer(g, 2001, json!({"type": "choice", "id": pass}));
            }
            _ => panic!("unexpected decision {}", d),
        }
    }
}

#[test]
fn angry_crowd_creates_a_manaseed_only_for_a_cheap_target() {
    for class in ["YZ_Rare_AngryCrowd", "OD_Rare_AngryCrowd"] {
        let Some((g0, targets)) = play_then_targets(class, 3) else { return };
        for t in targets {
            let mut g = g0.clone();
            let cost = base_cost(&mut g, t);
            let before = count(&g, 2001, "landmark", Some("Manaseed"));
            answer(&mut g, 2001, json!({"type": "select", "cardIds": [t]}));
            finish(&mut g);
            let location = g.cards.get(&t).map(|c| c.location.clone());
            assert!(!matches!(location.as_deref(), Some("stormLeft" | "stormRight" | "landmark")), "{}: target {} discarded ({:?})", class, t, location);
            let created = count(&g, 2001, "landmark", Some("Manaseed")) - before;
            assert_eq!(created, usize::from(cost <= 2), "{}: target {} of Base Cost {}", class, t, cost);
        }
    }
}

#[test]
fn partial_referee_draws_only_for_an_expensive_target() {
    for class in ["YZ_Common_PartialReferee", "YZ_Rare_PartialReferee", "MU_Rare_PartialReferee"] {
        let Some((g0, targets)) = play_then_targets(class, 3) else { return };
        for t in targets {
            let mut g = g0.clone();
            let cost = base_cost(&mut g, t);
            let owner = g.cards[&t].pid;
            let before = count(&g, owner, "hand", None);
            answer(&mut g, 2001, json!({"type": "select", "cardIds": [t]}));
            finish(&mut g);
            let drawn = count(&g, owner, "hand", None) - before;
            assert_eq!(drawn, usize::from(cost >= 3), "{}: target {} of Base Cost {} (controller {})", class, t, cost, owner);
        }
    }
}
