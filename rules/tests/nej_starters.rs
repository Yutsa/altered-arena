//! Smoke: NEJ hero decks are loadable as starters and create a playable game.
use altered_rules::arena::{self, NewPlayer};
use altered_rules::managers::cards;

#[test]
fn nej_starter_decks_are_registered() {
    for id in ["ax_130_wayfinder_sierra", "br_130_squire_kojo", "ly_130_zoe", "mu_130_osric", "or_130_eyota", "yz_130_gray"] {
        assert!(cards::STARTER_DECKS.iter().any(|(d, _)| *d == id), "missing STARTER_DECKS entry {id}");
        let contents = cards::starter_contents(id);
        assert!(!contents.is_empty(), "empty starter contents for {id}");
        assert!(
            contents.iter().any(|(c, _)| c.contains("WayfinderSierra")
                || c.contains("SquireKojo")
                || c.contains("Zoe")
                || c.contains("Osric")
                || c.contains("Eyota")
                || c.contains("Gray")),
            "no NEJ hero in {id}"
        );
    }
}

#[test]
fn create_game_with_wayfinder_sierra() {
    let players = [
        NewPlayer { name: "Alice".into(), bot: false, deck: Some("ax_130_wayfinder_sierra".into()), ..Default::default() },
        NewPlayer { name: "Bob".into(), bot: false, deck: Some("br_130_squire_kojo".into()), ..Default::default() },
    ];
    let g = arena::create_v1_game(42, &players, &[]).expect("create NEJ starter game");
    assert!(!g.is_over());
    let hero_names: Vec<_> = g
        .cards
        .values()
        .filter_map(|c| c.props.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .filter(|n| n == "Wayfinder Sierra" || n == "Squire Kojo")
        .collect();
    assert!(hero_names.contains(&"Wayfinder Sierra".into()), "missing Wayfinder Sierra: {hero_names:?}");
    assert!(hero_names.contains(&"Squire Kojo".into()), "missing Squire Kojo: {hero_names:?}");
}
