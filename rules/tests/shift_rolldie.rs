//! Shift ({R} Roll a die. On a 4+, I gain 1 boost) and Zoe's 6+ reaction (Exhaust + create Shift).
use altered_rules::arena::{self, NewPlayer};
use altered_rules::core::engine;
use altered_rules::helpers::utils;
use altered_rules::managers::{cards, meeples};
use altered_rules::models::card;
use serde_json::json;

fn zoe_game(dice: &[i64]) -> altered_rules::Game {
    let players = [
        NewPlayer { name: "Alice".into(), bot: false, deck: Some("ly_130_zoe".into()), ..Default::default() },
        NewPlayer { name: "Bob".into(), bot: false, deck: Some("br_130_squire_kojo".into()), ..Default::default() },
    ];
    arena::create_v1_game(42, &players, dice).expect("create")
}

fn create_shift(g: &mut altered_rules::Game, pid: i64, location: &str) -> i64 {
    let props = cards::classes().get("LY_Common_Shift").expect("LY_Common_Shift class").clone();
    let ids = cards::create(
        g,
        &json!([{
            "location": location,
            "player_id": pid,
            "nbr": 1,
            "properties": props,
        }]),
        None,
    )
    .expect("create Shift");
    *ids.first().unwrap()
}

fn zoe_id(g: &altered_rules::Game) -> i64 {
    *g.cards.iter().find(|(_, c)| c.props.get("name").and_then(|n| n.as_str()) == Some("Zoe")).map(|(id, _)| id).expect("Zoe hero")
}

/// Play-from-reserve path: tag effectReserve with sourceId and run it through the engine.
fn run_effect_reserve(g: &mut altered_rules::Game, sid: i64, pid: i64) -> Result<(), altered_rules::PhpError> {
    card::set_location(g, sid, "stormLeft")?;
    let effect = card::get(g, sid, "effectReserve")?;
    let tagged = utils::tag_tree(&effect, &json!({"sourceId": sid, "pId": pid}), false);
    g.begin_request(Some(pid))?;
    engine::setup(g, &json!({"childs": [tagged]}), json!({}))?;
    engine::proceed(g, false, false)
}

#[test]
fn effect_reserve_rolldie_gain_me_with_forced_5() {
    let mut g = zoe_game(&[5]);
    let sid = create_shift(&mut g, 2001, "reserve");
    run_effect_reserve(&mut g, sid, 2001).expect("RollDie→Gain(me)");
    assert_eq!(meeples::count_meeples(&g, &format!("card-{sid}"), &json!("boost")).unwrap(), 1);
    // 5 is not 6+: Zoe stays ready
    assert!(!card::is_tapped(&g, zoe_id(&g)).unwrap());
}

#[test]
fn effect_reserve_rolldie_low_roll_skips_gain() {
    let mut g = zoe_game(&[2]);
    let sid = create_shift(&mut g, 2001, "reserve");
    run_effect_reserve(&mut g, sid, 2001).expect("low roll");
    assert_eq!(meeples::count_meeples(&g, &format!("card-{sid}"), &json!("boost")).unwrap(), 0);
}

#[test]
fn gain_me_resolves_via_parent_source_id() {
    let mut g = zoe_game(&[]);
    let sid = create_shift(&mut g, 2001, "stormLeft");
    g.begin_request(Some(2001)).unwrap();
    let flow = json!({"childs": [{
        "type": "seq",
        "sourceId": sid,
        "pId": 2001,
        "childs": [{"action": "Gain", "pId": 2001, "args": {"cardId": "me", "type": "boost", "n": 1, "upTo": 99}}]
    }]});
    engine::setup(&mut g, &flow, json!({})).unwrap();
    engine::proceed(&mut g, false, false).expect("Gain(me) via parent sourceId");
    assert_eq!(meeples::count_meeples(&g, &format!("card-{sid}"), &json!("boost")).unwrap(), 1);
}

/// Zoe's RollDie reaction is `Exhaust([])` (no cardId): PHP uses resolveSourceId; before the fix
/// Rust threw the same banner as the screenshot ("no card in args (Gain). Should not happen").
#[test]
fn zoe_style_exhaust_empty_args_uses_source_id() {
    let mut g = zoe_game(&[]);
    let zoe = zoe_id(&g);
    assert!(!card::is_tapped(&g, zoe).unwrap());
    g.begin_request(Some(2001)).unwrap();
    // Same shape as Zoe's passive output after ActivateCard tags sourceId
    let flow = json!({"childs": [{
        "action": "Exhaust",
        "pId": 2001,
        "sourceId": zoe,
        "args": []
    }]});
    engine::setup(&mut g, &flow, json!({})).unwrap();
    engine::proceed(&mut g, false, false).expect("Exhaust([]) must resolve via sourceId");
    assert!(card::is_tapped(&g, zoe).unwrap());
}

#[test]
fn shift_roll_6_applies_boost_without_system_error() {
    // Full Shift path on a 6: Gain(me) must succeed. Zoe's optional reaction is a separate
    // ActivateCard choice in a live turn; Exhaust([]) itself is covered above.
    let mut g = zoe_game(&[6]);
    let sid = create_shift(&mut g, 2001, "reserve");
    run_effect_reserve(&mut g, sid, 2001).expect("RollDie 6 → Gain(me) must not error");
    assert_eq!(meeples::count_meeples(&g, &format!("card-{sid}"), &json!("boost")).unwrap(), 1);
}
