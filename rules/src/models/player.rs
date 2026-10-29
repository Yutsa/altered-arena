//! Models/Player.php: a player's cards, mana, slots, storm tokens and passive queries

use crate::core::engine;
use crate::core::globals;
use crate::game::{CardId, Game, PId};
use crate::helpers::utils;
use crate::managers::{actions, cards, meeples, players};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `getName()`
pub fn get_name(g: &Game, pid: PId) -> R<String> {
    Ok(g.player(pid)?.name.clone())
}

/// `getPref($prefId)` = `Preferences::get($pId, $prefId)`: the stored value (a string) or null
// PHP: Core/Preferences.php:131
pub fn get_pref(g: &Game, pid: PId, pref: i64) -> Value {
    g.prefs
        .iter()
        .find(|p| p.player_id == pid && p.pref_id == pref)
        .and_then(|p| p.value.clone())
        .map(Value::String)
        .unwrap_or(Value::Null)
}

/// `getDeckCount()`: deck + revealed cards
pub fn get_deck_count(g: &Game, pid: PId) -> R<usize> {
    Ok(cards::count_in_location(g, &format!("deck-{}", pid))? + cards::count_in_location(g, &format!("reveal-{}", pid))?)
}

/// `getDiscardCount()`
pub fn get_discard_count(g: &mut Game, pid: PId) -> R<usize> {
    Ok(cards::get_filtered(g, &json!(pid), &json!("discard"), &Value::Null, false)?.len())
}

/// `hasDeckCards()`
pub fn has_deck_cards(g: &Game, pid: PId) -> R<bool> {
    Ok(get_deck_count(g, pid)? > 0)
}

/// `getHero()`
pub fn get_hero(g: &Game, pid: PId) -> R<Option<CardId>> {
    Ok(cards::get_in_location(g, &format!("board-hero-{}", pid), None)?.first().copied())
}

/// `getHeroCollection()`
pub fn get_hero_collection(g: &Game, pid: PId) -> R<Vec<CardId>> {
    cards::get_in_location(g, &format!("board-hero-{}", pid), None)
}

/// `canTakeAction($action, $ctx)` = `Actions::isDoable`
pub fn can_take_action(g: &mut Game, pid: PId, action: &Value, ctx: Option<usize>) -> R<bool> {
    actions::is_doable(g, action, ctx, Some(pid))
}

/// `draw($nb, $fromLocation, $toLocation, $source, ..., $tapped, $exhaustingPlayer)`: a revealed
/// card is taken first; picked cards come first in the result; never reshuffles
// PHP: Models/Player.php:100
#[allow(clippy::too_many_arguments)]
pub fn draw(g: &mut Game, pid: PId, nb: i64, from: Option<&str>, to: Option<&str>, source: Option<CardId>, tapped: bool, exhausting_player: Option<PId>) -> R<Vec<CardId>> {
    let deck = format!("deck-{}", pid);
    let from = from.map(String::from).unwrap_or_else(|| deck.clone());
    let to = to.unwrap_or("hand").to_string();
    let mut nb = nb;
    let mut picked: Vec<CardId> = vec![];
    let exhausting = exhausting_player.unwrap_or_else(|| players::get_active_id(g));
    let reveal = format!("reveal-{}", pid);
    if from == deck && cards::count_in_location(g, &reveal)? > 0 {
        nb -= 1;
        let rev = cards::get_in_location(g, &reveal, None)?;
        for c in &rev {
            card::set_location(g, *c, &to)?;
        }
        picked = cards::get_many(g, &json!(rev), true)?;
    }
    if nb > 0 {
        let mut p = cards::pick_for_location(g, nb as usize, &from, &to, 0, false)?;
        for c in picked {
            if !p.contains(&c) {
                p.push(c);
            }
        }
        picked = p;
    }
    if tapped {
        for c in &picked {
            if to == "mana" {
                card::set_property(g, *c, "tapped", json!(true), true)?;
            } else {
                let sid = source.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
                let n = json!({"pId": exhausting, "optional": false, "sourceId": sid, "action": "Exhaust", "args": {"cardId": c}});
                engine::insert_as_child(g, &n, None)?;
            }
        }
    }
    if picked.is_empty() {
        return Ok(picked);
    }
    g.notify("drawCards", json!({"player_id": pid, "cardIds": picked, "to": to}));
    Ok(picked)
}

/// `payMana($n)`: taps the first `n` ready mana cards
// PHP: Models/Player.php:176
pub fn pay_mana(g: &mut Game, pid: PId, n: i64) -> R<()> {
    let mut cs = get_mana_cards(g, pid, Some(false))?;
    cs.truncate(n.max(0) as usize);
    if (cs.len() as i64) < n {
        return Err(PhpError::new("You don't have enough mana to pay"));
    }
    for c in cs {
        card::set_property(g, c, "tapped", json!(true), true)?;
    }
    Ok(())
}

/// `getHand($type)`
pub fn get_hand(g: &mut Game, pid: PId, ty: &Value) -> R<Vec<CardId>> {
    let hand = cards::get_hand(g, &json!(pid), &Value::Null)?;
    let mut out = vec![];
    for c in hand {
        if ty.is_null() || php::loose_eq(&card::get(g, c, "type")?, ty) || php::in_array(ty, &card::get(g, c, "additionalType")?) {
            out.push(c);
        }
    }
    Ok(out)
}

/// `getPlayedCards($type)`
pub fn get_played_cards(g: &mut Game, pid: PId, ty: &Value) -> R<Vec<CardId>> {
    cards::get_played_cards(g, &json!(pid), ty)
}

/// `getInfinityCards($type)`: Reserve cards with an effectInfinity
pub fn get_infinity_cards(g: &mut Game, pid: PId, ty: &Value) -> R<Vec<CardId>> {
    let mut out = vec![];
    for c in cards::get_reserve_cards(g, &json!(pid), ty)? {
        if !php::empty(&card::get(g, c, "effectInfinity")?) {
            out.push(c);
        }
    }
    Ok(out)
}

/// `getPlayedCards()->merge($this->getInfinityCards())`
pub fn played_and_infinity(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    let mut a = get_played_cards(g, pid, &Value::Null)?;
    for c in get_infinity_cards(g, pid, &Value::Null)? {
        if !a.contains(&c) {
            a.push(c);
        }
    }
    Ok(a)
}

/// `countCardsInLocation($location, $types)` (`$types` must be an array: a string throws)
// PHP: Models/Player.php:219
pub fn count_cards_in_location(g: &mut Game, pid: PId, location: &str, types: &Value) -> R<usize> {
    let mut n = 0;
    for c in get_played_cards(g, pid, &Value::Null)? {
        if card::location(g, c)? != location {
            continue;
        }
        if !php::is_arr(types) {
            return Err(PhpError::new("TypeError: in_array(): Argument #2 ($haystack) must be of type array, string given"));
        }
        let ty = card::get(g, c, "type")?;
        let at = card::get(g, c, "additionalType")?;
        if php::in_array(&ty, types) || php::count(&php::array_intersect(types, &at)) > 0 {
            n += 1;
        }
    }
    Ok(n)
}

/// `hasPlayedCard($id)`
pub fn has_played_card(g: &mut Game, pid: PId, id: &Value) -> R<bool> {
    cards::has_played_card(g, pid, id)
}

/// `getReserveCards()`
pub fn get_reserve_cards(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    cards::get_reserve_cards(g, &json!(pid), &Value::Null)
}

/// `getAllReserveSlots()`
pub fn get_all_reserve_slots(g: &mut Game, pid: PId) -> R<i64> {
    let mut n = 0;
    for c in get_reserve_cards(g, pid)? {
        if !card::is_tapped(g, c)? {
            n += card::get_int(g, c, "allReserveSlots")?;
        }
    }
    Ok(n)
}

/// `getReserveSlots()`
// PHP: Models/Player.php:247
pub fn get_reserve_slots(g: &mut Game, pid: PId) -> R<i64> {
    let Some(hero) = get_hero(g, pid)? else { return Ok(0) };
    let mut n = php::intval(&card::get_reserve_slots(g, hero)?);
    for c in get_played_cards(g, pid, &Value::Null)? {
        n += php::intval(&card::get_reserve_slots(g, c)?);
    }
    for c in get_reserve_cards(g, pid)? {
        if !card::is_tapped(g, c)? && php::loose_eq(&card::get(g, c, "ignoreReserveLimit")?, &json!(true)) {
            n += 1;
        }
    }
    n += players::get_all_reserve_slots(g)?;
    Ok(n.max(0))
}

/// `getLandmarkSlots()`
// PHP: Models/Player.php:265
pub fn get_landmark_slots(g: &mut Game, pid: PId) -> R<i64> {
    let Some(hero) = get_hero(g, pid)? else { return Ok(0) };
    let mut n = card::get_int(g, hero, "landmarkSlots")?;
    for c in get_landmarks(g, pid)? {
        let completed = card::get(g, c, "effectCompleted")?;
        if !php::is_arr(&completed) {
            continue;
        }
        if !php::in_array(&json!("feat"), &card::get(g, c, "subtypes")?) && !php::key_exists(&completed, "landmarkSlots") {
            continue;
        }
        if meeples::count_meeples(g, &format!("card-{}", c), &json!("featCompleted"))? < 1 {
            continue;
        }
        if php::isset(&completed, "landmarkSlots") {
            n = php::intval(php::get(&completed, "landmarkSlots"));
        }
    }
    Ok(n)
}

/// `getPermanents()`
pub fn get_permanents(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    get_played_cards(g, pid, &json!("permanent"))
}

/// `getLandmarks()`
pub fn get_landmarks(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    let ids = get_played_cards(g, pid, &json!("permanent"))?;
    cards::where_prop(g, ids, "subtypes", &json!("landmark"))
}

/// `getFeats()`
pub fn get_feats(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    let ids = get_played_cards(g, pid, &json!("permanent"))?;
    cards::where_prop(g, ids, "subtypes", &json!("feat"))
}

/// `getCompletedFeat()`
pub fn get_completed_feat(g: &mut Game, pid: PId) -> R<usize> {
    let mut n = 0;
    for c in get_landmarks(g, pid)? {
        n += meeples::count_meeples(g, &format!("card-{}", c), &json!("featCompleted"))?;
    }
    Ok(n)
}

/// `getManaCards($tapped)`
pub fn get_mana_cards(g: &mut Game, pid: PId, tapped: Option<bool>) -> R<Vec<CardId>> {
    let mut out = vec![];
    for c in cards::get_filtered(g, &json!(pid), &json!("mana"), &Value::Null, false)? {
        let t = card::is_tapped(g, c)?;
        if tapped.is_none() || (tapped == Some(true) && t) || (tapped == Some(false) && !t) {
            out.push(c);
        }
    }
    Ok(out)
}

/// `getMana()`: ready mana cards
pub fn get_mana(g: &mut Game, pid: PId) -> R<usize> {
    Ok(get_mana_cards(g, pid, Some(false))?.len())
}

/// `getTotalMana()`
pub fn get_total_mana(g: &mut Game, pid: PId) -> R<usize> {
    Ok(get_mana_cards(g, pid, None)?.len())
}

/// `getStormToken($type)` (null when missing)
pub fn get_storm_token(g: &Game, pid: PId, ty: &str) -> R<Option<i64>> {
    Ok(meeples::get_storm_tokens(g, pid)?.into_iter().find(|m| g.meeples[m].kind == ty))
}

fn token_req(g: &Game, pid: PId, ty: &str) -> R<i64> {
    get_storm_token(g, pid, ty)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null (storm token)"))
}

/// `getCompanionToken()` (used as an object: missing = error)
pub fn get_companion_token(g: &Game, pid: PId) -> R<i64> {
    token_req(g, pid, "companion")
}

/// `getHeroToken()` (used as an object: missing = error)
pub fn get_hero_token(g: &Game, pid: PId) -> R<i64> {
    token_req(g, pid, "hero")
}

/// `isAscended($expedition)`
pub fn is_ascended(g: &Game, pid: PId, expedition: &str) -> R<bool> {
    Ok(!meeples::get_ascended(g, pid, expedition)?.is_empty())
}

/// `getDeck($deckNumber)`
pub fn get_deck(g: &mut Game, pid: PId, deck: &str) -> R<Vec<CardId>> {
    let mut a = cards::get_filtered(g, &json!(pid), &json!(format!("deck-{}", deck)), &Value::Null, false)?;
    for c in cards::get_filtered(g, &json!(pid), &json!(format!("board-hero-{}", deck)), &Value::Null, false)? {
        if !a.contains(&c) {
            a.push(c);
        }
    }
    Ok(a)
}

fn sum_prop(g: &mut Game, list: Vec<CardId>, prop: &str) -> R<i64> {
    let mut n = 0;
    for c in list {
        n += card::get_int(g, c, prop)?;
    }
    Ok(n)
}

/// `getAddRoll()`
pub fn get_add_roll(g: &mut Game, pid: PId) -> R<i64> {
    let l = played_and_infinity(g, pid)?;
    sum_prop(g, l, "addRoll")
}

/// `getAddDice()`
pub fn get_add_dice(g: &mut Game, pid: PId) -> R<i64> {
    let l = played_and_infinity(g, pid)?;
    sum_prop(g, l, "addDice")
}

/// `getReserveAdd()`
pub fn get_reserve_add(g: &mut Game, pid: PId) -> R<i64> {
    let l = get_played_cards(g, pid, &Value::Null)?;
    sum_prop(g, l, "reserveAdd")
}

/// `getResupply2()`
pub fn get_resupply2(g: &mut Game, pid: PId) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        if php::truthy(&card::get(g, c, "resupply2")?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `getReduceCostType($playedCard)`: `['reduction' => n, 'minimum' => floor]`
// PHP: Models/Player.php:433
pub fn get_reduce_cost_type(g: &mut Game, pid: PId, played: CardId) -> R<Value> {
    let mut reduction = 0i64;
    let mut floor = 0i64;
    let pty = card::get(g, played, "type")?;
    let pat = card::get(g, played, "additionalType")?;
    for c in played_and_infinity(g, pid)? {
        let rct = card::get(g, c, "reduceCostType")?;
        if php::empty(&rct) {
            continue;
        }
        for (k, info) in php::entries(&rct) {
            let kv = php::key_value(&k);
            if !(php::loose_eq(&kv, &pty) || php::in_array(&kv, &pat)) {
                continue;
            }
            let ch = card::get(g, played, "costHand")?;
            let red = php::intval(php::get(info, "reduction"));
            let min = php::get(info, "minimum").clone();
            let apply = if php::isset(info, "maxHandCost") && php::le(&ch, php::get(info, "maxHandCost")) {
                true
            } else if php::isset(info, "minHandCost") && php::ge(&ch, php::get(info, "minHandCost")) {
                true
            } else if php::isset(info, "minBaseCost") {
                let base = php::get(info, "minBaseCost").clone();
                let loc = card::location(g, played)?;
                if loc == "reserve" && php::ge(&card::get(g, played, "costReserve")?, &base) {
                    true
                } else {
                    loc == "hand" && php::ge(&ch, &base)
                }
            } else {
                false
            };
            if apply {
                reduction += red;
                if !min.is_null() {
                    floor = floor.max(php::intval(&min));
                }
            }
        }
    }
    Ok(json!({"reduction": reduction, "minimum": floor}))
}

/// `getExhaustedReserveSlots()`
pub fn get_exhausted_reserve_slots(g: &mut Game, pid: PId) -> R<i64> {
    let l = played_and_infinity(g, pid)?;
    sum_prop(g, l, "exhaustedReserveSlots")
}

/// `getMinimumReserveCost()`
pub fn get_minimum_reserve_cost(g: &mut Game, pid: PId) -> R<i64> {
    let mut n = 0;
    for c in played_and_infinity(g, pid)? {
        n += php::intval(&card::get_minimum_reserve_cost(g, c)?);
    }
    Ok(n)
}

fn token_pos(g: &Game, m: i64) -> R<i64> {
    Ok(php::intval(&json!(g.meeple(m)?.location.split('-').nth(1).unwrap_or("").to_string())))
}

/// `getRegionDifference()`
pub fn get_region_difference(g: &Game, pid: PId) -> R<i64> {
    let Some(c) = get_storm_token(g, pid, "companion")? else { return Ok(7) };
    let h = get_hero_token(g, pid)?;
    Ok((token_pos(g, c)? - token_pos(g, h)?).max(0))
}

/// `checkVictory()`: `[crossed, moves]` (7 when there is no companion token)
// PHP: Models/Player.php:502
pub fn check_victory(g: &Game, pid: PId) -> R<Value> {
    let Some(c) = get_storm_token(g, pid, "companion")? else { return Ok(json!(7)) };
    let h = get_hero_token(g, pid)?;
    let (cp, hp) = (token_pos(g, c)?, token_pos(g, h)?);
    Ok(json!([cp - hp <= 0, hp + (7 - cp)]))
}

/// `getBiomeInStorms()`: biomes of the regions of the hero and companion tokens
// PHP: Models/Player.php:512
pub fn get_biome_in_storms(g: &mut Game, pid: PId) -> R<Value> {
    let tokens = meeples::get_storm_tokens(g, pid)?;
    let storms = globals::get(g, "storm")?;
    if globals::is(g, "tieBreakerMode")? {
        return Ok(json!({"hero": ["forest", "mountain", "ocean"], "companion": ["forest", "mountain", "ocean"]}));
    }
    let mut locations = php::arr();
    for t in tokens {
        let m = g.meeple(t)?.clone();
        let sid = php::intval(&json!(m.location.split('-').nth(1).unwrap_or("").to_string()));
        let markers = meeples::get_of_type(g, &format!("storm-{}", sid), &json!(["ocean", "forest", "mountain"]))?;
        if !markers.is_empty() {
            for mk in markers {
                let k = g.meeple(mk)?.kind.clone();
                php::set(&mut locations, &m.kind, json!([k]));
            }
            continue;
        }
        if sid == 0 || sid == 7 {
            php::set(&mut locations, &m.kind, json!(["forest", "mountain", "ocean"]));
            continue;
        }
        let c = php::geti(&storms, (sid + 1) / 2).clone();
        let mut storm = globals::storm_card(php::intval(php::get(&c, "cardId")));
        if php::truthy(php::get(&c, "rotated")) {
            storm.reverse();
        }
        let b = &storm[((sid - 1) % 2) as usize];
        php::set(&mut locations, &m.kind, php::str_list(b));
    }
    Ok(locations)
}

/// `hasBlockingPower($expedition, $isGigantic)`
// PHP: Models/Player.php:822
pub fn has_blocking_power(g: &mut Game, pid: PId, expedition: &str, is_gigantic: bool) -> R<bool> {
    for c in played_and_infinity(g, pid)? {
        if card::location(g, c)? != expedition && !php::truthy(&card::is_gigantic(g, c, false)?) && !is_gigantic {
            continue;
        }
        if card::is_blocking_power(g, c)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `isInBiome($storm, $biome, $excludeMove)`
// PHP: Models/Player.php:555
pub fn is_in_biome(g: &mut Game, pid: PId, storm: &str, biome: &str, exclude_move: bool) -> R<bool> {
    let biomes = get_biomes(g, pid, storm, exclude_move)?;
    if !php::is_arr(&biomes) {
        return Err(PhpError::new("TypeError: in_array(): Argument #2 ($haystack) must be of type array, bool given"));
    }
    Ok(php::in_array(&json!(biome), &biomes))
}

/// `hasGigantic()`
pub fn has_gigantic(g: &mut Game, pid: PId) -> R<bool> {
    for c in played_and_infinity(g, pid)? {
        if php::truthy(&card::is_gigantic(g, c, false)?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `hasExpeditionSeasoned($expedition)`
pub fn has_expedition_seasoned(g: &mut Game, pid: PId, expedition: Option<&str>) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        if let Some(e) = expedition {
            if card::location(g, c)? != e {
                continue;
            }
        }
        if card::is(g, c, "expeditionSeasoned")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `hasExpeditionToughBoosted($expedition)`
pub fn has_expedition_tough_boosted(g: &mut Game, pid: PId, expedition: Option<&str>) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        if let Some(e) = expedition {
            if card::location(g, c)? != e {
                continue;
            }
        }
        if php::loose_eq(&card::get(g, c, "expeditionTough")?, &json!("boosted")) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `hasBlockOpponentReserveGain()`
pub fn has_block_opponent_reserve_gain(g: &mut Game, pid: PId) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        if card::is(g, c, "blockOpponentReserveGain")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `hasBlockGainNewCounters()`
pub fn has_block_gain_new_counters(g: &mut Game, pid: PId) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        if card::is(g, c, "blockGainNewCounters")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `canPlayTappedCards($type, $location, $additionalType)`
// PHP: Models/Player.php:1022
pub fn can_play_tapped_cards(g: &mut Game, pid: PId, ty: &Value, location: &Value, additional: &Value) -> R<bool> {
    for c in played_and_infinity(g, pid)? {
        let play_tap = card::get(g, c, "playTappedCards")?;
        let mut chars = card::get(g, c, "playTappedCharacters")?;
        let mut all = card::get(g, c, "playTappedAllCards")?;
        if !all.is_boolean() && !php::is_arr(&all) {
            all = json!(php::loose_eq(&utils::check_attribute_condition(g, "tough", &all, Some(pid), c, &Value::Null)?, &json!("1")));
        }
        if !chars.is_boolean() && !php::is_arr(&chars) {
            chars = json!(php::loose_eq(&utils::check_attribute_condition(g, "tough", &chars, Some(pid), c, &Value::Null)?, &json!("1")));
        }
        if !php::truthy(&chars) && !php::truthy(&all) && (play_tap.is_null() || php::empty(&play_tap)) {
            continue;
        }
        if (php::isset(&play_tap, "type") && php::loose_eq(php::get(&play_tap, "type"), &json!("all"))) || php::truthy(&all) {
            return Ok(true);
        }
        if php::isset(&play_tap, "location") && !location.is_null() && php::loose_ne(location, &json!(card::location(g, c)?)) {
            continue;
        }
        if php::truthy(&chars) && php::loose_eq(ty, &json!("character")) {
            return Ok(true);
        }
        if !ty.is_null() && php::isset(&play_tap, "type") && (php::loose_eq(php::get(&play_tap, "type"), ty) || php::in_array(php::get(&play_tap, "type"), additional)) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn penalty_list(v: Value) -> Value {
    if php::is_arr(&v) {
        v
    } else {
        json!([v])
    }
}

/// `getOpponentAdditionalCost($type)`
// PHP: Models/Player.php:1070
pub fn get_opponent_additional_cost(g: &mut Game, pid: PId, ty: &Value) -> R<i64> {
    let prop = format!("increaseOpponent{}Cost", php::ucfirst(&php::strval(ty)));
    let mut cost = 0;
    for c in played_and_infinity(g, pid)? {
        let penalty = card::get(g, c, &prop)?;
        if penalty.is_i64() {
            cost += php::intval(&penalty);
        } else if !utils::check_attribute_condition(g, "tough", &penalty, Some(pid), c, &Value::Null)?.is_null() {
            cost += php::intval(&json!(php::explode(":", &php::strval(&penalty))[0]));
        }
        for p in php::vals(&penalty_list(card::get(g, c, "increaseOpponentCardsCost")?)) {
            if p.is_i64() {
                cost += php::intval(p);
            } else if !utils::check_attribute_condition(g, "tough", p, Some(pid), c, &Value::Null)?.is_null() {
                cost += php::intval(&json!(php::explode(":", &php::strval(p))[0]));
            }
        }
    }
    Ok(cost)
}

/// `getOpponentMinimumCost($type)`
// PHP: Models/Player.php:1101
pub fn get_opponent_minimum_cost(g: &mut Game, pid: PId, ty: &Value) -> R<i64> {
    let mut cost = 0i64;
    for c in played_and_infinity(g, pid)? {
        if php::loose_eq(ty, &json!("character")) {
            let penalty = card::get(g, c, "opponentCharactersMinimumCost")?;
            if penalty.is_i64() {
                cost = cost.max(php::intval(&penalty));
            } else if !utils::check_attribute_condition(g, "tough", &penalty, Some(pid), c, &Value::Null)?.is_null() {
                cost = cost.max(php::intval(&json!(php::explode(":", &php::strval(&penalty))[0])));
            }
        }
        for p in php::vals(&penalty_list(card::get(g, c, "opponentCardsMinimumCost")?)) {
            if p.is_i64() {
                cost = cost.max(php::intval(p));
            } else if !utils::check_attribute_condition(g, "tough", p, Some(pid), c, &Value::Null)?.is_null() {
                cost = cost.max(php::intval(&json!(php::explode(":", &php::strval(p))[0])));
            }
        }
    }
    Ok(cost)
}

/// `countUniversalTokenGigantic()`
// PHP: Models/Player.php:1270
pub fn count_universal_token_gigantic(g: &mut Game, pid: PId) -> R<usize> {
    let mut n = 0;
    for c in get_played_cards(g, pid, &Value::Null)? {
        let dg = card::get(g, c, "dynamicGigantic")?;
        let list = if !php::is_arr(&dg) && php::loose_ne(&dg, &json!("")) {
            json!([dg])
        } else if php::loose_eq(&dg, &json!("")) {
            json!([])
        } else {
            dg
        };
        let mut res = false;
        for s in php::vals(&list) {
            let split = php::explode(":", &php::strval(s));
            if split.len() > 1 {
                if !utils::check_attribute_condition(g, "gigantic", s, Some(pid), c, &Value::Null)?.is_null() {
                    res = split[0] == "universalGiganticToken";
                    break;
                }
            } else {
                res = php::loose_eq(s, &json!("universalGiganticToken"));
                break;
            }
        }
        if res {
            n += 1;
        }
    }
    Ok(n)
}

/// `isInContact($location)`: a storm token in the same region as an opponent's token
// PHP: Models/Player.php:1296
pub fn is_in_contact(g: &mut Game, pid: PId, location: Option<&str>) -> R<bool> {
    let opp = players::get_next_id(g, pid)?;
    let opp_tokens = meeples::get_storm_tokens(g, opp)?;
    let toks: Vec<&str> = match location {
        None => {
            if has_override_in_contact(g, pid, "stormLeft")? || has_override_in_contact(g, pid, "stormRight")? {
                return Ok(true);
            }
            vec!["hero", "companion"]
        }
        Some("stormLeft") => {
            if has_override_in_contact(g, pid, "stormLeft")? {
                return Ok(true);
            }
            vec!["hero"]
        }
        Some("stormRight") => {
            if has_override_in_contact(g, pid, "stormRight")? {
                return Ok(true);
            }
            vec!["companion"]
        }
        _ => return Ok(false),
    };
    for t in toks {
        let m = token_req(g, pid, t)?;
        let loc = g.meeple(m)?.location.clone();
        for o in &opp_tokens {
            if loc == g.meeple(*o)?.location {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `hasOverrideInContact($location)`
pub fn has_override_in_contact(g: &mut Game, pid: PId, location: &str) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        let loc = card::location(g, c)?;
        if (loc == location || (php::truthy(&card::is_gigantic(g, c, false)?) && (loc == "stormLeft" || loc == "stormRight"))) && card::is(g, c, "overrideContact")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `hasOverrideBehind($location)`
pub fn has_override_behind(g: &mut Game, pid: PId, location: &str) -> R<bool> {
    for c in get_played_cards(g, pid, &Value::Null)? {
        let loc = card::location(g, c)?;
        if (loc == location || (php::truthy(&card::is_gigantic(g, c, false)?) && (loc == "stormLeft" || loc == "stormRight"))) && card::is(g, c, "overrideBehind")? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `advanceStorm($token, $biomes, $n, $hasMovedFromAscension, $notify, $source)`
// PHP: Models/Player.php:578
pub fn advance_storm(g: &mut Game, pid: PId, token: &str, biomes: &Value, n: i64, has_moved_from_ascension: bool) -> R<bool> {
    let m = token_req(g, pid, token)?;
    let location = token_pos(g, m)?;
    let expedition = if token == "hero" { "stormLeft" } else { "stormRight" };
    let ascended = is_ascended(g, pid, expedition)?;
    let delta = if token == "hero" { n } else { -n };
    let sid = if token == "hero" { (location + delta).max(0).min(7) } else { (location + delta).min(7).max(0) };
    if sid == location {
        g.notify("message", json!({"log": "${player_name} expedition cannot move", "player_id": pid}));
        return Ok(false);
    }
    g.meeple_mut(m)?.location = format!("storm-{}", sid);
    let mut moves = globals::get(g, "stormMoves")?;
    let entry = json!({"biomes": if php::is_arr(biomes) { biomes.clone() } else { json!([]) }, "moves": n, "ascended": ascended, "hasMovedFromAscension": has_moved_from_ascension});
    php::set(php::get_mut(&mut moves, &pid.to_string()), expedition, entry);
    globals::set(g, "stormMoves", moves)?;
    let mut storms = globals::get(g, "storm")?;
    let idx = ((sid + 1) / 2).to_string();
    if !php::truthy(php::get(php::get(&storms, &idx), "visible")) {
        php::set(php::get_mut(&mut storms, &idx), "visible", json!(true));
        globals::set(g, "storm", storms)?;
    }
    g.notify("moveStormToken", json!({"player_id": pid, "token": token, "to": sid}));
    Ok(true)
}

/// `nightCleanup()` (Models/Player.php:626) — not translated yet
pub fn night_cleanup(_g: &mut Game, _pid: PId) -> R<Value> {
    Err(PhpError::nt("Models/Player.php:626 nightCleanup"))
}

/// `getBiomeStrength($expeditions, $includeModifiers)` (Models/Player.php:771) — not translated yet
pub fn get_biome_strength(_g: &mut Game, _pid: PId) -> R<Value> {
    Err(PhpError::nt("Models/Player.php:771 getBiomeStrength"))
}

/// `getBiomes($storm, $excludeMove)` (Models/Player.php:560) — needs Players::biomesModifier
pub fn get_biomes(g: &mut Game, pid: PId, storm: &str, exclude_move: bool) -> R<Value> {
    let biomes = get_biome_in_storms(g, pid)?;
    if storm.is_empty() {
        return Ok(json!(false));
    }
    let exp = if storm == "stormLeft" { "hero" } else { "companion" };
    let mut nb = php::arr();
    for b in php::vals(php::get(&biomes, exp)) {
        php::set(&mut nb, &php::strval(b), b.clone());
    }
    players::biomes_modifier(g, &mut nb, pid, storm, false, exclude_move)?;
    Ok(nb)
}
