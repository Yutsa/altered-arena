//! Models/Card.php: the card object (magic property getters, location helpers, listeners,
//! playability and costs)

use crate::core::engine;
use crate::core::globals;
use crate::game::{CardId, Game, PId};
use crate::helpers::{conditions, utils};
use crate::managers::{cards, meeples, players};
use crate::models::player;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};
use std::sync::Arc;

/// Models/Card.php:42-204 `$propertiesAttributes`
pub const PROPERTY_TYPES: &[(&str, &str)] = &[
    ("equinoxId","int"),
    ("faction","str"),
    ("name","str"),
    ("type","str"),
    ("typeline","str"),
    ("subtypes","obj"),
    ("token","bool"),
    ("uid","str"),
    ("flavorText","str"),
    ("effectDesc","str"),
    ("supportDesc","str"),
    ("supportIcon","str"),
    ("artist","str"),
    ("setIcon","str"),
    ("serial","str"),
    ("thumbnail","int"),
    ("statData","int"),
    ("extension","str"),
    ("fullArt","bool"),
    ("rarity","int"),
    ("asset","str"),
    ("mainAsset","str"),
    ("frame","int"),
    ("reserveSlots","int"),
    ("landmarkSlots","int"),
    ("mountain","int"),
    ("forest","int"),
    ("ocean","int"),
    ("costModifier","obj"),
    ("costHand","int"),
    ("costReserve","int"),
    ("costReductionDiscard","int"),
    ("dynamicCostReduction","str"),
    ("effectPlayed","obj"),
    ("effectHand","obj"),
    ("effectReserve","obj"),
    ("effectSupport","obj"),
    ("effectPassive","obj"),
    ("effectCompleted","obj"),
    ("effectTap","obj"),
    ("effectInfinity","obj"),
    ("gigantic","bool"),
    ("dynamicGigantic","str"),
    ("fleeting","bool"),
    ("seasoned","bool"),
    ("minManaOrbs","int"),
    ("defender","bool"),
    ("dynamicDefender","str"),
    ("oppositeDefender","bool"),
    ("dynamicOppositeDefender","str"),
    ("eternal","bool"),
    ("blockingPower","bool"),
    ("dynamicBlockingPower","str"),
    ("increaseOpponentCardsCost","int"),
    ("increaseOpponentCharacterCost","int"),
    ("increaseOpponentSpellCost","int"),
    ("increaseOpponentPermanentCost","int"),
    ("increaseOpponentTokenCost","int"),
    ("opponentCharactersMinimumCost","int"),
    ("opponentCardsMinimumCost","int"),
    ("sacrificeAndNotFleetingGoToReserve","bool"),
    ("sacrificeAndFleetingDraw","bool"),
    ("updateExpeditions","obj"),
    ("blockAutomaticAction","obj"),
    ("addDice","int"),
    ("resupply2","bool"),
    ("leaveExpeditionToMana","bool"),
    ("leaveExpeditionToManaOrDraw","bool"),
    ("leaveExpeditionBoostedToMana","bool"),
    ("tough","int"),
    ("dynamicTough","str"),
    ("excludeUniversalTough","bool"),
    ("excludeSelfTough","bool"),
    ("dynamicEternal","str"),
    ("addRoll","int"),
    ("tapped","bool"),
    ("extraDatas","obj"),
    ("playTappedCards","obj"),
    ("playTappedCharacters","bool"),
    ("playTappedAllCards","bool"),
    ("canAlwaysGainFleeting","bool"),
    ("dynamicGainReplace","obj"),
    ("defenderIgnoreBehind","bool"),
    ("ignoreDefender","bool"),
    ("dynamicIgnoreDefender","str"),
    ("cooldown","bool"),
    ("exhaustedReserveSlots","int"),
    ("costReductionIfEmpty","int"),
    ("giganticOneCharacter","bool"),
    ("opponentOceanOnly","bool"),
    ("opponentMountainOnly","bool"),
    ("opponentForestOnly","bool"),
    ("increaseBiomesHighest","bool"),
    ("advanceTwiceDusk","bool"),
    ("protectAnchoredInExpedition","bool"),
    ("protectBoostedInExpedition","bool"),
    ("increaseReserveCost","int"),
    ("dynamicIncreaseReserveCost","str"),
    ("reduceReserveCost","int"),
    ("dynamicReduceReserveCost","str"),
    ("dynamicMinimumReserveCost","str"),
    ("exhaustCharactersMorning","bool"),
    ("resupplyExhaust","bool"),
    ("scout","int"),
    ("dynamicSeasoned","str"),
    ("expeditionSeasoned","bool"),
    ("increaseAllOtherCharactersBiomesHighest","bool"),
    ("ignoreReserveLimit","bool"),
    ("blockOpponentReserveGain","bool"),
    ("blockGainNewCounters","bool"),
    ("allReserveSlots","int"),
    ("actionInsteadAdvance","str"),
    ("dynamicReserveSlots","str"),
    ("reduceCostType","obj"),
    ("allSpell1Fleeting","bool"),
    ("costReductionSacrificePermanent","int"),
    ("revealed","bool"),
    ("createMarkers","bool"),
    ("dynamicIncreaseBiomeHighestSelf","str"),
    ("costReductionLimitation","int"),
    ("effectPlayedLimited","obj"),
    ("additionalType","obj"),
    ("resupplyIfAscended","bool"),
    ("blockMoveExpedition","bool"),
    ("reserveAdd","int"),
    ("expeditionTough","str"),
    ("playLimitation","str"),
    ("leaveExpeditionDefect","bool"),
    ("overrideContact","bool"),
    ("overrideBehind","bool"),
    ("defenderIgnoreContact","bool"),
    ("costReductionTap","int"),
    ("playCondition","str"),
    ("cantGainBoost","str"),
    ("boostIfAscended","bool"),
];

pub fn prop_type(name: &str) -> Option<&'static str> {
    PROPERTY_TYPES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

fn default_of(t: &str) -> Value {
    match t {
        "int" => json!(0),
        "bool" => json!(false),
        "str" => json!(""),
        _ => json!([]),
    }
}

/// The magic getter `get<Name>($arg)` (Models/Card.php:231-286): the property if set, else the
/// `effectInfinity` value for an untapped Reserve card, else the type's default; DB attributes
/// (id, location, state, pId, properties) otherwise
// PHP: Models/Card.php:231
pub fn get_impl(g: &mut Game, id: CardId, name: &str, arg: Option<&str>) -> R<Value> {
    let c = g.card(id)?;
    match name {
        "id" => return Ok(json!(c.id)),
        "location" => return Ok(json!(c.location)),
        "state" => return Ok(json!(c.state)),
        "pId" => return Ok(json!(c.pid)),
        "properties" => {
            let p = Value::Object((*c.props).clone());
            return Ok(match arg {
                Some(a) => php::get(&p, a).clone(),
                None => p,
            });
        }
        _ => {}
    }
    let Some(t) = prop_type(name) else {
        return Err(PhpError::new(format!("InvalidArgumentException: Attribute {} doesn't exist", name)));
    };
    let v = c.props.get(name).cloned().unwrap_or(Value::Null);
    if !v.is_null() {
        if let (Some(a), "obj", true) = (arg, t, php::is_arr(&v)) {
            return Ok(php::get(&v, a).clone());
        }
        return Ok(v);
    }
    if name != "effectInfinity" && c.location == "reserve" && !is(g, id, "tapped")? {
        let c = g.card(id)?;
        let inf = c.props.get("effectInfinity").cloned().unwrap_or(Value::Null);
        let x = php::get(&inf, name).clone();
        if !x.is_null() {
            if let (Some(a), "obj", true) = (arg, t, php::is_arr(&x)) {
                return Ok(php::get(&x, a).clone());
            }
            return Ok(x);
        }
    }
    Ok(default_of(t))
}

/// `get<Name>()`
pub fn get(g: &mut Game, id: CardId, name: &str) -> R<Value> {
    get_impl(g, id, name, None)
}

/// `get<Name>()` as a string
pub fn get_str(g: &mut Game, id: CardId, name: &str) -> R<String> {
    Ok(php::strval(&get(g, id, name)?))
}

/// `get<Name>()` as an int
pub fn get_int(g: &mut Game, id: CardId, name: &str) -> R<i64> {
    Ok(php::intval(&get(g, id, name)?))
}

/// `is<Name>()`: `(bool) properties[name]`, false when unset (no effectInfinity fallback)
pub fn is(g: &Game, id: CardId, name: &str) -> R<bool> {
    let c = g.card(id)?;
    match name {
        "id" => return Ok(c.id != 0),
        "state" => return Ok(c.state != 0),
        "pId" => return Ok(c.pid != 0),
        "location" => return Ok(php::loose_ne(&json!(c.location), &json!(0))),
        _ => {}
    }
    if prop_type(name).is_none() {
        return Err(PhpError::new(format!("InvalidArgumentException: Attribute {} doesn't exist", name)));
    }
    Ok(php::truthy(c.props.get(name).unwrap_or(&Value::Null)))
}

/// `getProperty($variable)`
pub fn get_property(g: &Game, id: CardId, name: &str) -> R<Value> {
    Ok(g.card(id)?.props.get(name).cloned().unwrap_or(Value::Null))
}

/// `setProperty($variable, $value, $updateDB)` (also the magic `set<Name>`): writing stores the
/// whole properties array in the DB
// PHP: Models/Card.php:293
pub fn set_property(g: &mut Game, id: CardId, name: &str, v: Value, update_db: bool) -> R<()> {
    let c = g.card_mut(id)?;
    Arc::make_mut(&mut c.props).insert(name.to_string(), v);
    if update_db {
        c.db_props = c.props.clone();
    }
    c.reload = true;
    Ok(())
}

/// `setLocation($location)`
pub fn set_location(g: &mut Game, id: CardId, loc: &str) -> R<()> {
    g.card_mut(id)?.location = loc.to_string();
    Ok(())
}

/// `setState($state)` (int)
pub fn set_state(g: &mut Game, id: CardId, s: i64) -> R<()> {
    g.card_mut(id)?.state = s;
    Ok(())
}

/// `setPId($pId)` (int)
pub fn set_pid(g: &mut Game, id: CardId, p: &Value) -> R<()> {
    g.card_mut(id)?.pid = php::intval(p);
    Ok(())
}

/// The value `Collection::where` reads (`get<Field>()`)
pub fn get_field(g: &mut Game, id: CardId, field: &str) -> R<Value> {
    get(g, id, field)
}

pub fn location(g: &Game, id: CardId) -> R<String> {
    Ok(g.card(id)?.location.clone())
}

pub fn pid(g: &Game, id: CardId) -> R<PId> {
    Ok(g.card(id)?.pid)
}

/// `isTapped()`
pub fn is_tapped(g: &Game, id: CardId) -> R<bool> {
    is(g, id, "tapped")
}

/// `isToken()`
pub fn is_token(g: &Game, id: CardId) -> R<bool> {
    is(g, id, "token")
}

/// `getSubtypes()`
pub fn get_subtypes(g: &mut Game, id: CardId) -> R<Value> {
    get(g, id, "subtypes")
}

/// `getAdditionalType()`
pub fn get_additional_type(g: &mut Game, id: CardId) -> R<Value> {
    get(g, id, "additionalType")
}

/// `isPlayed()`: expeditions, landmark, or a hero
// PHP: Models/Card.php:317
pub fn is_played(g: &Game, id: CardId) -> R<bool> {
    let c = g.card(id)?;
    Ok(c.location == "stormLeft" || c.location == "stormRight" || c.location == "landmark" || php::loose_eq(c.props.get("type").unwrap_or(&Value::Null), &json!("hero")))
}

/// `getPlayer()`: the controller (null when no such player)
pub fn get_player(g: &mut Game, id: CardId) -> R<Option<PId>> {
    let p = g.card(id)?.pid;
    Ok(players::get_by_value(g, &json!(p)))
}

fn player_req(g: &mut Game, id: CardId) -> R<PId> {
    get_player(g, id)?.ok_or_else(|| PhpError::new("Error: Call to a member function on null (card player)"))
}

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

fn sub(cost: &Value, x: i64) -> Value {
    json!(php::intval(cost) - x)
}

/// `canBePlayed($player, $scout, $reserveFlipCost)`
// PHP: Models/Card.php:340
pub fn can_be_played(g: &mut Game, id: CardId, player: PId, scout: bool, reserve_flip_cost: bool) -> R<bool> {
    if is_exhausted_reserve_play_blocked(g, id, player)? {
        return Ok(false);
    }
    for _ in 0..2 {
        let pc = get(g, id, "playCondition")?;
        if php::loose_ne(&pc, &Value::Null) && !conditions::check(g, &json!({"condition": pc}), Some(id), &Value::Null)? {
            return Ok(false);
        }
    }
    let mut cost = get_cost(g, id, scout, reserve_flip_cost, None)?;
    let cri = get_int(g, id, "costReductionIfEmpty")?;
    let mana = player::get_mana(g, player)? as i64;
    let total_mana = player::get_total_mana(g, player)? as i64;
    if cri > 0 {
        let tc = json!(["token", "character"]);
        if (player::count_cards_in_location(g, player, "stormLeft", &tc)? == 0 || player::count_cards_in_location(g, player, "stormRight", &tc)? == 0) && !player::has_gigantic(g, player)? {
            cost = sub(&cost, cri);
        }
    }
    let crd = get_int(g, id, "costReductionDiscard")?;
    if crd > 0 {
        let p = player_req(g, id)?;
        let n = player::get_reserve_cards(g, p)?.len();
        if location(g, id)? == "reserve" && n >= 2 {
            cost = sub(&cost, crd);
        } else if n >= 1 {
            cost = sub(&cost, crd);
        }
    }
    let crt = get_int(g, id, "costReductionTap")?;
    if crt > 0 {
        let p = player_req(g, id)?;
        let mut n = 0;
        for c in player::get_reserve_cards(g, p)? {
            if !is_tapped(g, c)? {
                n += 1;
            }
        }
        if location(g, id)? == "reserve" && n >= 2 {
            cost = sub(&cost, crt);
        } else if n >= 1 {
            cost = sub(&cost, crt);
        }
    }
    let crsp = get_int(g, id, "costReductionSacrificePermanent")?;
    if crsp > 0 {
        let p = player_req(g, id)?;
        if !player::get_played_cards(g, p, &json!("permanent"))?.is_empty() {
            cost = sub(&cost, crsp);
        }
    }
    let crl = get_int(g, id, "costReductionLimitation")?;
    if crl > 0 {
        cost = sub(&cost, crl);
    }
    let pl = get(g, id, "playLimitation")?;
    if php::loose_eq(&pl, &json!("-2Contact")) && player::is_in_contact(g, player, None)? {
        cost = sub(&cost, 2);
    } else if php::loose_eq(&pl, &json!("-2Multi")) {
        let opp = players::get_next_id(g, player)?;
        if player::count_cards_in_location(g, opp, "stormLeft", &json!("character"))? > 0 || player::count_cards_in_location(g, opp, "stormRight", &json!("character"))? > 0 {
            cost = sub(&cost, 2);
        }
    }
    let min_orbs = get(g, id, "minManaOrbs")?;
    Ok(php::le(&cost, &json!(mana)) && php::le(&min_orbs, &json!(total_mana)))
}

/// `isExhaustedReservePlayBlocked($player)`
// PHP: Models/Card.php:427
pub fn is_exhausted_reserve_play_blocked(g: &mut Game, id: CardId, player: PId) -> R<bool> {
    if location(g, id)? != "reserve" || !is_tapped(g, id)? {
        return Ok(false);
    }
    let ty = get(g, id, "type")?;
    let at = get_additional_type(g, id)?;
    Ok(!player::can_play_tapped_cards(g, player, &ty, &Value::Null, &at)?)
}

/// `hasContactCostReduction()`
// PHP: Models/Card.php:439
pub fn has_contact_cost_reduction(g: &mut Game, id: CardId) -> R<bool> {
    let r = get(g, id, "dynamicCostReduction")?;
    let list = if php::is_arr(&r) { r } else { json!([r]) };
    for x in php::vals(&list) {
        if php::strval(x).contains("hasOneContact") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn storms_v() -> Value {
    json!(["stormLeft", "stormRight"])
}

/// `getPlayableLocation($player, $forcedLocation, $free)`
// PHP: Models/Card.php:451
pub fn get_playable_location(g: &mut Game, id: CardId, player: PId, forced: &Value, free: bool) -> R<Value> {
    let subtypes = get_subtypes(g, id)?;
    if php::in_array(&json!("landmark"), &subtypes) {
        return Ok(if is_exhausted_reserve_play_blocked(g, id, player)? { json!([]) } else { json!(["landmark"]) });
    }
    let ty = get(g, id, "type")?;
    if php::loose_eq(&ty, &json!("spell")) {
        return Ok(if is_exhausted_reserve_play_blocked(g, id, player)? { json!([]) } else { json!(["limbo"]) });
    }
    if location(g, id)? == "reserve" && is_tapped(g, id)? {
        let mut locations = vec![];
        for s in STORMS {
            let ok = player::can_play_tapped_cards(g, player, &ty, &json!(s), &json!([]))?;
            if (ok && forced.is_null()) || (ok && !forced.is_null() && php::loose_eq(forced, &json!(s))) {
                locations.push(json!(s));
            }
        }
        return Ok(Value::Array(locations));
    }
    let tc = json!(["token", "character"]);
    if get_int(g, id, "costReductionIfEmpty")? > 0 {
        let cost = get_cost(g, id, false, false, None)?;
        let mana = player::get_mana(g, player)? as i64;
        if php::le(&cost, &json!(mana)) {
            if !forced.is_null() {
                return Ok(json!([forced]));
            }
            return Ok(storms_v());
        }
        let mut locations = vec![];
        if player::count_cards_in_location(g, player, "stormLeft", &tc)? == 0 && ((!forced.is_null() && php::loose_eq(forced, &json!("stormLeft"))) || forced.is_null()) {
            locations.push(json!("stormLeft"));
        }
        if player::count_cards_in_location(g, player, "stormRight", &tc)? == 0 && ((!forced.is_null() && php::loose_eq(forced, &json!("stormRight"))) || forced.is_null()) {
            locations.push(json!("stormRight"));
        }
        return Ok(Value::Array(locations));
    }
    let pl = get(g, id, "playLimitation")?;
    if php::loose_eq(&pl, &json!("noOtherCardInHand")) {
        let ok = conditions::call(g, "hasXCardsInHandExceptCurrentCard", Some(id), &Value::Null, &[json!(0), json!("LTE")])?;
        if !php::truthy(&ok) {
            return Ok(json!([]));
        }
        if !forced.is_null() {
            return Ok(json!([forced]));
        }
        return Ok(storms_v());
    } else if php::loose_eq(&pl, &json!("controlFeat")) {
        let mut n = 0;
        for c in player::get_played_cards(g, player, &Value::Null)? {
            if php::in_array(&json!("feat"), &get_subtypes(g, c)?) {
                n += 1;
            }
        }
        if n == 0 {
            return Ok(json!([]));
        }
        if !forced.is_null() {
            return Ok(json!([forced]));
        }
        return Ok(storms_v());
    } else if php::loose_eq(&pl, &json!("nonStartingRegion")) {
        let mut locations = vec![];
        let h = player::get_hero_token(g, player)?;
        if g.meeple(h)?.location != "storm-0" {
            locations.push(json!("stormLeft"));
        }
        let c = player::get_companion_token(g, player)?;
        if g.meeple(c)?.location != "storm-7" {
            locations.push(json!("stormRight"));
        }
        return Ok(Value::Array(locations));
    } else if php::loose_eq(&pl, &json!("+3StartingRegion")) {
        let mut locations = vec![];
        if free {
            return Ok(storms_v());
        }
        let mana = player::get_mana(g, player)? as i64;
        let h = player::get_hero_token(g, player)?;
        let hl = g.meeple(h)?.location.clone();
        if hl == "storm-0" && php::le(&json!(php::intval(&get_cost(g, id, false, false, None)?) + 3), &json!(mana)) {
            locations.push(json!("stormLeft"));
        } else if hl != "storm-0" {
            locations.push(json!("stormLeft"));
        }
        let c = player::get_companion_token(g, player)?;
        let cl = g.meeple(c)?.location.clone();
        if cl == "storm-7" && php::le(&json!(php::intval(&get_cost(g, id, false, false, None)?) + 3), &json!(mana)) {
            locations.push(json!("stormRight"));
        } else if cl != "storm-7" {
            locations.push(json!("stormRight"));
        }
        return Ok(Value::Array(locations));
    }
    if !forced.is_null() {
        return Ok(json!([forced]));
    }
    if free || !has_contact_cost_reduction(g, id)? {
        return Ok(storms_v());
    }
    let mut locations = vec![];
    let mana = player::get_mana(g, player)? as i64;
    for s in STORMS {
        if php::le(&get_cost(g, id, false, false, Some(s))?, &json!(mana)) {
            locations.push(json!(s));
        }
    }
    Ok(if locations.is_empty() { storms_v() } else { Value::Array(locations) })
}

/// `getScoutableLocations($player, $forcedLocation)`
// PHP: Models/Card.php:553
pub fn get_scoutable_locations(g: &mut Game, id: CardId, player: PId, forced: &Value) -> R<Value> {
    if get_int(g, id, "costReductionIfEmpty")? > 0 {
        let mana = player::get_mana(g, player)? as i64;
        if php::le(&get_cost(g, id, true, false, None)?, &json!(mana)) {
            if !forced.is_null() {
                return Ok(json!([format!("{}_scout", php::strval(forced))]));
            }
            return Ok(json!(["stormLeft_scout", "stormRight_scout"]));
        }
        let tc = json!(["token", "character"]);
        let mut locations = vec![];
        if player::count_cards_in_location(g, player, "stormLeft", &tc)? == 0 && ((!forced.is_null() && php::loose_eq(forced, &json!("stormLeft"))) || forced.is_null()) {
            locations.push(json!("stormLeft_scout"));
        }
        if player::count_cards_in_location(g, player, "stormRight", &tc)? == 0 && ((!forced.is_null() && php::loose_eq(forced, &json!("stormRight"))) || forced.is_null()) {
            locations.push(json!("stormRight_scout"));
        }
        return Ok(Value::Array(locations));
    }
    if !forced.is_null() {
        return Ok(json!([format!("{}_scout", php::strval(forced))]));
    }
    Ok(json!(["stormLeft_scout", "stormRight_scout"]))
}

/// `hasToken($token)`
pub fn has_token(g: &Game, id: CardId, token: &str) -> R<bool> {
    Ok(count_token(g, id, token)? > 0)
}

/// `countToken($token)`
pub fn count_token(g: &Game, id: CardId, token: &str) -> R<usize> {
    meeples::count_meeples(g, &format!("card-{}", id), &json!(token))
}

/// `hasCounters()`
pub fn has_counters(g: &mut Game, id: CardId) -> R<bool> {
    let t = count_token(g, id, "boost")? as i64;
    let c = php::intval(php::coalesce(php::get(&get(g, id, "extraDatas")?, "counter"), &json!(0)));
    Ok(t > 0 || c > 0)
}

/// `countCounters()`
pub fn count_counters(g: &mut Game, id: CardId) -> R<i64> {
    let t = count_token(g, id, "boost")? as i64;
    let c = php::intval(php::coalesce(php::get(&get(g, id, "extraDatas")?, "counter"), &json!(0)));
    Ok(t + c)
}

/// `getOfType($type)`
pub fn get_of_type(g: &Game, id: CardId, ty: &Value) -> R<Vec<i64>> {
    meeples::get_of_type(g, &format!("card-{}", id), ty)
}

/// `discard($seasoned, $gigantic)`
pub fn discard(g: &mut Game, id: CardId, seasoned: &Value, gigantic: &Value) -> R<Vec<i64>> {
    discard_to(g, id, "discard", seasoned, false, gigantic)
}

/// `moveToReserve($seasoned, $gigantic)`
pub fn move_to_reserve(g: &mut Game, id: CardId, seasoned: &Value, gigantic: &Value) -> R<Vec<i64>> {
    discard_to(g, id, "reserve", seasoned, false, gigantic)
}

/// `discardTo($location, $seasoned, $afterNight, $gigantic)`
// PHP: Models/Card.php:627
pub fn discard_to(g: &mut Game, id: CardId, loc: &str, seasoned: &Value, after_night: bool, gigantic: &Value) -> R<Vec<i64>> {
    let is_seasoned = is_seasoned(g, id)?;
    check_leave_listener(g, id, loc, after_night, false, gigantic)?;
    set_location(g, id, loc)?;
    let mut extra = get(g, id, "extraDatas")?;
    if php::isset(&extra, "pId") {
        let p = php::get(&extra, "pId").clone();
        set_pid(g, id, &p)?;
        php::unset(&mut extra, "pId");
        set_property(g, id, "extraDatas", extra, true)?;
    }
    set_property(g, id, "tapped", json!(false), true)?;
    let mut ms = meeples::get_in_location(g, &format!("card-{}", id), None)?;
    if loc == "reserve" && (is_seasoned || php::in_array(&json!(id), seasoned)) {
        ms.retain(|m| g.meeples[m].kind != "boost");
    }
    if !ms.is_empty() {
        meeples::delete(g, &json!(ms))?;
    }
    let extra = get(g, id, "extraDatas")?;
    if !php::get(&extra, "counterName").is_null() && ["discard", "hand", "topOfDeck"].contains(&loc) {
        set_property(g, id, "extraDatas", json!([]), true)?;
        g.notify("deleteCounter", json!({"cardId": id}));
    }
    Ok(ms)
}

/// `nightCleanup()`: asleep and anchored tokens removed
pub fn night_cleanup(g: &mut Game, id: CardId) -> R<Vec<i64>> {
    let ids = meeples::get_filtered(g, &Value::Null, &json!(format!("card-{}", id)), &json!(["asleep", "anchored"]))?;
    meeples::delete(g, &json!(ids))?;
    Ok(ids)
}

/// `getOwner()`
// PHP: Models/Card.php:666
pub fn get_owner(g: &mut Game, id: CardId) -> R<Value> {
    let p = json!(g.card(id)?.pid);
    let extra = php::coalesce(php::get(&get(g, id, "extraDatas")?, "pId"), &p).clone();
    if php::loose_ne(&extra, &p) {
        return Ok(extra);
    }
    Ok(p)
}

/// `checkLeaveListener($target, $afterNight, $isSacrifice, $gigantic)`: queues the card's own
/// Leave* reaction and the Other<Leave*> reactions of the other cards
// PHP: Models/Card.php:675
pub fn check_leave_listener(g: &mut Game, id: CardId, target: &str, after_night: bool, is_sacrifice: bool, gigantic: &Value) -> R<()> {
    let loc = location(g, id)?;
    let ty = if STORMS.contains(&loc.as_str()) {
        "LeaveExpedition"
    } else if loc == "landmark" {
        "LeaveLandmark"
    } else {
        "LeaveOther"
    };
    let source_ascended = if STORMS.contains(&loc.as_str()) {
        let p = player_req(g, id)?;
        let other = if loc == "stormLeft" { "stormRight" } else { "stormLeft" };
        player::is_ascended(g, p, &loc)? || (php::truthy(&is_gigantic(g, id, false)?) && player::is_ascended(g, p, other)?)
    } else {
        false
    };
    let pid = g.card(id)?.pid;
    let owner = get_owner(g, id)?;
    let card_type = get(g, id, "type")?;
    let at = get_additional_type(g, id)?;
    let boost = count_token(g, id, "boost")?;
    let fleeting = has_token(g, id, "fleeting")?;
    let token = is_token(g, id)?;
    let gig = php::truthy(&is_gigantic(g, id, false)?) || php::in_array(&json!(id), gigantic);
    let mut event = json!({
        "type": ty, "method": ty, "cardId": id, "from": loc, "sourceAscended": source_ascended, "to": target,
        "pId": pid, "owner": owner, "controller": pid, "cardType": card_type, "additionalType": at,
        "boost": boost, "fleeting": fleeting, "token": token, "gigantic": gig,
    });
    let mut after_cleanup = globals::get(g, "afterNightCleanup")?;
    if is_listening_to(g, id, &event)? {
        php::set(&mut event, "cardsToListen", json!([id]));
        let nd = json!({"action": "ActivateCard", "args": {"cardId": id, "event": event}, "pId": pid});
        if after_night && !is_sacrifice {
            php::push(php::get_mut(&mut after_cleanup, &pid.to_string()), nd);
        } else if !is_sacrifice {
            engine::push_after_finishing_childs(g, &json!([nd]))?;
        }
    }
    if is_sacrifice && after_night {
        let at = get_additional_type(g, id)?;
        let event2 = json!({
            "type": "Discard", "method": "Discard", "discardCard": true, "cardsToListen": [id], "cardId": id,
            "additionalType": at, "token": token, "from": "landmark", "to": "discard", "sacrifice": is_sacrifice, "cardPId": pid,
        });
        if is_listening_to(g, id, &event2)? {
            let nd = json!({"action": "ActivateCard", "args": {"cardId": id, "event": event2}, "pId": pid});
            php::push(php::get_mut(&mut after_cleanup, &pid.to_string()), nd);
        }
    }
    php::set(&mut event, "type", json!(format!("Other{}", ty)));
    php::set(&mut event, "method", json!(format!("Other{}", ty)));
    php::set(&mut event, "pId", json!(pid));
    let reaction = cards::get_reaction(g, &event, true, true)?;
    if after_night {
        if !reaction.is_null() {
            for r in php::vals(&reaction) {
                let k = php::key_of(php::get(r, "pId"));
                php::push(php::get_mut(&mut after_cleanup, &k), r.clone());
            }
        }
    } else {
        engine::push_after_finishing_childs(g, &reaction)?;
    }
    globals::set(g, "afterNightCleanup", after_cleanup)?;
    Ok(())
}

/// `resolvePassiveEventAction($passive, $event)`: Discard and ChooseAssignment alias each other
/// for support events
// PHP: Models/Card.php:782
pub fn resolve_passive_event_action(passive: &Value, event: &Value) -> Value {
    let action = php::coalesce(php::get(event, "action"), &json!("none")).clone();
    if php::isset(passive, &php::key_of(&action)) {
        return action;
    }
    if php::get(event, "isSupport") != &json!(true) {
        return action;
    }
    if action == json!("Discard") && php::isset(passive, "ChooseAssignment") {
        return json!("ChooseAssignment");
    }
    if action == json!("ChooseAssignment") && php::isset(passive, "Discard") {
        return json!("Discard");
    }
    action
}

fn ids_contain(list: &Value, id: CardId) -> bool {
    php::in_array(&json!(id), list)
}

/// `isListeningTo($event)` (listening conditions are checked here)
// PHP: Models/Card.php:803
pub fn is_listening_to(g: &mut Game, id: CardId, event: &Value) -> R<bool> {
    let ctl = php::coalesce(php::get(event, "cardsToListen"), &php::NULL).clone();
    let passive = if !ids_contain(&ctl, id) && location(g, id)? == "reserve" && !is_tapped(g, id)? {
        let inf = get(g, id, "effectInfinity")?;
        let p = php::get(&inf, "effectPassive").clone();
        if p.is_null() {
            return Ok(false);
        }
        p
    } else {
        get(g, id, "effectPassive")?
    };
    let action = resolve_passive_event_action(&passive, event);
    let keys = Value::Array(php::keys(&passive));
    let ty = php::coalesce(php::get(event, "type"), &json!("none")).clone();
    if !php::in_array(&ty, &keys) && !php::in_array(&action, &keys) {
        return Ok(false);
    }
    if php::truthy(php::get(event, "phase")) && php::loose_ne(php::get(event, "pId"), &json!(g.card(id)?.pid)) {
        return Ok(false);
    }
    let conds = php::coalesce(php::get(php::get(&passive, &php::key_of(&action)), "listeningConditions"), &php::NULL).clone();
    if php::isset(event, "action") && !php::empty(&conds) {
        for cond in php::vals(&conds) {
            let t = php::explode(":", &php::strval(cond));
            let args: Vec<Value> = t[1..].iter().map(|s| json!(s)).collect();
            let r = conditions::call(g, &t[0], Some(id), event, &args)?;
            if r == json!(false) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn reserve_passive(g: &mut Game, id: CardId, event: &Value) -> R<Option<Value>> {
    let ctl = php::get(event, "cardsToListen").clone();
    let rtl = php::get(event, "reserveToListen").clone();
    if !ids_contain(&ctl, id) && (location(g, id)? == "reserve" || ids_contain(&rtl, id)) {
        let inf = get(g, id, "effectInfinity")?;
        return Ok(Some(php::get(&inf, "effectPassive").clone()));
    }
    Ok(None)
}

/// `isImmediateReaction($event)`
// PHP: Models/Card.php:856
pub fn is_immediate_reaction(g: &mut Game, id: CardId, event: &Value) -> R<bool> {
    let passive = match reserve_passive(g, id, event)? {
        Some(p) => p,
        None => get(g, id, "effectPassive")?,
    };
    if php::empty(&passive) {
        return Ok(false);
    }
    let key = php::coalesce(php::get(event, "action"), php::coalesce(php::get(event, "type"), &json!("none"))).clone();
    let power = php::get(&passive, &php::key_of(&key));
    if power.is_null() {
        return Ok(false);
    }
    Ok(php::get(power, "immediate") == &json!(true))
}

/// `getReactions($event)`: `[payment, output]` of the power listening to the event
// PHP: Models/Card.php:877
pub fn get_reactions(g: &mut Game, id: CardId, event: &Value) -> R<(Value, Value)> {
    let passive = match reserve_passive(g, id, event)? {
        Some(p) => {
            if p.is_null() {
                return Ok((Value::Null, Value::Null));
            }
            p
        }
        None => get(g, id, "effectPassive")?,
    };
    if php::empty(&passive) {
        return Ok((Value::Null, Value::Null));
    }
    let action = resolve_passive_event_action(&passive, event);
    let ty = php::coalesce(php::get(event, "type"), &json!("none")).clone();
    if !php::isset(&passive, &php::key_of(&ty)) && !php::isset(&passive, &php::key_of(&action)) {
        return Ok((Value::Null, Value::Null));
    }
    let key = php::coalesce(php::get(event, "action"), php::get(event, "type")).clone();
    let power = php::get(&passive, &php::key_of(&key)).clone();
    if power.is_null() {
        return Ok((Value::Null, Value::Null));
    }
    if php::isset(&power, "childs") {
        let mut outs = vec![];
        for pow in php::vals(php::get(&power, "childs")) {
            let (_, output) = check_reaction(g, id, pow, event)?;
            if !output.is_null() && !php::empty(&output) {
                outs.push(output);
            }
        }
        let unique = get_int(g, id, "rarity")? == 2;
        return Ok((Value::Null, json!({"type": "parallel", "childs": outs, "noIndependent": unique})));
    }
    check_reaction(g, id, &power, event)
}

/// `checkReaction($power, $event)`: conditions checked now; `[payment, output]`
// PHP: Models/Card.php:951
pub fn check_reaction(g: &mut Game, id: CardId, power: &Value, event: &Value) -> R<(Value, Value)> {
    let mut power = power.clone();
    let mut event = event.clone();
    let n = php::coalesce(php::get(&power, "n"), &json!("once")).clone();
    if php::isset(&power, "pId") {
        let p = php::get(&power, "pId").clone();
        php::set(php::get_mut(&mut power, "output"), "pId", p.clone());
        if php::isset(&power, "oppositeOutput") {
            let opp = php::get_mut(&mut power, "oppositeOutput");
            if let Value::String(_) = opp {
                return Err(PhpError::new("Error: Cannot access offset of type string on string"));
            }
            php::set(opp, "pId", p);
        }
    }
    let opp_usable = |power: &Value| php::isset(power, "oppositeOutput") && php::loose_ne(php::get(power, "oppositeOutput"), &json!("OPPOSITE"));
    if php::loose_eq(&n, &json!("eachExpedition")) {
        let mut output = vec![];
        for s in STORMS {
            php::set(&mut event, "expedition", json!(s));
            if !conditions::check(g, &power, Some(id), &event)? {
                if opp_usable(&power) {
                    output.push(php::get(&power, "oppositeOutput").clone());
                } else {
                    continue;
                }
            } else {
                output.push(php::get(&power, "output").clone());
            }
        }
        if output.is_empty() {
            return Ok((Value::Null, Value::Null));
        }
        php::set(&mut power, "output", json!({"type": "seq", "childs": output}));
    } else if php::loose_eq(&n, &json!("once")) && !conditions::check(g, &power, Some(id), &event)? {
        if opp_usable(&power) {
            let o = php::get(&power, "oppositeOutput").clone();
            php::set(&mut power, "output", o);
        } else {
            return Ok((Value::Null, Value::Null));
        }
    }
    php::set(php::get_mut(&mut power, "output"), "sourceId", json!(id));
    let payment = php::coalesce(php::get(&power, "payment"), &php::arr()).clone();
    Ok((payment, php::get(&power, "output").clone()))
}

/// `getCost($scout, $reserveFlipCost, $location)` (null when the card is neither in hand nor in
/// Reserve; `$location` is not used)
// PHP: Models/Card.php:1001
pub fn get_cost(g: &mut Game, id: CardId, scout: bool, reserve_flip_cost: bool, _location: Option<&str>) -> R<Value> {
    let ty = get(g, id, "type")?;
    let at = get_additional_type(g, id)?;
    if (php::loose_eq(&ty, &json!("spell")) || php::in_array(&json!("spell"), &at)) && globals::is(g, "nextSpellIsFree")? {
        return Ok(json!(0));
    }
    let p = get_player(g, id)?;
    let mut minimum = players::get_opponent_minimum_cost(g, p, &ty)?;
    minimum = minimum.max(players::get_minimum_reserve_cost(g)?);
    let pid = g.card(id)?.pid;
    let cr_all = globals::get(g, "costReduction")?;
    let cost_reduction = php::coalesce(php::get(&cr_all, &pid.to_string()), &php::arr()).clone();
    let mut type_reduction = 0i64;
    for (k, reduction) in php::entries(&cost_reduction) {
        if !php::is_arr(reduction) {
            continue;
        }
        let kv = php::key_value(&k);
        if php::loose_eq(&kv, &ty) || php::in_array(&kv, &at) || php::loose_eq(&kv, &json!("all")) {
            type_reduction += php::intval(php::get(reduction, "reduction"));
            minimum = minimum.max(php::intval(php::coalesce(php::get(reduction, "minimum"), &json!(0))));
        }
    }
    for st in php::vals(&get_subtypes(g, id)?) {
        let r = php::get(&cost_reduction, &php::key_of(st));
        if r.is_null() || !php::is_arr(r) {
            continue;
        }
        type_reduction += php::intval(php::get(r, "reduction"));
        minimum = minimum.max(php::intval(php::coalesce(php::get(r, "minimum"), &json!(0))));
    }
    let mut dynamic = 0i64;
    let drs = get(g, id, "dynamicCostReduction")?;
    let drs = if php::is_arr(&drs) { drs } else { json!([drs]) };
    for dr in php::vals(&drs) {
        let s = php::strval(dr);
        let split = php::explode(":", &s);
        if split.len() > 1 {
            if !utils::check_attribute_condition(g, "cost", dr, p, id, &Value::Null)?.is_null() {
                dynamic += php::intval(&json!(split[0]));
            }
        } else if s.is_empty() {
        } else if s == "exhaustedReserve" {
            let mut n = 0;
            for pl in players::get_all(g) {
                for c in player::get_reserve_cards(g, pl)? {
                    if is_tapped(g, c)? {
                        n += 1;
                    }
                }
            }
            dynamic += n;
        } else if s == "eachOwnerAscended" {
            let pp = player_req(g, id)?;
            dynamic += player::is_ascended(g, pp, "stormLeft")? as i64 + player::is_ascended(g, pp, "stormRight")? as i64;
        } else if s == "playedCards" {
            dynamic += globals::get_int(g, "playedCards")?;
        } else if s == "hasPlayedCards" {
            dynamic += (globals::get_int(g, "playedCards")? > 0) as i64;
        } else if s == "hasInContact" {
            let pp = player_req(g, id)?;
            dynamic += player::is_in_contact(g, pp, None)? as i64;
        } else if s == "expeditionsInContact" {
            let pp = player_req(g, id)?;
            dynamic += player::is_in_contact(g, pp, Some("stormLeft"))? as i64;
            dynamic += player::is_in_contact(g, pp, Some("stormRight"))? as i64;
        } else {
            dynamic += php::intval(dr);
        }
    }
    let increase = players::get_increase_reserve_cost(g, &ty)?;
    let subtypes = get_subtypes(g, id)?;
    let reduce = players::get_reduce_reserve_cost(g, &ty, &subtypes, pid, id)?;
    if reduce > 0 && location(g, id)? == "reserve" {
        minimum = minimum.max(1);
    }
    let pp = player_req(g, id)?;
    let rct = player::get_reduce_cost_type(g, pp, id)?;
    minimum = minimum.max(php::intval(php::coalesce(php::get(&rct, "minimum"), &json!(0))));
    dynamic += php::intval(php::get(&rct, "reduction"));
    match location(g, id)?.as_str() {
        "hand" => {
            let scout_v = get_int(g, id, "scout")?;
            let initial = if scout && scout_v > 0 { scout_v } else { get_int(g, id, "costHand")? };
            Ok(json!(minimum.max(initial - type_reduction - dynamic)))
        }
        "reserve" => {
            let cr = get_int(g, id, "costReserve")?;
            if reserve_flip_cost {
                let ch = get_int(g, id, "costHand")?;
                return Ok(json!(minimum.max(cr - type_reduction - dynamic + increase - reduce).min(minimum.max(ch - type_reduction - dynamic + increase - reduce))));
            }
            Ok(json!(minimum.max(cr - type_reduction - dynamic + increase - reduce)))
        }
        _ => Ok(Value::Null),
    }
}

/// `getBiomes($includeModifiers, $increaseBiomesToHighest)`
// PHP: Models/Card.php:1102
pub fn get_biomes(g: &mut Game, id: CardId, include_modifiers: bool, increase_to_highest: bool) -> R<Value> {
    let mut b = [("ocean", get_int(g, id, "ocean")?), ("mountain", get_int(g, id, "mountain")?), ("forest", get_int(g, id, "forest")?)];
    let mut dyn_self = false;
    if include_modifiers {
        let boost = count_token(g, id, "boost")? as i64;
        for x in b.iter_mut() {
            x.1 += boost;
        }
        dyn_self = has_biomes_equal_to_highest_self(g, id)?;
    }
    if increase_to_highest || dyn_self {
        let max = b.iter().map(|x| x.1).max().unwrap_or(0).max(0);
        for x in b.iter_mut() {
            x.1 = max;
        }
    }
    Ok(json!({"ocean": b[0].1, "mountain": b[1].1, "forest": b[2].1}))
}

/// `hasBiomesEqualToHighestSelf()`
// PHP: Models/Card.php:1135
pub fn has_biomes_equal_to_highest_self(g: &mut Game, id: CardId) -> R<bool> {
    let d = get(g, id, "dynamicIncreaseBiomeHighestSelf")?;
    let list = if php::is_arr(&d) {
        d
    } else if d == json!("") {
        json!([])
    } else {
        json!([d])
    };
    if php::count(&list) == 0 {
        return Ok(false);
    }
    let printed = g.card(id)?.props.contains_key("dynamicIncreaseBiomeHighestSelf") && !g.card(id)?.props["dynamicIncreaseBiomeHighestSelf"].is_null();
    if printed && !STORMS.contains(&location(g, id)?.as_str()) {
        return Ok(false);
    }
    let p = get_player(g, id)?;
    for s in php::vals(&list) {
        if php::intval(&utils::check_attribute_condition(g, "cost", s, p, id, &Value::Null)?) == 1 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `getDynamicTough()` (Models/Card.php:1185) — not translated yet
pub fn get_dynamic_tough(_g: &mut Game, _id: CardId) -> R<Value> {
    Err(PhpError::nt("Models/Card.php:1185 getDynamicTough"))
}

/// `getTough()` (Models/Card.php:1214) — not translated yet
pub fn get_tough(_g: &mut Game, _id: CardId) -> R<i64> {
    Err(PhpError::nt("Models/Card.php:1214 getTough"))
}

/// `isSeasoned()`
// PHP: Models/Card.php:1330
pub fn is_seasoned(g: &mut Game, id: CardId) -> R<bool> {
    if php::loose_eq(g.card(id)?.props.get("seasoned").unwrap_or(&json!(false)), &json!(true)) {
        return Ok(true);
    }
    let ty = get(g, id, "type")?;
    if php::in_list(&ty, &["token", "character"]) {
        let p = player_req(g, id)?;
        let loc = location(g, id)?;
        if player::has_expedition_seasoned(g, p, Some(&loc))? {
            return Ok(true);
        } else if php::truthy(&is_gigantic(g, id, false)?) && player::has_expedition_seasoned(g, p, None)? {
        } else if is_seasoned_from_opposite_source_character(g, id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `isSeasonedFromOppositeSourceCharacter()`
// PHP: Models/Card.php:1351
pub fn is_seasoned_from_opposite_source_character(g: &mut Game, id: CardId) -> R<bool> {
    if !STORMS.contains(&location(g, id)?.as_str()) {
        return Ok(false);
    }
    let me = player_req(g, id)?;
    let opp = players::get_next_id(g, me)?;
    for src in player::get_played_cards(g, opp, &json!(["character", "token"]))? {
        if !STORMS.contains(&location(g, src)?.as_str()) {
            continue;
        }
        let ds = get(g, src, "dynamicSeasoned")?;
        let list = if !php::is_arr(&ds) && php::loose_ne(&ds, &json!("")) {
            json!([ds])
        } else if php::loose_eq(&ds, &json!("")) {
            continue;
        } else {
            ds
        };
        for s in php::vals(&list) {
            let sp = get_player(g, src)?;
            if utils::check_attribute_condition(g, "dynamicSeasoned", s, sp, src, &Value::Null)? != json!("oppositeSourceCharacter") {
                continue;
            }
            if php::truthy(&conditions::call(g, "isFacingSource", Some(id), &json!({"cardId": src}), &[])?) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `isGigantic($ignoreOneGigantic)`: true, false, or the first part of a conditional
/// dynamicGigantic value
// PHP: Models/Card.php:1382
pub fn is_gigantic(g: &mut Game, id: CardId, ignore_one: bool) -> R<Value> {
    if php::loose_eq(g.card(id)?.props.get("gigantic").unwrap_or(&json!(false)), &json!(true)) {
        return Ok(json!(true));
    }
    if is_token(g, id)? {
        let p = player_req(g, id)?;
        if player::count_universal_token_gigantic(g, p)? > 0 {
            return Ok(json!(true));
        }
    }
    let ty = get(g, id, "type")?;
    if php::in_list(&ty, &["token", "character"]) {
        let dg = get(g, id, "dynamicGigantic")?;
        let list = if !php::is_arr(&dg) && php::loose_ne(&dg, &json!("")) {
            json!([dg])
        } else if php::loose_eq(&dg, &json!("")) {
            json!([])
        } else {
            dg
        };
        for s in php::vals(&list) {
            let split = php::explode(":", &php::strval(s));
            if split.len() > 1 {
                let p = get_player(g, id)?;
                if split[0] != "universalGiganticToken" && !utils::check_attribute_condition(g, "gigantic", s, p, id, &Value::Null)?.is_null() {
                    return Ok(json!(split[0]));
                }
            } else if php::loose_eq(s, &json!("1")) {
                return Ok(json!(true));
            }
        }
        let p = player_req(g, id)?;
        let loc = location(g, id)?;
        let count = player::count_cards_in_location(g, p, &loc, &json!(["token", "character"]))?;
        if count > 1 || ignore_one {
            return Ok(json!(false));
        }
        let mut one = false;
        for c in player::get_played_cards(g, p, &json!(["permanent"]))? {
            if is(g, c, "giganticOneCharacter")? && location(g, c)? == loc {
                one = true;
            }
        }
        if one {
            let other = if loc == "stormLeft" { "stormRight" } else { "stormLeft" };
            let my_state = g.card(id)?.state;
            for c in player::get_played_cards(g, p, &json!(["token", "character"]))? {
                if location(g, c)? != other {
                    continue;
                }
                if php::truthy(&is_gigantic(g, c, true)?) {
                    return Ok(json!(false));
                }
                if g.card(c)?.state > my_state {
                    continue;
                }
                if php::truthy(&is_gigantic(g, c, false)?) {
                    return Ok(json!(false));
                }
            }
            return Ok(json!(true));
        }
    }
    Ok(json!(false))
}

/// `getReserveSlots()`
// PHP: Models/Card.php:1451
pub fn get_reserve_slots(g: &mut Game, id: CardId) -> R<Value> {
    let rs = g.card(id)?.props.get("reserveSlots").cloned().unwrap_or(json!(false));
    if rs != json!(false) && !rs.is_null() {
        return Ok(rs);
    }
    let ds = get(g, id, "dynamicReserveSlots")?;
    if php::loose_ne(&ds, &json!("")) {
        let p = get_player(g, id)?;
        let r = utils::check_attribute_condition(g, "reserveSlots", &ds, p, id, &Value::Null)?;
        if r.is_null() {
            return Ok(json!(0));
        }
        return Ok(json!(php::intval(&r)));
    }
    Ok(json!(0))
}

fn dynamic_flag(g: &mut Game, id: CardId, base: &str, dynamic: &str, attr: &str) -> R<bool> {
    if php::loose_eq(g.card(id)?.props.get(base).unwrap_or(&json!(false)), &json!(true)) {
        return Ok(true);
    }
    let d = get(g, id, dynamic)?;
    if php::loose_ne(&d, &json!("")) {
        let p = get_player(g, id)?;
        return Ok(!utils::check_attribute_condition(g, attr, &d, p, id, &Value::Null)?.is_null());
    }
    Ok(false)
}

/// `isOppositeDefender()`
pub fn is_opposite_defender(g: &mut Game, id: CardId) -> R<bool> {
    dynamic_flag(g, id, "oppositeDefender", "dynamicOppositeDefender", "oppositeDefender")
}

/// `getIncreaseReserveCost($type)`
// PHP: Models/Card.php:1481
pub fn get_increase_reserve_cost(g: &mut Game, id: CardId, ty: &Value) -> R<Value> {
    let v = g.card(id)?.props.get("increaseReserveCost").cloned().unwrap_or(json!(0));
    if php::gt(&v, &json!(0)) {
        return Ok(v);
    }
    let d = get(g, id, "dynamicIncreaseReserveCost")?;
    if php::loose_ne(&d, &json!("")) {
        let p = get_player(g, id)?;
        let r = utils::check_attribute_condition(g, "oppositeDefender", &d, p, id, &Value::Null)?;
        if php::loose_eq(&r, &json!("character")) && php::loose_eq(ty, &json!("character")) {
            return Ok(json!(1));
        } else if php::loose_eq(&r, &json!("character")) {
            return Ok(json!(0));
        } else if !r.is_null() {
            return Ok(r);
        }
    }
    Ok(json!(0))
}

/// `getReduceReserveCost($type, $subtypes, $ownerId, $cardId)`
// PHP: Models/Card.php:1500
pub fn get_reduce_reserve_cost(g: &mut Game, id: CardId, ty: &Value, subtypes: &Value, owner_id: PId, card_id: CardId) -> R<Value> {
    let v = g.card(id)?.props.get("reduceReserveCost").cloned().unwrap_or(json!(0));
    if php::gt(&v, &json!(0)) {
        return Ok(v);
    }
    if card_id == id {
        return Ok(json!(0));
    }
    let d = get(g, id, "dynamicReduceReserveCost")?;
    if php::loose_ne(&d, &json!("")) {
        let p = get_player(g, id)?;
        let r = utils::check_attribute_condition(g, "reduceReserveCost", &d, p, id, &Value::Null)?;
        let me = g.card(id)?.pid;
        if php::loose_eq(&r, &json!("myCharacter")) {
            return Ok(json!(if php::loose_eq(ty, &json!("character")) && owner_id == me { 1 } else { 0 }));
        } else if php::loose_eq(&r, &json!("myArtist")) && owner_id == me {
            return Ok(json!(if php::in_array(&json!("artist"), subtypes) { 1 } else { 0 }));
        } else if php::loose_eq(&r, &json!("myRobot")) && owner_id == me {
            return Ok(json!(if php::in_array(&json!("robot"), subtypes) { 1 } else { 0 }));
        } else if php::loose_eq(&r, &json!("1")) {
            return Ok(json!(1));
        } else if !r.is_null() && r.is_i64() {
            return Ok(r);
        }
    }
    Ok(json!(0))
}

/// `getMinimumReserveCost()`
// PHP: Models/Card.php:1539
pub fn get_minimum_reserve_cost(g: &mut Game, id: CardId) -> R<Value> {
    let v = g.card(id)?.props.get("minimumReserveCost").cloned().unwrap_or(json!(0));
    if php::gt(&v, &json!(0)) {
        return Ok(v);
    }
    let d = get(g, id, "dynamicMinimumReserveCost")?;
    if php::loose_ne(&d, &json!("")) {
        let p = get_player(g, id)?;
        let r = utils::check_attribute_condition(g, "minimumReserveCost", &d, p, id, &Value::Null)?;
        if php::loose_eq(&r, &json!("1")) {
            return Ok(json!(1));
        } else if !r.is_null() && r.is_i64() {
            return Ok(r);
        }
    }
    Ok(json!(0))
}

/// `isIgnoreDefender()`
pub fn is_ignore_defender(g: &mut Game, id: CardId) -> R<bool> {
    dynamic_flag(g, id, "ignoreDefender", "dynamicIgnoreDefender", "ignoreDefender")
}

/// `isBlockingPower()` (strict `=== true` on the property)
// PHP: Models/Card.php:1574
pub fn is_blocking_power(g: &mut Game, id: CardId) -> R<bool> {
    if g.card(id)?.props.get("blockingPower") == Some(&json!(true)) {
        return Ok(true);
    }
    let d = get(g, id, "dynamicBlockingPower")?;
    if php::loose_ne(&d, &json!("")) {
        let p = get_player(g, id)?;
        return Ok(!utils::check_attribute_condition(g, "eternal", &d, p, id, &Value::Null)?.is_null());
    }
    Ok(false)
}

/// `isEternal()`
pub fn is_eternal(g: &mut Game, id: CardId) -> R<bool> {
    dynamic_flag(g, id, "eternal", "dynamicEternal", "eternal")
}

/// `isDefender()` (Models/Card.php:1602) — not translated yet
pub fn is_defender(_g: &mut Game, _id: CardId) -> R<bool> {
    Err(PhpError::nt("Models/Card.php:1602 isDefender"))
}

/// `isInAscended()`
// PHP: Models/Card.php:1684
pub fn is_in_ascended(g: &mut Game, id: CardId) -> R<bool> {
    let loc = location(g, id)?;
    if !STORMS.contains(&loc.as_str()) {
        return Ok(false);
    }
    let other = if loc == "stormLeft" { "stormRight" } else { "stormLeft" };
    let p = player_req(g, id)?;
    if php::truthy(&is_gigantic(g, id, false)?) {
        Ok(player::is_ascended(g, p, &loc)? || player::is_ascended(g, p, other)?)
    } else {
        player::is_ascended(g, p, &loc)
    }
}
