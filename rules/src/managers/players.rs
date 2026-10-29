//! Managers/Players.php: player lookup and order, opponent powers, storm and victory

use crate::core::globals;
use crate::framework;
use crate::game::{CardId, Game, PId};
use crate::managers::cards;
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

/// `Players::hasOpponentBlockingPower($player, $expedition, $isGigantic)`
// PHP: Managers/Players.php:156
pub fn has_opponent_blocking_power(g: &mut Game, pid: PId, expedition: &str, is_gigantic: bool) -> R<bool> {
    for p2 in get_all(g) {
        if p2 == pid {
            continue;
        }
        if player::has_blocking_power(g, p2, expedition, is_gigantic)? {
            return Ok(true);
        }
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

/// `Players::computeStorm($advance)` (Managers/Players.php:572) — not translated yet
pub fn compute_storm(_g: &mut Game, _advance: bool) -> R<Value> {
    Err(PhpError::nt("Managers/Players.php:572 computeStorm"))
}

/// `Players::getBiomeTotals()` (Managers/Players.php:316) — not translated yet
pub fn get_biome_totals(_g: &mut Game) -> R<Value> {
    Err(PhpError::nt("Managers/Players.php:316 getBiomeTotals"))
}

/// `Players::getDefenders($onlyDefenders)` (Managers/Players.php:326) — not translated yet
pub fn get_defenders(_g: &mut Game, _only: bool) -> R<Value> {
    Err(PhpError::nt("Managers/Players.php:326 getDefenders"))
}

/// `Players::getBlockedExpeditions()` (Managers/Players.php:459) — not translated yet
pub fn get_blocked_expeditions(_g: &mut Game) -> R<Value> {
    Err(PhpError::nt("Managers/Players.php:459 getBlockedExpeditions"))
}

/// `Players::biomesModifier(&$biomes, $player, $expedition, ...)` (Managers/Players.php:777) — not
/// translated yet
pub fn biomes_modifier(_g: &mut Game, _biomes: &mut Value, _pid: PId, _expedition: &str, _tie: bool, _exclude_move: bool) -> R<()> {
    Err(PhpError::nt("Managers/Players.php:777 biomesModifier"))
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
