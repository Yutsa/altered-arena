//! SpecialEffect cases that boost cards: mass boosts and "1 boost per X" counts.

use super::{ext, gain, gigantic, has_subtype, player_of, req_player, type_in, Case, Out, Se, STORMS};
use crate::game::{CardId, Game, PId};
use crate::helpers::{conditions, ft};
use crate::managers::players;
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub(super) fn case_of(effect: &str) -> Option<Case> {
    let f: Case = match effect {
        "boostAllSubtype" => boost_all_subtype,
        "boostAllCharacters" => boost_all_characters,
        "boostAllCharactersInExpedition" => |g, s| all_characters_in_expedition(g, s, "boost"),
        "fleetingAllCharactersInExpedition" => |g, s| all_characters_in_expedition(g, s, "fleeting"),
        "boostAllCharactersExceptSelf" => boost_all_characters_except_self,
        "boostXReserve" => boost_x_reserve,
        "boostXLandmark" => boost_x_landmark,
        "boostXFeat" => boost_x_feat,
        "boostXFleetingChar" => |g, s| boost_x_token_char(g, s, "fleeting"),
        "boostXReserveAll" => boost_x_reserve_all,
        "boost3Stat0" => |g, s| boost_stat0(g, s, json!(1)),
        "boost23Stat0" => |g, s| boost_stat0(g, s, json!(2)),
        "boostXForForest" => boost_x_for_forest,
        "boostReserve" => boost_reserve,
        "boostXBoostedChar" => |g, s| boost_x_token_char(g, s, "boost"),
        "boostXCompletedFeat" => boost_x_completed_feat,
        "drawXCompletedFeat" => draw_x_completed_feat,
        "boostXAnimalsInExpeditions" => boost_x_animals_in_expeditions,
        "boostXAnchoredChar" => |g, s| boost_x_token_char(g, s, "anchored"),
        "boostXreserveBoost" => boost_x_reserve_boost,
        "augmentXreserveBoost" => augment_x_reserve_boost,
        "boostXMana" => boost_x_mana,
        "doubleBoosts" => double_boosts,
        "counterPerOpponentCharacter" => counter_per_opponent_character,
        "boostHandCards" => boost_hand_cards,
        "boostReserveCards" => boost_reserve_cards,
        "boostTargetReserveCards" => boost_target_reserve_cards,
        "boostXOpponentExpedition" => boost_x_opponent_expedition,
        "boostReserveSubtype" => boost_reserve_subtype,
        "boostXExhaustedMax3" => boost_x_exhausted_max3,
        "boostXLandmarkMax3" => boost_x_landmark_max3,
        "boostXAscended" => boost_x_ascended,
        "boostXAnimalsMax2" => boost_x_animals_max2,
        "PlagueofIntolerance" => plague_of_intolerance,
        _ => return None,
    };
    Some(f)
}

/// `if ($n > 0) $this->insertAsChild(FT::GAIN($card, BOOST, $n))`
fn gain_self_if(g: &mut Game, s: &Se, c: CardId, n: i64) -> R<Out> {
    if n > 0 {
        s.insert(g, &gain(json!(c), "boost", json!(n)))?;
    }
    Ok(Out::Break)
}

/// `FT::ACTION(GAIN, ['cardId' => ME, 'type' => BOOST, 'n' => $count], ['sourceId' => $card->getId()])`
fn gain_me_if(g: &mut Game, s: &Se, c: CardId, count: usize) -> R<Out> {
    if count > 0 {
        s.insert(g, &ft::action("Gain", json!({"cardId": "me", "type": "boost", "n": count}), json!({"sourceId": c})))?;
    }
    Ok(Out::Break)
}

/// The played characters and tokens of a player (`in_array($c->getType(), [TOKEN, CHARACTER])`)
fn count_characters(g: &mut Game, p: PId) -> R<usize> {
    let mut n = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if type_in(g, x, &["token", "character"])? {
            n += 1;
        }
    }
    Ok(n)
}

// PHP: Actions/SpecialEffect.php:529
fn boost_all_subtype(g: &mut Game, s: &mut Se) -> R<Out> {
    if !php::isset(&s.args, "subType") {
        return Err(PhpError::new("No subtype defined for boostAllSubtype. Shoud not happen"));
    }
    let sub_type = php::get(&s.args, "subType").clone();
    let exclude_self = php::truthy(php::coalesce(php::get(&s.args, "excludeSelf"), &json!(false)));
    let n = php::coalesce(php::get(&s.args, "n"), &json!(1)).clone();
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if exclude_self && x == c {
            continue;
        }
        if php::in_array(&sub_type, &card::get_subtypes(g, x)?) {
            nodes.push(gain(json!(x), "boost", n.clone()));
        }
    }
    s.push_parallel(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:548
fn boost_all_characters(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if type_in(g, x, &["token", "character"])? {
            nodes.push(gain(json!(x), "boost", json!(1)));
        }
    }
    s.push_parallel(g, nodes)?;
    Ok(Out::Break)
}

/// `boostAllCharactersInExpedition` / `fleetingAllCharactersInExpedition`
// PHP: Actions/SpecialEffect.php:558
fn all_characters_in_expedition(g: &mut Game, s: &mut Se, token: &str) -> R<Out> {
    let pv = s.arg(g, "player")?;
    let p = req_player(players::get_by_value(g, &pv))?;
    let expedition = s.arg(g, "expedition")?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        let in_exp = php::loose_eq(&json!(card::location(g, x)?), &expedition) || gigantic(g, x)?;
        if in_exp && type_in(g, x, &["token", "character"])? {
            nodes.push(gain(json!(x), token, json!(1)));
        }
    }
    s.push_parallel(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:580
fn boost_all_characters_except_self(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if x != c && type_in(g, x, &["token", "character"])? {
            nodes.push(gain(json!(x), "boost", json!(1)));
        }
    }
    s.push_parallel(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:590
fn boost_x_reserve(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_reserve_cards(g, p)?.len() as i64;
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:599
fn boost_x_landmark(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_landmarks(g, p)?.len() as i64;
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:608
fn boost_x_feat(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_feats(g, p)?.len() as i64;
    gain_self_if(g, s, c, n)
}

/// `boostXFleetingChar` / `boostXBoostedChar` / `boostXAnchoredChar`: own characters and tokens
/// with the token
// PHP: Actions/SpecialEffect.php:617
fn boost_x_token_char(g: &mut Game, s: &mut Se, token: &str) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &json!(["character", "token"]))? {
        if card::has_token(g, x, token)? {
            n += 1;
        }
    }
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:629
fn boost_x_reserve_all(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut n = 0;
    for p in players::get_all(g) {
        n += player::get_reserve_cards(g, p)?.len() as i64;
    }
    if n > 0 {
        let c = s.card()?;
        s.insert(g, &gain(json!(c), "boost", json!(n)))?;
    }
    Ok(Out::Break)
}

/// `boost3Stat0` (1 boost) / `boost23Stat0` (2 boosts): 3 base statistics at 0 or more
// PHP: Actions/SpecialEffect.php:639
fn boost_stat0(g: &mut Game, s: &mut Se, n: Value) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nb = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if !type_in(g, x, &["character", "token"])? {
            continue;
        }
        let biomes = card::get_biomes(g, x, false, false)?;
        for v in php::vals(&biomes) {
            if php::loose_eq(v, &json!(0)) {
                nb += 1;
            }
        }
    }
    if nb >= 3 {
        s.insert(g, &gain(json!(c), "boost", n))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1141
fn boost_x_for_forest(g: &mut Game, s: &mut Se) -> R<Out> {
    let biomes = ext::filter_biomes(g, &json!(["forest"]))?;
    let mut n = 0;
    for locs in php::vals(&biomes) {
        n += php::count_strict(locs)? as i64;
    }
    if n > 0 {
        let c = s.card()?;
        s.insert(g, &gain(json!(c), "boost", json!(n)))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1171
fn boost_reserve(g: &mut Game, s: &mut Se) -> R<Out> {
    let p = req_player(players::get_active(g))?;
    let mut nodes = vec![];
    for x in player::get_reserve_cards(g, p)? {
        if php::loose_ne(&card::get(g, x, "type")?, &json!("character")) {
            continue;
        }
        nodes.push(ft::action("Gain", json!({"cardId": x, "type": "boost"}), json!({"sourceId": s.source_id(g)?})));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1196
fn boost_x_completed_feat(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_completed_feat(g, p)? as i64;
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:1202
fn draw_x_completed_feat(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_completed_feat(g, p)?;
    if n > 0 {
        s.insert(g, &ft::action("Draw", json!({"players": "me", "n": n}), json!({"sourceId": c})))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1208
fn boost_x_animals_in_expeditions(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &json!(["character", "token"]))? {
        if x == c {
            continue;
        }
        if STORMS.contains(&card::location(g, x)?.as_str()) && has_subtype(g, x, "animal")? {
            n += 1;
        }
    }
    gain_self_if(g, s, c, n)
}

/// `for ($i = 0; $i < $boost; $i++)` with `$boost` from the event
fn event_boost(g: &Game, s: &Se) -> R<Value> {
    let e = s.event(g)?;
    Ok(php::coalesce(php::get(&e, "boost"), &json!(0)).clone())
}

// PHP: Actions/SpecialEffect.php:1249
fn boost_x_reserve_boost(g: &mut Game, s: &mut Se) -> R<Out> {
    let boost = event_boost(g, s)?;
    let mut nodes = vec![];
    let mut i = 0;
    while php::lt(&json!(i), &boost) {
        nodes.push(ft::action(
            "Target",
            json!({"targetLocation": ["reserve"], "targetPlayer": "me", "excludeSelf": true, "effect": gain(json!("EFFECT"), "boost", json!(1))}),
            json!({"sourceId": s.source_id(g)?}),
        ));
        i += 1;
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

/// `FT::AUGMENT($this)` passes the action object as card id: it is saved as JSON `{}`, read back
/// as `[]` (Target then replaces it with the chosen card)
// PHP: Actions/SpecialEffect.php:1264
fn augment_x_reserve_boost(g: &mut Game, s: &mut Se) -> R<Out> {
    let boost = event_boost(g, s)?;
    let mut nodes = vec![];
    let mut i = 0;
    while php::lt(&json!(i), &boost) {
        nodes.push(ft::action(
            "Target",
            json!({
                "targetType": ["character", "spell", "permanent"],
                "targetLocation": ["stormLeft", "stormRight", "landmark", "reserve"],
                "upTo": true,
                "augmentOnly": true,
                "effect": ft::augment(php::arr()),
            }),
            php::arr(),
        ));
        i += 1;
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1280
fn boost_x_mana(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_total_mana(g, p)? as i64;
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:1513
fn double_boosts(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    let reserve = player::get_reserve_cards(g, p)?;
    let played = player::get_played_cards(g, p, &Value::Null)?;
    for x in crate::helpers::collection::merge(&reserve, &played) {
        if card::has_token(g, x, "boost")? {
            let n = card::count_token(g, x, "boost")?;
            nodes.push(gain(json!(x), "boost", json!(n)));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1631
fn counter_per_opponent_character(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let opp = req_player(players::get_next(g, p)?)?;
    let count = count_characters(g, opp)?;
    gain_me_if(g, s, c, count)
}

// PHP: Actions/SpecialEffect.php:1644
fn boost_hand_cards(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let count = player::get_hand(g, p, &Value::Null)?.len();
    gain_me_if(g, s, c, count)
}

// PHP: Actions/SpecialEffect.php:1655
fn boost_reserve_cards(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let count = player::get_reserve_cards(g, p)?.len();
    gain_me_if(g, s, c, count)
}

// PHP: Actions/SpecialEffect.php:1942
fn boost_target_reserve_cards(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let count = player::get_reserve_cards(g, p)?.len();
    if count > 0 {
        let target = s.get_card(g)?;
        s.insert(g, &ft::action("Gain", json!({"cardId": target, "type": "boost", "n": count}), json!({"sourceId": c})))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1984
fn boost_x_opponent_expedition(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let _expedition = card::location(g, c)?;
    let opp = req_player(players::get_next(g, p)?)?;
    let mut count = 0;
    for x in player::get_played_cards(g, opp, &Value::Null)? {
        if type_in(g, x, &["token", "character"])? && STORMS.contains(&card::location(g, x)?.as_str()) {
            count += 1;
        }
    }
    gain_me_if(g, s, c, count)
}

// PHP: Actions/SpecialEffect.php:2006
fn boost_reserve_subtype(g: &mut Game, s: &mut Se) -> R<Out> {
    let p = req_player(players::get_active(g))?;
    let sub_type = php::get(&s.args, "subType").clone();
    let mut nodes = vec![];
    for x in player::get_reserve_cards(g, p)? {
        if !php::in_array(&sub_type, &card::get_subtypes(g, x)?) {
            continue;
        }
        nodes.push(ft::action("Gain", json!({"cardId": x, "type": "boost"}), json!({"sourceId": s.source_id(g)?})));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2101
fn boost_x_exhausted_max3(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &json!("permanent"))? {
        if card::is_tapped(g, x)? {
            n += 1;
        }
    }
    for x in player::get_reserve_cards(g, p)? {
        if card::is_tapped(g, x)? {
            n += 1;
        }
    }
    if n > 0 {
        s.insert(g, &ft::gain(json!(c), "boost", json!(n), json!(3)))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2340
fn boost_x_landmark_max3(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let n = player::get_landmarks(g, p)?.len() as i64;
    let n = (3 - card::count_token(g, c, "boost")? as i64).min(n);
    gain_self_if(g, s, c, n)
}

// PHP: Actions/SpecialEffect.php:2437
fn boost_x_ascended(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let event = s.event(g)?;
    let n = conditions::count_source_ascended(g, Some(c), &event, &[])?;
    if php::gt(&n, &json!(0)) {
        s.insert(g, &gain(json!(c), "boost", n))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2443
fn boost_x_animals_max2(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if x != c && has_subtype(g, x, "animal")? {
            n += 1;
        }
    }
    if n > 0 {
        s.insert(g, &ft::gain(json!(c), "boost", json!(n), json!(2)))?;
    }
    Ok(Out::Break)
}

/// The thresholds run in order 2, 4, 6
// PHP: Actions/SpecialEffect.php:2455
fn plague_of_intolerance(g: &mut Game, s: &mut Se) -> R<Out> {
    let active = req_player(players::get_active(g))?;
    let mut count = 0;
    for x in player::get_played_cards(g, active, &Value::Null)? {
        if type_in(g, x, &["character"])? {
            count += 1;
        }
    }
    let c = s.card()?;
    let p = player_of(g, c)?;
    if count >= 2 {
        s.insert(g, &ft::action("Draw", json!({"players": "me"}), php::arr()))?;
    }
    if count >= 4 {
        s.insert(g, &gain(json!(c), "boost", json!(2)))?;
    }
    if count >= 6 {
        let mut nodes = vec![];
        for x in player::get_played_cards(g, p, &Value::Null)? {
            if x != c && type_in(g, x, &["token", "character"])? {
                nodes.push(gain(json!(x), "boost", json!(1)));
            }
        }
        s.push_parallel(g, nodes)?;
    }
    Ok(Out::Break)
}
