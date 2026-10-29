//! Helpers/Conditions.php predicates with PHP quirks worth pinning down

use altered_rules::core::globals;
use altered_rules::helpers::conditions;
use altered_rules::models::card;
use altered_rules::Game;
use serde_json::{json, Value};

fn tiktok() -> Option<Game> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens/rich/AX_Common_TikTok.json");
    let data: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    Some(Game::from_bga_tables(&data["db"], &[3, 1, 6, 4, 2, 5]).expect("load"))
}

fn check(g: &mut Game, cond: &str, card: Option<i64>, event: Value) -> bool {
    conditions::check(g, &json!({ "condition": cond }), card, &event).unwrap()
}

#[test]
fn event_predicates_use_loose_comparisons() {
    let mut g = Game::empty();
    assert!(check(&mut g, "isFromReserve", None, json!({"from": "reserve"})));
    assert!(!check(&mut g, "isFromReserve", None, json!({})));
    // the dispatcher ignores the case of the name, as PHP method names
    assert!(check(&mut g, "ISTORESERVE", None, json!({"to": "reserve"})));
    assert!(check(&mut g, "isNotToken", None, json!({"token": "0"})));
    assert!(check(&mut g, "realResupply", None, json!({"notResupply": 0})));
    // a raw non-false value passes the `=== false` check
    assert!(check(&mut g, "leaveAndFleeting", None, json!({"fleeting": 0})));
    assert!(!check(&mut g, "leaveAndFleeting", None, json!({})));
    assert!(check(&mut g, "hasRolled:4", None, json!({"rolls": [1, "5"]})));
    assert!(!check(&mut g, "hasRolled:4:LTE", None, json!({"rolls": [5, 6]})));
    assert!(check(&mut g, "selectedRoll:3:EQ", None, json!({"selectedRoll": "3"})));
    assert!(check(&mut g, "specialEffect:foo", None, json!({"specialEffect": "foo"})));
    assert!(check(&mut g, "isGain:boost", None, json!({"action": "Gain", "gain": {"type": "boost"}})));
    assert!(!check(&mut g, "isGain:boost", None, json!({"action": "Gain", "gain": {"type": "anchored"}})));
}

#[test]
fn missing_arguments_and_unknown_ops_are_errors() {
    let mut g = Game::empty();
    let e = conditions::check(&mut g, &json!({"condition": "specialEffect"}), None, &json!({}));
    assert!(e.unwrap_err().to_string().contains("ArgumentCountError"));
    let e = conditions::check(&mut g, &json!({"condition": "nope"}), None, &json!({}));
    assert!(e.is_err());
}

#[test]
fn type_check() {
    let t = |ty: Value, ct: &str, tok: bool, at: Value| conditions::type_check_v(&ty, &json!(ct), &json!(tok), &at).unwrap();
    assert!(t(Value::Null, "spell", false, json!([])));
    assert!(t(json!("character"), "token", true, json!([])));
    assert!(!t(json!("characterOnly"), "character", true, json!([])));
    assert!(!t(json!("token"), "character", false, json!([])));
    assert!(t(json!("permanent"), "character", false, json!(["permanent"])));
    assert!(!t(json!("spell"), "character", false, json!(["permanent"])));
    // in_array on a non array additionalType throws, as PHP 8
    assert!(conditions::type_check_v(&json!("spell"), &json!("character"), &json!(false), &Value::Null).is_err());
    // subTypeCheck: any truthy $exclude (even the string "false") excludes
    assert!(conditions::sub_type_check_v(&json!("robot"), &json!(["robot"]), &json!(false)).unwrap());
    assert!(!conditions::sub_type_check_v(&json!("robot"), &json!(["robot"]), &json!("false")).unwrap());
}

#[test]
fn smoke_them_out_charges_are_per_copy() {
    let mut g = Game::empty();
    let p = 2001;
    conditions::arm_smoke_them_out_pid(&mut g, p, 5).unwrap();
    conditions::arm_smoke_them_out_pid(&mut g, p, 5).unwrap();
    conditions::arm_smoke_them_out_pid(&mut g, p, 6).unwrap();
    assert_eq!(conditions::smoke_them_out_charges(&mut g, &json!(p), &json!(5)).unwrap(), 2);
    assert_eq!(conditions::smoke_them_out_charges(&mut g, &json!(p), &Value::Null).unwrap(), 3);
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, Some(6)).unwrap();
    assert_eq!(globals::get(&mut g, "smokeThemOutArmed").unwrap(), json!({"2001": {"5": 2}}));
    // without a card id nothing is consumed from per-copy charges
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, None).unwrap();
    assert_eq!(conditions::smoke_them_out_charges(&mut g, &json!(p), &json!(5)).unwrap(), 2);
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, Some(5)).unwrap();
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, Some(5)).unwrap();
    assert_eq!(globals::get(&mut g, "smokeThemOutArmed").unwrap(), json!({}));

    // legacy saves: one player-wide count
    globals::set(&mut g, "smokeThemOutArmed", json!({"2001": 2})).unwrap();
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, Some(5)).unwrap();
    assert_eq!(globals::get(&mut g, "smokeThemOutArmed").unwrap(), json!({"2001": 1}));
    conditions::consume_smoke_them_out_charge_pid(&mut g, p, Some(5)).unwrap();
    assert_eq!(globals::get(&mut g, "smokeThemOutArmed").unwrap(), json!({}));
    conditions::arm_smoke_them_out_pid(&mut g, p, 7).unwrap();
    conditions::disarm_smoke_them_out_pid(&mut g, p).unwrap();
    assert_eq!(conditions::smoke_them_out_charges(&mut g, &json!(p), &json!(7)).unwrap(), 0);
}

#[test]
fn card_search_names() {
    let Some(mut g) = tiktok() else { return };
    let c = *g.cards.keys().next().unwrap();
    let set_uid = |g: &mut Game, uid: &str| card::set_property(g, c, "uid", json!(uid), false).unwrap();
    set_uid(&mut g, "ALT_EOLE_B_YZ_115_C");
    assert_eq!(conditions::card_search_name(&mut g, c).unwrap(), "YZ_115");
    assert!(conditions::card_matching_search(&mut g, c, &json!("YZ_115")).unwrap());
    assert!(!conditions::card_matching_search(&mut g, c, &json!("YZ_11")).unwrap());
    assert!(!conditions::card_matching_search(&mut g, c, &json!("")).unwrap());
    set_uid(&mut g, "ALT_EOLE_B_YZ_1150_C");
    assert!(!conditions::card_matching_search(&mut g, c, &json!("YZ_115")).unwrap());
    set_uid(&mut g, "ALT_CORE_B_YZ_115");
    assert!(conditions::card_matching_search(&mut g, c, &json!("YZ_115")).unwrap());
    set_uid(&mut g, "ALT_CORE_YZ_115_C");
    assert_eq!(conditions::card_search_name(&mut g, c).unwrap(), "");
    set_uid(&mut g, "ALT_CORE_B_yz_115_C");
    assert_eq!(conditions::card_search_name(&mut g, c).unwrap(), "");
}

#[test]
fn card_predicates_on_a_recorded_position() {
    let Some(mut g) = tiktok() else { return };
    let hero = *g.cards.iter().find(|(_, c)| c.location.starts_with("board-hero-")).unwrap().0;
    let pid = g.card(hero).unwrap().pid;
    // hasControl returns null for an unknown op: the check passes
    assert!(check(&mut g, "hasControl:character:99:false:all:XX", Some(hero), json!({})));
    assert!(check(&mut g, "hasControl:character:0", Some(hero), json!({})));
    assert!(!check(&mut g, "hasControl:character:99", Some(hero), json!({})));
    assert!(check(&mut g, "isMe", Some(hero), json!({"pId": pid.to_string()})));
    assert!(check(&mut g, "isNotMe", Some(hero), json!({})));
    assert!(check(&mut g, "excludeSelf", Some(hero), json!({"sourceId": hero + 1})));
    assert!(!check(&mut g, "excludeSelf", Some(hero), json!({"cardId": hero.to_string()})));
    // the card is required when the predicate calls a method on it
    assert!(conditions::check(&mut g, &json!({"condition": "isMe"}), None, &json!({})).is_err());
}
