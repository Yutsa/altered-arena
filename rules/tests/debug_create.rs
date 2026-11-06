//! Debug spawn: a card of a class appears in the asked zone.

use altered_rules::arena::{self, NewPlayer};
use altered_rules::managers::cards;
use altered_rules::models::card;

fn game() -> altered_rules::Game {
    arena::create_v1_game(
        42,
        &[
            NewPlayer { name: "Alice".into(), bot: false, deck: Some("ax_130_wayfinder_sierra".into()), ..Default::default() },
            NewPlayer { name: "Bob".into(), bot: false, deck: Some("br_130_squire_kojo".into()), ..Default::default() },
        ],
        &[],
    )
    .expect("create")
}

#[test]
fn create_from_class_puts_card_in_hand_and_expedition() {
    let mut g = game();
    let hand = cards::create_from_class(&mut g, "LY_Common_DoubleCross", "hand", 2001).unwrap();
    assert_eq!(g.card(hand).unwrap().location, "hand");
    assert_eq!(g.card(hand).unwrap().pid, 2001);
    assert_eq!(card::get(&mut g, hand, "name").unwrap(), serde_json::json!("Double-Cross"));

    let left = cards::create_from_class(&mut g, "AX_Common_Oddball", "stormLeft", 2001).unwrap();
    assert_eq!(g.card(left).unwrap().location, "stormLeft");
    assert!(cards::get_played_cards(&mut g, &serde_json::json!(2001), &serde_json::Value::Null).unwrap().contains(&left));
}
