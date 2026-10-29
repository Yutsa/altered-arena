//! Managers/Players.php: player lookup and order, opponent powers, storm and victory

use crate::core::engine;
use crate::core::globals;
use crate::framework;
use crate::game::{CardId, Game, PId};
use crate::helpers::{collection, ft};
use crate::managers::{cards, meeples};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `Players::getAll()` (table order)
pub fn get_all(g: &Game) -> Vec<PId> {
    g.players.keys().copied().collect()
}

/// `Players::getActiveId()`
pub fn get_active_id(g: &Game) -> PId {
    g.active_player_id()
}

/// `Players::getCurrentId()`
pub fn get_current_id(g: &Game) -> PId {
    g.current_player_id()
}

/// `Players::get($id)`: the player whose id loosely equals `$id` (null when none); null id = the
/// active player
// PHP: Managers/Players.php:75, Helpers/CachedDB_Manager.php
pub fn get_by_value(g: &Game, id: &Value) -> Option<PId> {
    let id = if id.is_null() { json!(get_active_id(g)) } else { id.clone() };
    g.players.keys().copied().find(|p| php::loose_eq(&json!(p), &id))
}

/// `Players::get($id)` with an int id
pub fn get(g: &Game, id: PId) -> Option<PId> {
    get_by_value(g, &json!(id))
}

/// `Players::getActive()`
pub fn get_active(g: &Game) -> Option<PId> {
    get(g, get_active_id(g))
}

/// `Players::getCurrent()`
pub fn get_current(g: &Game) -> Option<PId> {
    get(g, get_current_id(g))
}

/// `Players::getNextId($player)`
// PHP: Managers/Players.php:90
pub fn get_next_id(g: &Game, pid: PId) -> R<PId> {
    let t = framework::get_next_player_table(g);
    let n = php::get(&t, &pid.to_string());
    if n.is_null() {
        return Err(PhpError::new(format!("Undefined array key {} (getNextPlayerTable)", pid)));
    }
    Ok(php::intval(n))
}

/// `Players::getNext($player)`
pub fn get_next(g: &Game, pid: PId) -> R<Option<PId>> {
    let n = get_next_id(g, pid)?;
    Ok(get(g, n))
}

/// `Players::getPrevious($player)`
pub fn get_previous(g: &Game, pid: PId) -> R<Option<PId>> {
    let t = framework::get_prev_player_table(g);
    Ok(get(g, php::intval(php::get(&t, &pid.to_string()))))
}

/// `Players::hasOpponentBlockingPower($player, $expedition, $isGigantic)`
// PHP: Managers/Players.php:156
pub fn has_opponent_blocking_power(g: &mut Game, pid: PId, expedition: &str, is_gigantic: bool) -> R<bool> {
    for p2 in get_all(g) {
        if p2 == pid {
            continue;
        }
        if crate::models::player::has_blocking_power(g, p2, expedition, is_gigantic)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::count()`
pub fn count(g: &Game) -> usize {
    g.players.len()
}

/// `Players::getTurnOrder($firstPlayer)`: the first player, then the next ones
// PHP: Managers/Players.php:132
pub fn get_turn_order(g: &mut Game, first: Option<PId>) -> R<Vec<PId>> {
    let first = match first {
        Some(f) => f,
        None => globals::get_int(g, "firstPlayer")?,
    };
    let mut order = vec![];
    let mut p = first;
    loop {
        order.push(p);
        p = get_next_id(g, p)?;
        if p == first {
            break;
        }
        if order.len() > 64 {
            return Err(PhpError::new("getTurnOrder: first player not found in the player table"));
        }
    }
    Ok(order)
}

/// `Players::getOpponentAdditionalCost($player, $type)`
pub fn get_opponent_additional_cost(g: &mut Game, pid: PId, ty: &Value) -> R<i64> {
    let mut cost = 0;
    for p2 in get_all(g) {
        if p2 == pid {
            continue;
        }
        cost += player::get_opponent_additional_cost(g, p2, ty)?;
    }
    Ok(cost)
}

/// `Players::getOpponentMinimumCost($player, $type)`
// PHP: Managers/Players.php:207
pub fn get_opponent_minimum_cost(g: &mut Game, pid: Option<PId>, ty: &Value) -> R<i64> {
    let mut cost = 0;
    let me = pid.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    for p2 in get_all(g) {
        if p2 == me {
            continue;
        }
        cost = cost.max(player::get_opponent_minimum_cost(g, p2, ty)?);
    }
    Ok(cost)
}

/// `Players::getMinimumReserveCost()`
pub fn get_minimum_reserve_cost(g: &mut Game) -> R<i64> {
    let mut cost = 0;
    for p in get_all(g) {
        cost = cost.max(player::get_minimum_reserve_cost(g, p)?);
    }
    Ok(cost)
}

/// `Players::hasBlockOpponentReserveGain($player)` (returns at the first opponent)
pub fn has_block_opponent_reserve_gain(g: &mut Game, pid: PId) -> R<bool> {
    for p2 in get_all(g) {
        if p2 == pid {
            continue;
        }
        return player::has_block_opponent_reserve_gain(g, p2);
    }
    Ok(false)
}

/// `Players::hasBlockGainNewCounters()`
pub fn has_block_gain_new_counters(g: &mut Game) -> R<bool> {
    for p in get_all(g) {
        if player::has_block_gain_new_counters(g, p)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::getIncreaseReserveCost($type)`
pub fn get_increase_reserve_cost(g: &mut Game, ty: &Value) -> R<i64> {
    let mut cost = 0;
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        cost += php::intval(&card::get_increase_reserve_cost(g, c, ty)?);
    }
    Ok(cost)
}

/// `Players::getReduceReserveCost($type, $subtypes, $ownerId, $cardId)`
pub fn get_reduce_reserve_cost(g: &mut Game, ty: &Value, subtypes: &Value, owner: PId, card_id: CardId) -> R<i64> {
    let mut cost = 0;
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        cost += php::intval(&card::get_reduce_reserve_cost(g, c, ty, subtypes, owner, card_id)?);
    }
    Ok(cost)
}

/// `Players::isExhaustedCharactersMorning()`
pub fn is_exhausted_characters_morning(g: &mut Game) -> R<bool> {
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        if card::get(g, c, "exhaustCharactersMorning")? == json!(true) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::getReserveSlots()`
pub fn get_reserve_slots(g: &mut Game) -> R<Value> {
    let mut out = php::arr();
    for p in get_all(g) {
        let n = player::get_reserve_slots(g, p)?;
        php::set(&mut out, &p.to_string(), json!(n));
    }
    Ok(out)
}

/// `Players::getLandmarkSlots()`
pub fn get_landmark_slots(g: &mut Game) -> R<Value> {
    let mut out = php::arr();
    for p in get_all(g) {
        let n = player::get_landmark_slots(g, p)?;
        php::set(&mut out, &p.to_string(), json!(n));
    }
    Ok(out)
}

/// `Players::getAllReserveSlots()`
pub fn get_all_reserve_slots(g: &mut Game) -> R<i64> {
    let mut n = 0;
    for p in get_all(g) {
        n += player::get_all_reserve_slots(g, p)?;
    }
    Ok(n)
}

/// `Players::getBiomesInStorm($excludeMoveInfo)`: pId → expedition → biome => biome, with the
/// modifiers of the cards in play
// PHP: Managers/Players.php:513
pub fn get_biomes_in_storm(g: &mut Game, exclude_move: bool) -> R<Value> {
    let mut all = php::arr();
    for p in get_all(g) {
        let player_storm = player::get_biome_in_storms(g, p)?;
        for (side, biomes) in php::entries(&player_storm) {
            let expedition = if side == "hero" { "stormLeft" } else { "stormRight" };
            let mut new_biomes = php::arr();
            for b in php::vals(biomes) {
                php::set(&mut new_biomes, &php::key_of(b), b.clone());
            }
            biomes_modifier(g, &mut new_biomes, p, expedition, false, exclude_move)?;
            php::set(php::get_mut(&mut all, &p.to_string()), expedition, new_biomes);
        }
    }
    Ok(all)
}

/// `Players::filterBiomes($expeditionAttributes)`: pId → the expeditions having one of the biomes
/// (all of them when null)
// PHP: Managers/Players.php:531
pub fn filter_biomes(g: &mut Game, attrs: &Value) -> R<Value> {
    let all = get_biomes_in_storm(g, true)?;
    let mut authorized = php::arr();
    for (pid, player_storm) in php::entries(&all) {
        php::set(&mut authorized, &pid, php::arr());
        if attrs.is_null() {
            php::set(&mut authorized, &pid, Value::Array(php::keys(player_storm)));
            continue;
        }
        for a in foreach_vals(attrs)? {
            let k = php::key_of(&a);
            if php::isset(php::get(player_storm, "stormLeft"), &k) {
                php::push(php::get_mut(&mut authorized, &pid), json!("stormLeft"));
            }
            if php::isset(php::get(player_storm, "stormRight"), &k) {
                php::push(php::get_mut(&mut authorized, &pid), json!("stormRight"));
            }
        }
    }
    Ok(authorized)
}

/// `Players::excludeBiomes($expeditionAttributes)`: pId → the expeditions missing one of the
/// biomes
// PHP: Managers/Players.php:554
pub fn exclude_biomes(g: &mut Game, attrs: &Value) -> R<Value> {
    let all = get_biomes_in_storm(g, true)?;
    let mut excluded = php::arr();
    for (pid, player_storm) in php::entries(&all) {
        for a in foreach_vals(attrs)? {
            let k = php::key_of(&a);
            if !php::isset(php::get(player_storm, "stormLeft"), &k) {
                php::push(php::get_mut(&mut excluded, &pid), json!("stormLeft"));
            }
            if !php::isset(php::get(player_storm, "stormRight"), &k) {
                php::push(php::get_mut(&mut excluded, &pid), json!("stormRight"));
            }
        }
    }
    Ok(excluded)
}

/// `foreach ($x as ...)` on a non-array: a warning and no iteration
fn foreach_vals(v: &Value) -> R<Vec<Value>> {
    Ok(php::vals(v).into_iter().cloned().collect())
}

/// `Players::checkVictory()`
// PHP: Managers/Players.php:249
pub fn check_victory(g: &mut Game) -> R<bool> {
    let mut max_moves = 0i64;
    let mut tiebreaker = false;
    let mut victor: PId = -1;
    if globals::get(g, "tieBreakerMode")? == json!(true) {
        return Ok(false);
    }
    for p in get_all(g) {
        let r = player::check_victory(g, p)?;
        let (vic, moves) = match &r {
            Value::Array(a) => (a.first().cloned().unwrap_or(Value::Null), php::intval(a.get(1).unwrap_or(&Value::Null))),
            _ => (Value::Null, 0),
        };
        if vic == json!(true) {
            if moves > max_moves {
                victor = p;
                max_moves = moves;
                tiebreaker = false;
            } else if moves == max_moves {
                tiebreaker = true;
            }
        }
    }
    if tiebreaker {
        globals::set(g, "enterTieBreakerMode", json!(true))?;
        for p in get_all(g) {
            let c = player::get_companion_token(g, p)?;
            g.meeple_mut(c)?.location = "storm-4".into();
            let h = player::get_hero_token(g, p)?;
            g.meeple_mut(h)?.location = "storm-3".into();
        }
        let markers = crate::managers::meeples::get_of_type(g, "storm-%", &json!(["ocean", "forest", "mountain"]))?;
        for m in &markers {
            crate::managers::meeples::delete(g, &json!(m))?;
        }
        g.notify("startTiebreak", json!({}));
    } else if victor != -1 {
        if let Some(p) = g.players.get_mut(&victor) {
            p.score = 1;
        }
        crate::game_methods::jump_to_or_call(g, &json!(framework::ST_PRE_END_OF_GAME), &json!([]))?;
        return Ok(true);
    }
    Ok(false)
}

/// `Players::initializeDecks()` (setup)
pub fn initialize_decks(_g: &mut Game) -> R<()> {
    Err(PhpError::nt("Managers/Players.php:309 initializeDecks"))
}

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

fn other_storm(expedition: &str) -> &'static str {
    if expedition == "stormLeft" {
        "stormRight"
    } else {
        "stormLeft"
    }
}

/// `Players::getPowersBlockedExpeditions()`
// PHP: Managers/Players.php:144
pub fn get_powers_blocked_expeditions(g: &mut Game) -> R<Value> {
    let mut statuses = php::arr();
    for p in get_all(g) {
        for exp in STORMS {
            let b = has_opponent_blocking_power(g, p, exp, false)?;
            php::set(php::get_mut(&mut statuses, &p.to_string()), exp, json!(b));
        }
    }
    Ok(statuses)
}

/// `Players::hasOpponentBlockMoveExpedition($player, $expedition)`
// PHP: Managers/Players.php:170
pub fn has_opponent_block_move_expedition(g: &mut Game, pid: PId, expedition: &str) -> R<bool> {
    for p2 in get_all(g) {
        if p2 == pid {
            continue;
        }
        if player::has_block_move_expedition(g, p2, expedition)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::hasOppositeDefender($expedition)`: a player has an opposite defender facing it
// PHP: Managers/Players.php:184
pub fn has_opposite_defender(g: &mut Game, expedition: &str) -> R<bool> {
    let opposite = other_storm(expedition);
    for p in get_all(g) {
        if player::has_opposite_defender(g, p, opposite)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::getBiomeTotals()`
// PHP: Managers/Players.php:316
pub fn get_biome_totals(g: &mut Game) -> R<Value> {
    let mut biomes = php::arr();
    for p in get_all(g) {
        let s = player::get_biome_strength(g, p, &STORMS, true)?;
        php::set(&mut biomes, &p.to_string(), s);
    }
    Ok(biomes)
}

/// `Players::getDefenders($onlyDefenders)`: `[pId][expedition][] = cardId` (a Gigantic
/// defender in both expeditions), or `[$defenders, $ignoreDefenders]`
// PHP: Managers/Players.php:326
pub fn get_defenders(g: &mut Game, only: bool) -> R<Value> {
    let mut defenders = php::arr();
    let mut ignore_defenders = php::arr();
    for p in get_all(g) {
        let played = player::get_played_cards(g, p, &Value::Null)?;
        for c in cards::where_location(g, played, &json!(STORMS)) {
            let loc = card::location(g, c)?;
            let ps = p.to_string();
            if card::is_defender(g, c)? {
                php::push(php::get_mut(php::get_mut(&mut defenders, &ps), &loc), json!(c));
                if php::truthy(&card::is_gigantic(g, c, false)?) {
                    php::push(php::get_mut(php::get_mut(&mut defenders, &ps), other_storm(&loc)), json!(c));
                }
            }
            let power = if card::is(g, c, "defenderIgnoreBehind")? {
                Some("behind")
            } else if card::is(g, c, "defenderIgnoreContact")? {
                Some("contact")
            } else if card::is_ignore_defender(g, c)? {
                Some("all")
            } else {
                None
            };
            if let Some(power) = power {
                php::set(php::get_mut(php::get_mut(&mut ignore_defenders, &ps), &loc), &c.to_string(), json!(power));
            }
        }
    }
    if only {
        return Ok(defenders);
    }
    Ok(json!([defenders, ignore_defenders]))
}

/// `Players::getIgnoreDefenders(&$defenders, $ignoreDefenders)`: removes the defenders ignored by
/// the cards that ignore them
// PHP: Managers/Players.php:409
pub fn get_ignore_defenders(g: &mut Game, defenders: &mut Value, ignore_defenders: &Value) -> R<()> {
    if php::empty(ignore_defenders) {
        return Ok(());
    }
    let winners = get_winning_player_by_storms(g)?;
    for (pid_k, storms) in php::entries(ignore_defenders) {
        for (storm, cs) in php::entries(storms) {
            for (_cid, power) in php::entries(cs) {
                let has = |d: &Value| php::isset(php::get(d, &pid_k), &storm);
                if php::loose_eq(power, &json!("all")) {
                    if php::key_exists(defenders, &pid_k) {
                        php::unset(php::get_mut(defenders, &pid_k), &storm);
                    }
                } else if php::loose_eq(power, &json!("behind")) && {
                    let w = php::get(&winners, &storm);
                    !w.is_null() && php::loose_ne(w, &json!(-1)) && php::loose_ne(w, &php::key_value(&pid_k)) && has(defenders)
                } {
                    php::unset(php::get_mut(defenders, &pid_k), &storm);
                } else if php::loose_eq(power, &json!("contact")) && {
                    let p = get_by_value(g, &php::key_value(&pid_k)).ok_or_else(|| PhpError::new("Error: Call to a member function isInContact() on null"))?;
                    player::is_in_contact(g, p, Some(&storm))?
                } {
                    if php::key_exists(defenders, &pid_k) {
                        php::unset(php::get_mut(defenders, &pid_k), &storm);
                    }
                }
            }
        }
    }
    Ok(())
}

/// `Players::getBlockedExpeditions()`: `[pId][expedition] => bool` (global blocks, opposite
/// defenders, defenders)
// PHP: Managers/Players.php:459
pub fn get_blocked_expeditions(g: &mut Game) -> R<Value> {
    let blocked_expeditions = globals::get(g, "blockedExpeditions")?;
    let mut blocked_opposite_defenders = php::arr();
    for exp in STORMS {
        let b = has_opposite_defender(g, exp)?;
        php::set(&mut blocked_opposite_defenders, exp, json!(b));
    }
    let both = get_defenders(g, false)?;
    let mut defenders = php::geti(&both, 0).clone();
    let ignore_defenders = php::geti(&both, 1).clone();
    get_ignore_defenders(g, &mut defenders, &ignore_defenders)?;

    let mut statuses = php::arr();
    for p in get_all(g) {
        let ps = p.to_string();
        for exp in STORMS {
            let mut blocked = false;
            if php::isset(&blocked_expeditions, &ps) && php::in_array(&json!(exp), php::get(&blocked_expeditions, &ps)) {
                blocked = true;
            }
            if !blocked && php::truthy(php::get(&blocked_opposite_defenders, exp)) {
                blocked = true;
            }
            if !blocked && !php::empty(php::coalesce(php::get(php::get(&defenders, &ps), exp), &php::arr())) {
                blocked = true;
            }
            php::set(php::get_mut(&mut statuses, &ps), exp, json!(blocked));
        }
    }
    Ok(statuses)
}

/// `Players::getActionInsteadOfAdvance()`: `[pId][expedition][cardId] => action`
// PHP: Managers/Players.php:500
pub fn get_action_instead_of_advance(g: &mut Game) -> R<Value> {
    let mut actions = php::arr();
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        let a = card::get(g, c, "actionInsteadAdvance")?;
        if php::loose_ne(&a, &json!("")) {
            let ps = php::strval(&json!(card::pid(g, c)?));
            let loc = card::location(g, c)?;
            let cs = c.to_string();
            php::set(php::get_mut(php::get_mut(&mut actions, &ps), &loc), &cs, a.clone());
            let gig = php::truthy(&card::is_gigantic(g, c, false)?);
            if gig && php::in_list(&a, &["ErisRare", "ErisCommon"]) {
                php::set(php::get_mut(php::get_mut(&mut actions, &ps), other_storm(&loc)), &cs, json!("blockMove"));
            }
            if gig && php::loose_eq(&a, &json!("PegasusCommon")) {
                php::set(php::get_mut(php::get_mut(&mut actions, &ps), other_storm(&loc)), &cs, json!("PegasusCommon"));
            }
        }
    }
    Ok(actions)
}

/// `Players::computeStorm($advance)`: who wins each biome of each expedition; with `$advance`,
/// moves the winning expeditions (or queues the cards replacing the move); returns
/// `[pId][side][biome] => 0 / 1 / 2`
// PHP: Managers/Players.php:572
pub fn compute_storm(g: &mut Game, advance: bool) -> R<Value> {
    if globals::is(g, "tieBreakerMode")? {
        return Ok(php::arr());
    }

    let players = get_all(g);
    let wb = || json!({"forest": {"pId": null, "value": 0}, "mountain": {"pId": null, "value": 0}, "ocean": {"pId": null, "value": 0}});
    let mut winners = json!({"stormLeft": wb(), "stormRight": wb()});
    let eb = || json!({"forest": false, "mountain": false, "ocean": false});
    let mut equality = json!({"stormLeft": eb(), "stormRight": eb()});

    // Winner calculation
    for &p in &players {
        let expeditions = player::get_biome_strength(g, p, &STORMS, true)?;
        for (expedition, biomes) in php::entries(&expeditions) {
            for (biome, value) in php::entries(biomes) {
                let w = php::get_mut(php::get_mut(&mut winners, &expedition), &biome);
                let cur = php::get(w, "value").clone();
                if php::lt(&cur, value) {
                    php::set(w, "value", value.clone());
                    php::set(w, "pId", json!(p));
                } else if php::loose_eq(&cur, value) {
                    php::set(w, "pId", Value::Null);
                    if php::ge(value, &json!(1)) {
                        php::set(php::get_mut(&mut equality, &expedition), &biome, json!(true));
                    }
                }
            }
        }
    }

    let mut movements = php::arr();
    let blocked_expeditions = get_blocked_expeditions(g)?;
    let action_instead_advance = get_action_instead_of_advance(g)?;
    let mut to_move = php::arr();
    let mut expedition_ascension_moves = php::arr();
    // For each player, check whether hero and/or companion move forward
    for side in ["hero", "companion"] {
        for &p in &players {
            let ps = p.to_string();
            let biomes_by_storm = player::get_biome_in_storms(g, p)?;
            let biomes = php::get(&biomes_by_storm, side).clone();
            if biomes.is_null() {
                continue;
            }
            let mut mv = false;
            let mut winning_biomes: Vec<Value> = vec![];
            let expedition = if side == "hero" { "stormLeft" } else { "stormRight" };
            php::set(php::get_mut(&mut movements, &ps), side, json!({"ocean": 0, "forest": 0, "mountain": 0}));
            if php::truthy(php::get(php::get(&blocked_expeditions, &ps), expedition)) {
                continue;
            }
            let mut new_biomes = php::arr();
            for b in php::vals(&biomes) {
                php::set(&mut new_biomes, &php::key_of(b), b.clone());
            }
            biomes_modifier(g, &mut new_biomes, p, expedition, false, false)?;
            let is_ascended = player::is_ascended(g, p, expedition)?;
            for biome in php::vals(&new_biomes) {
                let bk = php::key_of(biome);
                let pid_winner = php::get(php::get(php::get(&winners, expedition), &bk), "pId").clone();
                let is_equal = php::truthy(php::coalesce(php::get(php::get(&equality, expedition), &bk), &json!(false)));
                let win = php::loose_eq(&pid_winner, &json!(p)) || (is_equal && is_ascended);
                php::set(php::get_mut(php::get_mut(&mut movements, &ps), side), &bk, json!(if win { 2 } else { 1 }));
                if win {
                    mv = true;
                    winning_biomes.push(biome.clone());
                }
                if is_equal && is_ascended {
                    php::set(php::get_mut(&mut expedition_ascension_moves, &ps), expedition, json!(true));
                }
            }
            if advance && mv {
                php::set(php::get_mut(&mut to_move, &ps), expedition, Value::Array(winning_biomes));
            }
        }
    }

    for (pid_k, player_moves) in php::entries(&to_move) {
        let p = get_by_value(g, &php::key_value(&pid_k)).ok_or_else(|| PhpError::new("Error: Call to a member function isAscended() on null"))?;
        let mut dusk_engine_steps: Vec<Value> = vec![];
        // `$isAscended = $player->isAscended($expedition)` before the loop: a read, overwritten below
        for (expedition, winning_biomes) in php::entries(player_moves) {
            let expedition = expedition.as_str();
            let has_moved_from_ascension = php::truthy(php::coalesce(php::get(php::get(&expedition_ascension_moves, &pid_k), expedition), &json!(false)));
            let is_ascended = player::is_ascended(g, p, expedition)?;
            let side = if expedition == "stormLeft" { "hero" } else { "companion" };
            let mut n = 1;
            if player::has_advance_twice_dusk(g, p, expedition)? > 0 {
                n = player::has_advance_twice_dusk(g, p, expedition)? + 1;
            }
            let aia = php::coalesce(php::get(php::get(&action_instead_advance, &pid_k), expedition), &php::arr()).clone();
            if n > 0 && !php::empty(&aia) {
                let mut nodes: Vec<Value> = vec![];
                let forced_move_args = json!({
                    "pId": p,
                    "expedition": [expedition],
                    "force": true,
                    "forceExpedition": [p, expedition],
                    "winningBiomes": winning_biomes,
                    "ascended": is_ascended,
                    "hasMovedFromAscension": has_moved_from_ascension,
                });
                let move_n = |n: i64| ft::action("MoveExpedition", php::array_merge(&[&forced_move_args, &json!({"n": n})]), json!({"pId": p}));

                // Pegasus is cumulative: all copies apply to a single move instead of being alternatives
                let mut pegasus_cards = vec![];
                for (cid, action) in php::entries(&aia) {
                    if php::loose_eq(action, &json!("PegasusCommon")) {
                        pegasus_cards.push(cid);
                    }
                }

                if !php::in_array(&json!("ErisCommon"), &aia)
                    && !php::in_array(&json!("ErisRare"), &aia)
                    && !php::in_array(&json!("blockMove"), &aia)
                    && !php::in_array(&json!("PegasusCommon"), &aia)
                {
                    nodes.push(move_n(n));
                }
                for (cid, action) in php::entries(&aia) {
                    let cid_v = php::key_value(&cid);
                    let triggered = cards::get(g, &cid_v)?;
                    if php::loose_eq(action, &json!("draw2")) {
                        nodes.push(ft::seq(vec![
                            ft::action("Discard", json!({"cardId": cid_v}), json!({"sourceId": cid_v})),
                            ft::action("Draw", json!({"players": "me", "n": 2}), json!({"pId": p, "sourceId": cid_v})),
                        ]));
                    } else if php::loose_eq(action, &json!("look4")) {
                        nodes.push(ft::seq(vec![
                            ft::action("Discard", json!({"cardId": cid_v}), json!({"sourceId": cid_v})),
                            ft::action("SpecialEffect", json!({"effect": "RunesTestamentLook4"}), json!({"pId": p, "sourceId": cid_v})),
                        ]));
                    } else if php::loose_eq(action, &json!("ErisCommon")) || php::loose_eq(action, &json!("ErisRare")) {
                        let low = if php::loose_eq(action, &json!("ErisCommon")) { "1-3" } else { "2-3" };
                        let gig = php::truthy(&card::is_gigantic(g, triggered, false)?);
                        if (gig && php::count(player_moves) == 2) || !gig {
                            let mut effect = php::arr();
                            php::set(&mut effect, low, move_n(n));
                            php::set(&mut effect, "4+", move_n(n + 1));
                            nodes.push(ft::action("RollDie", json!({"effect": effect}), json!({"sourceId": cid_v, "pId": p})));
                        } else {
                            nodes.push(move_n(n));
                        }
                    } else if php::loose_eq(action, &json!("blockMove")) {
                        // Eris, we need to check if it was triggered, else we do need to move the expedition
                        let tl = card::location(g, triggered)?;
                        if !php::isset(player_moves, &tl) {
                            nodes.push(move_n(n));
                        }
                    }
                }
                if !pegasus_cards.is_empty() {
                    let n_value = if has_moved_from_ascension && is_ascended { n + pegasus_cards.len() as i64 } else { n };
                    nodes.push(move_n(n_value));
                }
                if !nodes.is_empty() {
                    dusk_engine_steps.push(if nodes.len() == 1 { nodes.remove(0) } else { json!({"type": "xor", "childs": nodes, "pId": p}) });
                }
                continue;
            }
            player::advance_storm(g, p, side, winning_biomes, n, has_moved_from_ascension)?;
        }
        if !dusk_engine_steps.is_empty() {
            let step = if dusk_engine_steps.len() == 1 { dusk_engine_steps.remove(0) } else { json!({"type": "seq", "childs": dusk_engine_steps, "pId": p}) };
            engine::push_after_finishing_childs(g, &json!([step]))?;
        }
    }

    Ok(movements)
}

/// `Players::biomesModifier(&$biomes, $player, $expedition, $tiebreak, $excludeMoveInfo)`: the
/// region changes of the played cards (updateExpeditions), then the "opponent X only" cards
// PHP: Managers/Players.php:777
pub fn biomes_modifier(g: &mut Game, biomes: &mut Value, pid: PId, expedition: &str, tiebreak: bool, exclude_move: bool) -> R<()> {
    let token_of = |exp: &str| if exp == "stormLeft" { "hero" } else { "companion" };
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        let ue = card::get(g, c, "updateExpeditions")?;
        if php::empty(&ue) {
            continue;
        }
        let ty = php::coalesce(php::get(&ue, "type"), &json!("")).clone();
        let cpid = card::pid(g, c)?;
        if php::loose_eq(&ty, &json!("all")) {
            update_biomes_modifier(biomes, &ue, tiebreak);
        }
        if php::loose_eq(&ty, &json!("me")) && cpid == pid {
            update_biomes_modifier(biomes, &ue, tiebreak);
        }
        if php::loose_eq(&ty, &json!("opponent")) && cpid != pid {
            update_biomes_modifier(biomes, &ue, tiebreak);
        }

        // manage update of the region of the card
        if php::loose_eq(&ty, &json!("source")) || php::loose_eq(&ty, &json!("sourceAll")) {
            let cp = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function getHeroToken() on null"))?;
            let ctok = if card::location(g, c)? == "stormLeft" { player::get_hero_token(g, cp)? } else { player::get_companion_token(g, cp)? };
            let card_token = g.meeple(ctok)?.location.clone();
            let ptok = player::get_storm_token(g, pid, token_of(expedition))?
                .ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
            let player_token = g.meeple(ptok)?.location.clone();
            if player_token == card_token || php::truthy(&card::is_gigantic(g, c, false)?) || globals::is(g, "tieBreakerMode")? {
                update_biomes_modifier(biomes, &ue, tiebreak);
            }
        }

        if php::loose_eq(&ty, &json!("sourceAll")) {
            // we manage only opposite expedition as the other is managed just above
            let cp = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function getNext() on null"))?;
            let opp = get_next(g, cp)?.ok_or_else(|| PhpError::new("Error: Call to a member function getHeroToken() on null"))?;
            let otok = if card::location(g, c)? == "stormLeft" { player::get_hero_token(g, opp)? } else { player::get_companion_token(g, opp)? };
            let opponent_location = g.meeple(otok)?.location.clone();
            let ptok = player::get_storm_token(g, pid, token_of(expedition))?
                .ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
            let player_token = g.meeple(ptok)?.location.clone();
            if player_token == opponent_location || php::truthy(&card::is_gigantic(g, c, false)?) || globals::is(g, "tieBreakerMode")? {
                update_biomes_modifier(biomes, &ue, tiebreak);
            }
        }
    }

    if !exclude_move {
        // Looped a second time as this takes precedence on the "modifiers"
        for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
            if (card::location(g, c)? == expedition || php::truthy(&card::is_gigantic(g, c, false)?)) && pid != card::pid(g, c)? {
                if card::is(g, c, "opponentOceanOnly")? && php::isset(biomes, "ocean") {
                    *biomes = json!({"ocean": "ocean"});
                } else if card::is(g, c, "opponentForestOnly")? && php::isset(biomes, "forest") {
                    *biomes = json!({"forest": "forest"});
                } else if card::is(g, c, "opponentMountainOnly")? && php::isset(biomes, "mountain") {
                    *biomes = json!({"mountain": "mountain"});
                }
            }
        }
    }
    Ok(())
}

/// `Players::updateBiomesModifier(&$biomes, $updateExpeditions, $tiebreak)`
// PHP: Managers/Players.php:850
pub fn update_biomes_modifier(biomes: &mut Value, update_expeditions: &Value, tiebreak: bool) {
    // remove all the one to remove
    for region in php::vals(php::coalesce(php::get(update_expeditions, "regionsRemove"), &php::arr())) {
        let k = php::key_of(region);
        if php::isset(biomes, &k) {
            php::unset(biomes, &k);
        }
    }
    // add
    for region in php::vals(php::coalesce(php::get(update_expeditions, "regionsAdd"), &php::arr())) {
        let k = php::key_of(region);
        if !tiebreak && !php::in_array(region, biomes) {
            php::set(biomes, &k, region.clone());
        } else if tiebreak && !php::isset(biomes, &k) {
            php::set(biomes, &k, json!(0));
        }
    }
}

/// `Players::getRegionsInfo()`: the visible regions (terrain markers applied), the regions of
/// the expeditions modified by the cards
// PHP: Managers/Players.php:876
pub fn get_regions_info(g: &mut Game) -> R<Value> {
    let mut visible_regions = globals::get_visible_regions(g)?;
    let tiebreak = globals::is(g, "tieBreakerMode")?;
    for v_id in php::keys_str(&visible_regions) {
        // check if there is a terrain marker
        let markers = sort_markers_by_state(g, &format!("storm-{}", v_id))?;
        if !markers.is_empty() {
            for m in markers {
                let t = g.meeple(m)?.kind.clone();
                php::set(&mut visible_regions, &v_id, json!([t]));
            }
        }
    }

    for side in ["hero", "companion"] {
        for p in get_all(g) {
            let tok = player::get_storm_token(g, p, side)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
            let loc = g.meeple(tok)?.location.clone();
            let mut location = php::explode("-", &loc).get(1).cloned().unwrap_or_default();
            let expedition = if side == "hero" { "stormLeft" } else { "stormRight" };
            if globals::is(g, "tieBreakerMode")? {
                location = "4".into();
            }
            if php::isset(&visible_regions, &location) {
                let mut new_biomes = php::arr();
                for b in php::vals(php::get(&visible_regions, &location)) {
                    php::set(&mut new_biomes, &php::key_of(b), b.clone());
                }
                biomes_modifier(g, &mut new_biomes, p, expedition, tiebreak, true)?;
                php::set(&mut visible_regions, &location, php::values(&new_biomes));
            }
        }
    }
    Ok(visible_regions)
}

/// `Meeples::getOfType($location, [OCEAN, FOREST, MOUNTAIN])->sortBy('state')`
pub fn sort_markers_by_state(g: &Game, location: &str) -> R<Vec<i64>> {
    let markers = meeples::get_of_type(g, location, &json!(["ocean", "forest", "mountain"]))?;
    let keys: Vec<String> = markers.iter().map(|m| g.meeples[m].state.clone()).collect();
    Ok(collection::sort_by(&markers, &keys))
}

/// `Players::getWinningPlayerByStorms()`
// PHP: Managers/Players.php:380
pub fn get_winning_player_by_storms(g: &mut Game) -> R<Value> {
    let mut hero: Value = Value::Null;
    let mut hero_pos = json!(-1);
    let mut comp: Value = Value::Null;
    let mut comp_pos = json!(999);
    for p in get_all(g) {
        let c = player::get_companion_token(g, p)?;
        let cn = json!(g.meeple(c)?.location.split('-').nth(1).unwrap_or("").to_string());
        if php::lt(&cn, &comp_pos) {
            comp = json!(p);
            comp_pos = cn;
        } else if php::loose_eq(&cn, &comp_pos) {
            comp = json!(-1);
        }
        let h = player::get_hero_token(g, p)?;
        let hn = json!(g.meeple(h)?.location.split('-').nth(1).unwrap_or("").to_string());
        if php::gt(&hn, &hero_pos) {
            hero = json!(p);
            hero_pos = hn;
        } else if php::loose_eq(&hero_pos, &hn) {
            hero = json!(-1);
        }
    }
    Ok(json!({"stormLeft": hero, "stormRight": comp}))
}
