//! Actions/InvokeToken.php: creates token card(s) in an expedition / the landmarks, applies the
//! next-token modifiers, fires the listeners.
//!
//! `getInvokePlayerId()` (Actions/InvokeToken.php:44) has no caller and is not translated.

use crate::core::{globals, node};
use crate::framework::*;
use crate::game::{CardId, ClassData, Game, PId};
use crate::helpers::ft;
use crate::managers::{cards, players};
use crate::models::action::{self, get_arg, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct InvokeToken;

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

/// `getToken()`: `new \ALT\Cards\<faction>\<tokenType>(null)`, i.e. the class properties
// PHP: Actions/InvokeToken.php:106
fn get_token(g: &Game, ctx: Ctx) -> R<ClassData> {
    let token_type = php::strval(&get_ctx_arg(g, ctx, "tokenType"));
    let faction = php::explode("_", &token_type).into_iter().next().unwrap_or_default();
    cards::classes()
        .get(&token_type)
        .cloned()
        .ok_or_else(|| PhpError::new(format!("Error: Class \"\\ALT\\Cards\\{}\\{}\" not found", faction, token_type)))
}

/// `getToken()->getName()`
fn token_name(t: &ClassData) -> Value {
    t.get("name").cloned().filter(|v| !v.is_null()).unwrap_or(json!(""))
}

/// `getN()`: an int, `landmarks3` (landmarks, at most 3) or `paidMana` (the first resolution arg
/// of the parent's first child); null otherwise
// PHP: Actions/InvokeToken.php:94
fn get_n(a: &InvokeToken, g: &mut Game, ctx: Ctx) -> R<Value> {
    let n = get_arg(a, g, ctx, "n")?;
    if n.is_i64() || n.is_u64() {
        Ok(n)
    } else if php::loose_eq(&n, &json!("landmarks3")) {
        let p = a.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLandmarks() on null"))?;
        Ok(json!(player::get_landmarks(g, p)?.len().min(3)))
    } else if php::loose_eq(&n, &json!("paidMana")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getParent() on null"))?;
        let p = node::parent(g, n).ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))?;
        let arr = node::to_array(g, p);
        Ok(php::geti(php::get(php::geti(php::get(&arr, "childs"), 0), "actionResolutionArgs"), 0).clone())
    } else {
        Ok(Value::Null)
    }
}

/// `argsInvokeToken()`
// PHP: Actions/InvokeToken.php:72
fn args_invoke_token(a: &InvokeToken, g: &mut Game, ctx: Ctx) -> R<Value> {
    let _player = players::get_active(g);

    let token_type = get_ctx_arg(g, ctx, "tokenType");
    let mut target_locations = get_ctx_arg(g, ctx, "targetLocation");
    if target_locations.is_null() {
        target_locations = php::str_list(&STORMS);
    }
    let ctx_args = get_ctx_args(g, ctx);
    if php::isset(&ctx_args, "expedition") {
        // we come from a target Expedition
        target_locations = json!([php::get(&ctx_args, "expedition")]);
    }

    let name = token_name(&get_token(g, ctx)?);
    let n = get_n(a, g, ctx)?;
    let can_pass = php::coalesce(&get_ctx_arg(g, ctx, "optional"), &json!(false)).clone();
    let all_players = (get_ctx_arg(g, ctx, "targetPlayer").is_null() && php::count_strict(&target_locations)? > 1)
        || php::truthy(php::coalesce(&get_ctx_arg(g, ctx, "allPlayers"), &json!(false)));
    Ok(json!({
        "token": token_type,
        "tokenType": name,
        "n": n,
        "canPass": can_pass,
        "locations": target_locations,
        "allPlayers": all_players,
        "targetPlayer": null,
    }))
}

/// `getLocationInfos($location)`: `[realLocation, displayed location]`
// PHP: Actions/InvokeToken.php:130
fn get_location_infos(g: &mut Game, ctx: Ctx, location: &Value) -> R<(Value, Value)> {
    let mut real = location.clone();
    let mut text = location.clone();
    if php::in_list(location, &["source", "oppositeSource", "initialSource"]) {
        let source = action::get_source(g, ctx)?;

        // No actual source => is that even possible ??
        match source {
            None => {
                text = json!(if php::loose_ne(location, &json!("oppositeSource")) { "Source" } else { "Opposite source" });
            }
            // Get the source location
            Some(_) => {
                let s = action::get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
                let mut src_loc = json!(card::location(g, s)?);

                // Check event in case of leaving expedition
                let event = action::get_event_recursive(g, ctx)?;
                if !event.is_null() && php::in_list(php::get(&event, "method"), &["LeaveExpedition", "LeaveLandmark"]) && php::isset(&event, "from") {
                    src_loc = php::get(&event, "from").clone();
                }

                // Switch if needed
                if php::loose_eq(location, &json!("oppositeSource")) {
                    real = json!(if php::loose_eq(&src_loc, &json!("stormLeft")) { "stormRight" } else { "stormLeft" });
                } else {
                    real = src_loc;
                }
                text = json!(if php::loose_eq(&real, &json!("stormLeft")) { "Hero's expedition" } else { "Companion's expedition" });
            }
        }
    }

    if php::loose_eq(location, &json!("discardedSource")) {
        return Ok((json!("stormLeft"), json!("discarded card source")));
    }
    if php::loose_eq(location, &json!("stormLeft")) {
        return Ok((json!("stormLeft"), json!("Hero expedition")));
    }
    if php::loose_eq(location, &json!("stormRight")) {
        return Ok((json!("stormRight"), json!("Companion expedition")));
    }

    Ok((real, text))
}

/// `getDisplayLocation()`
// PHP: Actions/InvokeToken.php:176
fn get_display_location(g: &mut Game, ctx: Ctx) -> R<Vec<Value>> {
    let mut locations = get_ctx_arg(g, ctx, "targetLocation");
    if locations.is_null() {
        locations = php::str_list(&STORMS);
    }
    let mut out = vec![];
    for loc in php::vals(&locations) {
        let (_real, text) = get_location_infos(g, ctx, loc)?;
        out.push(text);
    }
    Ok(out)
}

/// `actInvokeToken($location, $auto)`
// PHP: Actions/InvokeToken.php:214
fn act_invoke_token(a: &InvokeToken, g: &mut Game, ctx: Ctx, location: &Value) -> R<()> {
    let player = a.get_player(g, ctx)?;
    let args = args_invoke_token(a, g, ctx)?;

    let exploded = php::explode("-", &php::strval(location));
    let ctx_args = get_ctx_args(g, ctx);
    let mut invoke_pid = if php::isset(&ctx_args, "expedition") {
        php::get(&ctx_args, "player").clone()
    } else if exploded.len() == 1 {
        json!(player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?)
    } else {
        json!(exploded[1])
    };

    if !get_ctx_arg(g, ctx, "targetPlayer").is_null() {
        let target_player = get_ctx_arg(g, ctx, "targetPlayer");
        if php::loose_eq(&target_player, &json!("opponent")) {
            let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            invoke_pid = json!(players::get_next_id(g, p)?);
        } else if php::loose_eq(&target_player, &json!("owner")) {
            invoke_pid = php::coalesce(php::get(&get_ctx_args(g, ctx), "ownerId"), &json!(-1)).clone();
            if php::loose_eq(&invoke_pid, &json!(-1)) {
                let effect_id = get_ctx_arg(g, ctx, "cardId");
                let c = cards::get(g, &effect_id)?;
                invoke_pid = json!(card::pid(g, c)?);
            }
        }
    }

    let first = json!(exploded[0]);
    if !php::in_array(&first, php::get(&args, "locations")) && exploded.len() == 1 {
        return Err(PhpError::new("You cannot invoke in this location. Should not happen"));
    }

    let (real, _text) = get_location_infos(g, ctx, &first)?;
    let mut location = real;

    let forced = get_arg(a, g, ctx, "forcedLocation")?;
    if !forced.is_null() {
        location = get_arg(a, g, ctx, "forcedLocation")?;
    }

    let mut last: Option<CardId> = None;
    let mut i: i64 = 0;
    while php::lt(&json!(i), &get_n(a, g, ctx)?) {
        let mut props = Value::Object((*get_token(g, ctx)?).clone());

        // Use the art variant chosen by the invoking player on the deck builder
        let token_type = get_ctx_arg(g, ctx, "tokenType");
        let token_styles = globals::get(g, "tokenStyles")?;
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let style = php::get(php::get(&token_styles, &p.to_string()), &php::key_of(&token_type)).clone();
        if !style.is_null() {
            let asset = php::coalesce(php::get(&style, "asset"), php::get(&props, "asset")).clone();
            php::set(&mut props, "asset", asset);
            if php::isset(&style, "mainAsset") {
                php::set(&mut props, "mainAsset", php::get(&style, "mainAsset").clone());
            }
            if php::isset(&style, "fullArt") {
                php::set(&mut props, "fullArt", php::get(&style, "fullArt").clone());
            }
        }

        let c = cards::single_create(g, &json!({"player_id": invoke_pid, "location": location, "nbr": 1, "properties": props}))?
            .ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        last = Some(c);

        let source = action::get_source(g, ctx)?;
        let loc = card::location(g, c)?;
        g.notify("invokeToken", json!({"player_id": player, "location": loc, "displayLocation": loc, "cardId": c, "card2": source}));

        if php::loose_eq(&globals::get(g, "nextTokenAnchored")?, &json!(true)) {
            action::insert_as_child(g, ctx, &ft::gain(json!(c), "anchored", json!(1), json!(99)))?;
            globals::set(g, "nextTokenAnchored", json!(false))?;
        }
        if php::loose_eq(&globals::get(g, "nextTokenAsleep")?, &json!(true)) {
            action::insert_as_child(g, ctx, &ft::gain(json!(c), "asleep", json!(1), json!(99)))?;
            globals::set(g, "nextTokenAsleep", json!(false))?;
        }

        let mut boosts = globals::get(g, "nextCharacterInExpeditionBoost")?;
        let pk = p.to_string();
        let loc_key = php::key_of(&location);
        let mut boost_expedition = location.clone();
        let opposite = if php::loose_eq(&location, &json!("stormLeft")) { json!("stormRight") } else { json!("stormLeft") };
        if !php::isset(php::get(&boosts, &pk), &loc_key) && php::truthy(&card::is_gigantic(g, c, false)?) && php::isset(php::get(&boosts, &pk), &php::key_of(&opposite)) {
            boost_expedition = opposite;
        }
        let bk = php::key_of(&boost_expedition);
        if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) && php::isset(php::get(&boosts, &pk), &bk) {
            let n = php::get(php::get(&boosts, &pk), &bk).clone();
            action::insert_as_child(g, ctx, &ft::gain(json!(c), "boost", n, json!(99)))?;
            php::unset(php::get_mut(&mut boosts, &pk), &bk);
            globals::set(g, "nextCharacterInExpeditionBoost", boosts)?;
        }

        let event = |g: &mut Game| -> R<Value> {
            Ok(json!({
                "playCard": true,
                "cardId": c,
                "cardType": card::get(g, c, "type")?,
                "additionalType": card::get_additional_type(g, c)?,
                "from": "invoke",
                "to": location,
                "locationPId": invoke_pid,
                "gigantic": card::is_gigantic(g, c, false)?,
                "token": true,
                "invoked": get_ctx_arg(g, ctx, "tokenType"),
            }))
        };
        let e = event(g)?;
        action::check_after_listeners(a, g, ctx, p, &e, true, None)?;
        if !php::truthy(&get_arg(a, g, ctx, "moreThan1")?) && i == 0 {
            let e = event(g)?;
            action::check_after_listeners(a, g, ctx, p, &e, true, Some("InvokeTokenOnce"))?;
        }
        i += 1;
    }
    if php::le(&get_n(a, g, ctx)?, &json!(0)) {
        action::resolve_action(a, g, ctx, json!([""]), Value::Null)?;
    } else {
        let c = last.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        action::resolve_action(a, g, ctx, json!([c]), Value::Null)?;
    }
    Ok(())
}

impl ActionClass for InvokeToken {
    fn name(&self) -> &'static str {
        "InvokeToken"
    }

    fn default_args(&self) -> Value {
        json!({"allPlayers": false, "moreThan1": false, "forcedLocation": null, "n": 1})
    }

    // PHP: Actions/InvokeToken.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_INVOKE_TOKEN))
    }

    // PHP: Actions/InvokeToken.php:31
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let name = token_name(&get_token(g, ctx)?);
        let locations: Vec<String> = get_display_location(g, ctx)?.iter().map(php::strval).collect();
        Ok(json!({"log": "Invoke ${tokenName} in ${location}", "args": {"tokenName": name, "location": locations.join(", "), "i18n": ["tokenName", "location"]}}))
    }

    // PHP: Actions/InvokeToken.php:114
    fn is_automatic(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        if let Some(n) = ctx {
            let auto = node::info(g, n, "automatic");
            if !auto.is_null() {
                return Ok(php::truthy(&auto));
            }
        } else {
            return Err(PhpError::new("Error: Call to a member function getInfos() on null"));
        }
        Ok(php::count_strict(php::get(&args_invoke_token(self, g, ctx)?, "locations"))? == 1)
    }

    // PHP: Actions/InvokeToken.php:122
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        if let Some(n) = ctx {
            let ind = node::info(g, n, "independent");
            if !ind.is_null() {
                return Ok(php::truthy(&ind));
            }
        } else {
            return Err(PhpError::new("Error: Call to a member function getInfos() on null"));
        }
        Ok(php::count_strict(php::get(&args_invoke_token(self, g, ctx)?, "locations"))? == 1)
    }

    // PHP: Actions/InvokeToken.php:202
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let args = get_ctx_args(g, ctx);
        let mut pid = php::get(&args, "pId").clone();
        if pid.is_null() {
            pid = json!(players::get_active_id(g));
        }
        if php::loose_eq(&pid, &json!("source")) || php::loose_eq(&pid, &json!("owner")) {
            let s = action::get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getPId() on null"))?;
            pid = json!(card::pid(g, s)?);
        } else if php::loose_eq(&pid, &json!("controller")) {
            let c = php::get(&action::get_event(g, ctx)?, "controller").clone();
            pid = if c.is_null() {
                let s = action::get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getPId() on null"))?;
                json!(card::pid(g, s)?)
            } else {
                c
            };
        }
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/InvokeToken.php:72
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_invoke_token(self, g, ctx)?))
    }

    // PHP: Actions/InvokeToken.php:188
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let args = args_invoke_token(self, g, ctx)?;

        let tl = php::coalesce(&get_ctx_arg(g, ctx, "targetLocation"), &php::arr()).clone();
        if php::loose_eq(&tl, &json!(["oppositeSource"])) && !action::get_source_id(g, ctx)?.is_null() {
            let s = cards::get(g, &action::get_source_id(g, ctx)?)?;
            if php::truthy(&card::is_gigantic(g, s, false)?) {
                // Source is gigantic so no opposite expedition
                action::resolve_action(self, g, ctx, json!([crate::core::node::PASS]), Value::Null)?;
                return Ok(None);
            }
        }
        let locations = php::get(&args, "locations").clone();
        if php::count_strict(&locations)? == 1 && php::loose_eq(&get_arg(self, g, ctx, "allPlayers")?, &json!(false)) {
            act_invoke_token(self, g, ctx, php::geti(&locations, 0))?;
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            // PHP: Actions/InvokeToken.php:214
            "actinvoketoken" => {
                let location = action::param_req(args, 0, "location", "ALT\\Actions\\InvokeToken::actInvokeToken")?;
                act_invoke_token(self, g, ctx, &location)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\InvokeToken::{}()", method))),
        }
    }
}
