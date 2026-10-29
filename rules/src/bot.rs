//! server/src/Bot.php: a simple opponent for solo games. It answers with what the state args
//! allow, through the same game methods as a human client: at worst it plays badly. Its random
//! choices draw from the game's generator, where the last act left it (PHP's mt_rand state).
//!
//! `decide` returns the game method and its arguments, or None when it has nothing to do.

use crate::framework;
use crate::game::{Game, PId};
use crate::managers::{cards, players};
use crate::models::card;
use crate::php;
use crate::R;
use serde_json::{json, Value};

pub type Call = (String, Vec<Value>);

fn call(method: &str, args: Vec<Value>) -> Option<Call> {
    Some((method.to_string(), args))
}

fn atomic(action: &str, args: Vec<Value>) -> Option<Call> {
    call("actTakeAtomicAction", vec![json!(action), Value::Array(args)])
}

/// `$items[array_rand($items)]` on a list
fn rand_item<'a>(g: &mut Game, items: &'a [Value]) -> &'a Value {
    let keys: Vec<i64> = (0..items.len() as i64).collect();
    let i = g.php_array_rand_int_keys(&keys);
    &items[i]
}

/// `$array[array_rand($array)]` on an array decoded from JSON (a list, or a map with integer keys)
fn rand_value(g: &mut Game, a: &Value) -> Value {
    match a {
        Value::Object(m) => {
            let keys: Vec<i64> = m.keys().map(|k| php::intval(&json!(k))).collect();
            let i = g.php_array_rand_int_keys(&keys);
            m.values().nth(i).cloned().unwrap_or(Value::Null)
        }
        _ => {
            let items: Vec<Value> = php::vals(a).into_iter().cloned().collect();
            rand_item(g, &items).clone()
        }
    }
}

fn list(v: &Value) -> Vec<Value> {
    php::vals(v).into_iter().cloned().collect()
}

fn opt(args: &Value, k: &str) -> bool {
    php::truthy(php::coalesce(php::get(args, k), &json!(false)))
}

/// `Bot::decide($game, $botId)`
// PHP: server/src/Bot.php:16
pub fn decide(g: &mut Game, bot: PId) -> R<Option<Call>> {
    let (def, args) = framework::state_with_args(g)?;
    let name = def.map(|d| d.name).unwrap_or("");
    let args = php::round_trip(&args);
    let args = if args.is_null() { php::arr() } else { args };
    let private = {
        let p = php::get(php::get(&args, "_private"), &bot.to_string());
        if !p.is_null() {
            p.clone()
        } else {
            php::coalesce(php::get(&args, "_private"), &php::arr()).clone()
        }
    };
    Ok(match name {
        "selectPrecoDeck" => {
            if !php::get(&private, "selection").is_null() {
                return Ok(None);
            }
            let decks = list(php::coalesce(php::get(&private, "decks"), &php::arr()));
            if decks.is_empty() {
                return Ok(None);
            }
            let deck = rand_item(g, &decks).clone();
            call("actSelectPrecoDeck", vec![php::get(&deck, "deckNumber").clone()])
        }
        "firstDayManaSelection" => {
            if !php::empty(php::get(&private, "selection")) {
                return Ok(None);
            }
            let cs = sort_by_cost(g, list(php::coalesce(php::get(&private, "cards"), &php::arr())), true)?;
            let n = php::intval(php::coalesce(php::get(&private, "n"), &json!(3))).max(0) as usize;
            call("actFirstDayManaSelection", vec![Value::Array(cs.into_iter().take(n).collect())])
        }
        "chooseAssignment" => choose_assignment(g, &args, &private)?,
        "confirmTurn" => call("actConfirmTurn", vec![]),
        "confirmPartialTurn" => call("actConfirmPartialTurn", vec![]),
        "resolveChoice" => {
            let choices = list(php::coalesce(php::get(&args, "choices"), &php::arr()));
            if choices.is_empty() {
                return Ok(pass_or_null(&args));
            }
            let c = rand_item(g, &choices).clone();
            call("actChooseAction", vec![php::get(&c, "id").clone()])
        }
        "target" | "discardDo" => {
            let ids = list(php::coalesce(php::get(&args, "cardIds"), &php::arr()));
            let n = (php::intval(php::coalesce(php::get(&args, "n"), &json!(1))).min(ids.len() as i64)).max(0) as usize;
            // Prefer opponent cards for effects, own cheap cards otherwise: a coin flip keeps it simple
            let opponent_first = g.mt_rand(0, 1) == 1;
            let ids = prefer_owner(g, ids, bot, opponent_first);
            atomic("actTarget", vec![Value::Array(ids.into_iter().take(n).collect())])
        }
        "discard" => {
            let ids = sort_by_cost(g, list(php::coalesce(php::get(&private, "cards"), &php::arr())), false)?;
            let n = if php::is_numeric(php::get(&args, "n")) { php::intval(php::get(&args, "n")) } else { 1 };
            if opt(&args, "upTo") && opt(&args, "optionalAction") {
                return Ok(call("actPassOptionalAction", vec![]));
            }
            let k = n.min(ids.len() as i64).max(0) as usize;
            atomic("actDiscard", vec![Value::Array(ids.into_iter().take(k).collect())])
        }
        "nightCleanup" => {
            let reserve = sort_by_cost(g, list(php::coalesce(php::get(&private, "reserveCards"), &php::arr())), false)?;
            let landmarks = sort_by_cost(g, list(php::coalesce(php::get(&private, "landmarkCards"), &php::arr())), false)?;
            let nr = php::intval(php::coalesce(php::get(&args, "nReserve"), &json!(0))).max(0) as usize;
            let nl = php::intval(php::coalesce(php::get(&args, "nLandmarks"), &json!(0))).max(0) as usize;
            atomic("actNightCleanup", vec![Value::Array(reserve.into_iter().take(nr).collect()), Value::Array(landmarks.into_iter().take(nl).collect())])
        }
        "moveExpedition" => {
            let exps = list(php::coalesce(php::get(&args, "expeditions"), &php::arr()));
            if exps.is_empty() {
                return Ok(pass_or_null(&args));
            }
            let pick = exps.iter().find(|e| php::intval(php::geti(e, 0)) == bot).cloned().unwrap_or_else(|| exps[0].clone());
            atomic("actMoveExpedition", vec![pick])
        }
        "invokeToken" => {
            let locations = php::coalesce(php::get(&args, "locations"), &php::arr()).clone();
            if php::empty(&locations) {
                return Ok(pass_or_null(&args));
            }
            let mut location = php::strval(&rand_value(g, &locations));
            let owner = php::coalesce(php::get(&args, "invokePlayerId"), &json!(bot)).clone();
            if opt(&args, "allPlayers") || location == "stormLeft" || location == "stormRight" {
                location = format!("{}-{}", location, php::strval(&owner));
            }
            atomic("actInvokeToken", vec![json!(location)])
        }
        "targetPlayer" => {
            let opponent = players::get_next_id(g, bot)?;
            atomic("actTargetPlayer", vec![json!(opponent)])
        }
        "targetExpedition" => {
            let exps = list(php::coalesce(php::get(&args, "expeditions"), &php::arr()));
            let n = php::intval(php::coalesce(php::get(&args, "n"), &json!(1))).max(0) as usize;
            atomic("actTargetExpedition", vec![Value::Array(exps.into_iter().take(n).collect())])
        }
        "blockExpedition" => {
            let opponent = players::get_next_id(g, bot)?;
            atomic("actBlockExpedition", vec![json!(format!("board-stormLeft-{}", opponent))])
        }
        "rollDie" => {
            let rolls = list(php::coalesce(php::get(&args, "rolls"), &php::arr()));
            if rolls.is_empty() {
                return Ok(pass_or_null(&args));
            }
            let mut best = rolls[0].clone();
            for r in &rolls[1..] {
                if php::gt(r, &best) {
                    best = r.clone();
                }
            }
            atomic("actRollDie", vec![best])
        }
        "pay" => {
            let max = php::intval(php::coalesce(php::get(&args, "mana"), &json!(0))).min(php::intval(php::coalesce(php::get(&args, "maximum"), &json!(0))));
            atomic("actPay", vec![json!(max)])
        }
        "spend" => atomic("actSpend", vec![json!(php::intval(php::coalesce(php::get(&args, "n"), &json!(1))))]),
        "exchange" => {
            if opt(&args, "optionalAction") {
                return Ok(call("actPassOptionalAction", vec![]));
            }
            let reserve = list(php::coalesce(php::get(&args, "reserveIds"), &php::arr()));
            let hand = list(php::coalesce(php::get(&args, "handIds"), &php::arr()));
            if reserve.is_empty() || hand.is_empty() {
                return Ok(pass_or_null(&args));
            }
            atomic("actExchange", vec![reserve[0].clone(), hand[0].clone()])
        }
        "interuptReveal" => atomic("actInteruptReveal", vec![]),
        "markRegion" => {
            let markers = list(php::coalesce(php::get(&args, "markers"), &php::arr()));
            let regions = list(php::coalesce(php::get(&args, "regions"), &php::arr()));
            if markers.is_empty() || regions.is_empty() {
                return Ok(pass_or_null(&args));
            }
            let region = rand_item(g, &regions).clone();
            let region = if php::is_arr(&region) { php::coalesce(php::get(&region, "id"), &json!(0)).clone() } else { region };
            atomic("actMarkRegion", vec![php::get(&markers[0], "id").clone(), region])
        }
        "moveRegionMarker" => {
            let markers = list(php::coalesce(php::get(&args, "markers"), &php::arr()));
            if markers.is_empty() {
                return Ok(pass_or_null(&args));
            }
            atomic("actMoveRegionMarker", vec![php::get(&markers[0], "id").clone()])
        }
        "playCard" => {
            let play = php::coalesce(php::get(&private, "play"), &php::arr()).clone();
            let card_id = match php::get(&args, "cardId") {
                Value::Null => php::keys(&play).first().cloned().unwrap_or(Value::Null),
                v => v.clone(),
            };
            let locations = php::coalesce(php::get(&play, &php::key_of(&card_id)), &php::arr()).clone();
            if php::empty(&locations) {
                return Ok(pass_or_null(&args));
            }
            let loc = rand_value(g, &locations);
            atomic("actPlayCard", vec![card_id, loc])
        }
        "impossibleAction" => pass_or_null(&args),
        _ => pass_or_null(&args),
    })
}

/// `Bot::fallback($game, $botId)`: second chance after a refused decision: pass if possible,
/// else end the turn or take the first targets
// PHP: server/src/Bot.php:170
pub fn fallback(g: &mut Game, _bot: PId) -> R<Option<Call>> {
    let (def, args) = framework::state_with_args(g)?;
    let args = php::round_trip(&args);
    let possible = def.and_then(|d| d.possible).unwrap_or(&[]);
    if opt(&args, "optionalAction") && possible.contains(&"actPassOptionalAction") {
        return Ok(call("actPassOptionalAction", vec![]));
    }
    let name = def.map(|d| d.name).unwrap_or("");
    if name == "chooseAssignment" {
        return Ok(atomic("actPass", vec![]));
    }
    if name == "target" || name == "discardDo" {
        let ids = list(php::coalesce(php::get(&args, "cardIds"), &php::arr()));
        let n = (php::intval(php::coalesce(php::get(&args, "n"), &json!(1))).min(ids.len() as i64)).max(0) as usize;
        return Ok(atomic("actTarget", vec![Value::Array(ids.into_iter().take(n).collect())]));
    }
    Ok(None)
}

fn pass_or_null(args: &Value) -> Option<Call> {
    if opt(args, "optionalAction") {
        call("actPassOptionalAction", vec![])
    } else {
        None
    }
}

/// `Bot::chooseAssignment($args, $private, $botId)`: the best affordable card, else pass. A
/// card's value is its total statistics (spells and permanents count as 4) minus its cost.
// PHP: server/src/Bot.php:200
fn choose_assignment(g: &mut Game, args: &Value, private: &Value) -> R<Option<Call>> {
    let play = php::coalesce(php::get(private, "play"), &php::arr()).clone();
    let mut best: Option<(f64, i64, String)> = None;
    for (card_id, locations) in php::entries(&play) {
        let cid = php::intval(&json!(card_id));
        if !g.cards.contains_key(&cid) || php::empty(locations) {
            continue;
        }
        let from_reserve = card::location(g, cid)? == "reserve";
        let cost = if from_reserve { php::intval(&card::get(g, cid, "costReserve")?) } else { php::intval(&card::get(g, cid, "costHand")?) };
        let stats = php::intval(&card::get(g, cid, "forest")?) + php::intval(&card::get(g, cid, "mountain")?) + php::intval(&card::get(g, cid, "ocean")?);
        let value = (if stats > 0 { stats } else { 4 }) as f64 - 0.6 * cost as f64 + g.mt_rand(0, 100) as f64 / 100.0;
        for location in php::vals(locations) {
            let loc = php::strval(location);
            let mut v = value;
            if loc.contains("scout") {
                v -= 1.5;
            }
            if loc == "reserve" {
                v -= 3.0;
            }
            if best.as_ref().is_none_or(|b| v > b.0) {
                best = Some((v, cid, loc));
            }
        }
    }
    if let Some((_, cid, loc)) = best {
        return Ok(atomic("actPlay", vec![json!(cid), json!(loc)]));
    }
    if !opt(args, "additionalAction") {
        return Ok(atomic("actPass", vec![]));
    }
    Ok(call("actPassOptionalAction", vec![]))
}

/// `Bot::sortByCost($ids, $desc)` (hand cost; stable, as PHP 8's usort)
fn sort_by_cost(g: &mut Game, ids: Vec<Value>, desc: bool) -> R<Vec<Value>> {
    let mut keyed = vec![];
    for id in ids {
        let cid = php::intval(&id);
        let cost = if g.cards.contains_key(&cid) { php::intval(&card::get(g, cid, "costHand")?) } else { 0 };
        keyed.push((cost, id));
    }
    keyed.sort_by(|a, b| if desc { b.0.cmp(&a.0) } else { a.0.cmp(&b.0) });
    Ok(keyed.into_iter().map(|(_, id)| id).collect())
}

/// `Bot::preferOwner($ids, $botId, $opponentFirst)`
fn prefer_owner(g: &Game, ids: Vec<Value>, bot: PId, opponent_first: bool) -> Vec<Value> {
    let mut keyed: Vec<(i64, Value)> = ids
        .into_iter()
        .map(|id| {
            let mine = cards::get_single(g, &id, false).ok().flatten().map(|c| g.cards.get(&c).map(|r| r.pid == bot).unwrap_or(false)).unwrap_or(false);
            (mine as i64, id)
        })
        .collect();
    keyed.sort_by(|a, b| if opponent_first { a.0.cmp(&b.0) } else { b.0.cmp(&a.0) });
    keyed.into_iter().map(|(_, id)| id).collect()
}
