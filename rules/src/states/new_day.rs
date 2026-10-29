//! States/NewDayTrait.php: first day (draw, mana selection), morning, new day discard, noon

use crate::core::engine;
use crate::core::globals;
use crate::framework::{self, ST_BEFORE_ASSIGNMENT};
use crate::game::{Game, PId};
use crate::game_methods;
use crate::helpers::ft;
use crate::managers::{cards, meeples, players};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `stFirstDay()`
// PHP: States/NewDayTrait.php:19
pub fn st_first_day(g: &mut Game) -> R<()> {
    globals::inc(g, "day", 1)?;
    let n = if globals::get(g, "testingOption")? != json!(false) && php::truthy(&globals::get(g, "testingOption")?) { 15 } else { 6 };
    let markers = meeples::create_hero_markers(g)?;
    if !markers.is_empty() {
        g.notify("addTerrainMarkers", json!({"ids": markers}));
    }
    let mut pids = vec![];
    for pid in players::get_all(g) {
        player::draw(g, pid, n, None, None, None, false, None)?;
        pids.push(pid);
    }
    globals::set(g, "firstDayManaSelection", json!([]))?;
    framework::set_players_multiactive(g, &pids, "", true)?;
    framework::next_state(g, "")
}

/// `argsFirstDayManaSelection()`
// PHP: States/NewDayTrait.php:49
pub fn args_first_day_mana_selection(g: &mut Game) -> R<Value> {
    let selection = globals::get(g, "firstDayManaSelection")?;
    let n = if php::truthy(&globals::get(g, "testingOption")?) { 10 } else { 3 };
    let mut args = json!({"canPass": false, "_private": [], "n": n});
    for pid in players::get_all(g) {
        let hand = player::get_hand(g, pid, &Value::Null)?;
        let sel = php::get(&selection, &pid.to_string()).clone();
        let private = php::get_mut(&mut args, "_private");
        php::set(private, &pid.to_string(), json!({"n": n, "cards": hand, "selection": sel}));
    }
    Ok(args)
}

/// `actFirstDayManaSelection($cardIds)`
// PHP: States/NewDayTrait.php:70
pub fn act_first_day_mana_selection(g: &mut Game, card_ids: &Value) -> R<()> {
    game_methods::local_check_action(g, "actFirstDayManaSelection", true)?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let all = args_first_day_mana_selection(g)?;
    let args = php::get(php::get(&all, "_private"), &p.to_string()).clone();
    if php::count_strict(card_ids)? as i64 != php::intval(php::get(&args, "n")) {
        return Err(PhpError::new("You must select the correct number of cards to put as mana"));
    }
    if !php::empty(&php::array_diff(card_ids, php::get(&args, "cards"))) {
        return Err(PhpError::new("Invalid card to select. Should not happen"));
    }
    let mut selection = globals::get(g, "firstDayManaSelection")?;
    php::set(&mut selection, &p.to_string(), card_ids.clone());
    globals::set(g, "firstDayManaSelection", selection)?;
    update_active_players_first_day_mana_selection(g)
}

/// `actCancelFirstDayManaSelection()`
pub fn act_cancel_first_day_mana_selection(g: &mut Game) -> R<()> {
    framework::check_possible_action(g, "actCancelFirstDayManaSelection")?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut selection = globals::get(g, "firstDayManaSelection")?;
    php::unset(&mut selection, &p.to_string());
    globals::set(g, "firstDayManaSelection", selection)?;
    update_active_players_first_day_mana_selection(g)
}

/// `updateActivePlayersFirstDayManaSelection()`
// PHP: States/NewDayTrait.php:104
pub fn update_active_players_first_day_mana_selection(g: &mut Game) -> R<()> {
    let selection = globals::get(g, "firstDayManaSelection")?;
    let all = players::get_all(g);
    let chosen: Vec<String> = php::keys_str(&selection);
    let ids: Vec<PId> = all.iter().copied().filter(|p| !chosen.contains(&p.to_string())).collect();
    if !ids.is_empty() {
        framework::set_players_multiactive(g, &ids, "done", true)?;
    } else {
        let selection = globals::get(g, "firstDayManaSelection")?;
        for pid in all {
            let ids = php::get(&selection, &pid.to_string()).clone();
            if php::empty(&ids) {
                continue;
            }
            cards::discard(g, &ids, "mana", None)?;
        }
        game_methods::check_card_listeners(g, "Noon", json!(ST_BEFORE_ASSIGNMENT), json!([]), None)?;
    }
    Ok(())
}

/// `stNewDay()`
// PHP: States/NewDayTrait.php:139
pub fn st_new_day(g: &mut Game) -> R<()> {
    let a = players::get_active_id(g);
    game_methods::init_custom_turn_order(g, "morning", Some(vec![a]), json!("stNewDayInit"), json!("stCheckMorning"), false, true, json!([]))
}

/// `stNewDayInit()`: victory check, new day resets, untap, first player, draws
// PHP: States/NewDayTrait.php:144
pub fn st_new_day_init(g: &mut Game) -> R<()> {
    if players::check_victory(g)? {
        return Ok(());
    }
    if globals::is(g, "enterTieBreakerMode")? {
        globals::set(g, "tieBreakerMode", json!(true))?;
        globals::set(g, "enterTieBreakerMode", json!(false))?;
    }
    globals::inc(g, "day", 1)?;
    globals::set(g, "skippedPlayers", json!([]))?;
    globals::set(g, "stormMoves", json!([]))?;
    globals::set(g, "expeditionMoves", json!([]))?;
    globals::set(g, "nextCharacterBoost", json!(0))?;
    globals::set(g, "nextCharacterBoostOccurence", json!(0))?;
    globals::set(g, "nextAnimalBoost", json!(0))?;
    globals::set(g, "nextAnimalBoostOccurence", json!(0))?;
    globals::set(g, "nextReserveCharacterBoost", json!(0))?;
    globals::set(g, "blockedExpeditions", json!([]))?;
    globals::set(g, "globalTough", json!([]))?;
    globals::set(g, "firstPass", json!(-1))?;
    globals::set(g, "turnCards", json!([]))?;
    globals::set(g, "nextCharacterInExpeditionBoost", json!([]))?;
    cards::untap_all(g)?;
    globals::set(g, "phase", json!(0))?;
    g.notify("newPhase", json!({"phase": "morning"}));

    let cur = globals::get_int(g, "firstPlayer")?;
    let new_first = players::get_next_id(g, cur)?;
    globals::set(g, "firstPlayer", json!(new_first))?;

    let mut nodes: Vec<Value> = vec![];
    // `$skipped[] = ...` on a variable that is never initialised: a new array each time
    let mut skipped: Option<Value> = None;
    for pid in players::get_all(g) {
        let mut node: Vec<Value> = vec![];
        let hero = player::get_hero(g, pid)?.ok_or_else(|| PhpError::new("Error: Call to a member function getUid() on null"))?;
        let uid = card::get(g, hero, "uid")?;
        let deck_count = player::get_deck_count(g, pid)?;
        let src = json!({"pId": pid, "sourceId": hero});
        let draw = |n: i64| ft::action("Draw", json!({"n": n, "players": "me"}), src.clone());
        let draw_mana = || ft::action("DrawMana", json!({"n": 1, "tapped": false}), src.clone());
        let shuffle = || ft::action("Shuffle", json!([]), json!({"pId": pid, "optional": true}));
        if uid == json!("ALT_CORE_B_LY_03_C") {
            if deck_count == 0 {
                node.push(ft::seq_optional(vec![shuffle(), draw(1), draw_mana()]));
            } else if deck_count == 1 {
                node.push(ft::seq(vec![draw(1), ft::seq_optional(vec![shuffle(), draw_mana()])]));
            } else if deck_count == 2 {
                node.push(ft::seq(vec![draw(1), draw_mana(), ft::seq_optional(vec![shuffle()])]));
            } else {
                player::draw(g, pid, 1, None, None, Some(hero), false, None)?;
                player::draw(g, pid, 1, None, Some("mana"), Some(hero), false, None)?;
            }
            let s = skipped.get_or_insert_with(php::arr);
            php::push(s, json!(pid));
            let s = s.clone();
            globals::set(g, "skippedPlayers", s)?;
        } else if deck_count == 0 {
            node.push(ft::seq_optional(vec![shuffle(), draw(2)]));
        } else if deck_count == 1 {
            node.push(ft::seq(vec![draw(1), ft::seq_optional(vec![shuffle(), draw(1)])]));
        } else if deck_count == 2 {
            node.push(ft::seq(vec![draw(2), ft::seq_optional(vec![shuffle()])]));
        } else {
            player::draw(g, pid, 2, None, None, Some(hero), false, None)?;
        }
        if !node.is_empty() {
            for no in node.iter_mut() {
                php::set(no, "pId", json!(pid));
            }
            nodes.extend(node);
        }
    }
    if !nodes.is_empty() {
        engine::setup(g, &ft::seq(nodes), json!({"order": "morning"}))?;
        engine::proceed(g, false, false)
    } else {
        game_methods::end_custom_order(g, "morning")
    }
}

/// `stCheckMorning()`
pub fn st_check_morning(g: &mut Game) -> R<()> {
    game_methods::check_card_listeners(g, "Morning", json!("initNewDay"), json!([]), None)
}

/// `initNewDay()`
pub fn init_new_day(g: &mut Game) -> R<()> {
    // 'stAfterNewday' calls stAfterNewDay (method names are case-insensitive)
    game_methods::init_custom_default_turn_order(g, "newDay", json!("stNewDayDraw"), json!("stAfterNewday"), true, true)
}

/// `stNewDayDraw()`: each player may put a card of their hand in mana
// PHP: States/NewDayTrait.php:275
pub fn st_new_day_draw(g: &mut Game) -> R<()> {
    let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut skipped = globals::get(g, "skippedPlayers")?;
    if php::in_array(&json!(p), &skipped) {
        let all = json!(players::get_all(g));
        let remaining = php::array_diff(&all, &skipped);
        if php::empty(&remaining) {
            game_methods::end_custom_order(g, "newDay")?;
        } else {
            game_methods::next_player_custom_order(g, "newDay")?;
        }
        return Ok(());
    }
    php::push(&mut skipped, json!(p));
    globals::set(g, "skippedPlayers", skipped)?;
    let node = json!({"pId": p, "action": "Discard", "args": {"canPass": true, "n": 1, "source": "hand", "destination": "mana"}});
    engine::setup(g, &node, json!({"order": "newDay"}))?;
    engine::proceed(g, false, false)
}

/// `stAfterNewDay()`: Noon
// PHP: States/NewDayTrait.php:309
pub fn st_after_new_day(g: &mut Game) -> R<()> {
    globals::set(g, "phase", json!(1))?;
    g.notify("newPhase", json!({"phase": "noon"}));
    game_methods::check_card_listeners(g, "Noon", json!(ST_BEFORE_ASSIGNMENT), json!([]), None)
}
