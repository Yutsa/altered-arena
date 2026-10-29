//! Actions/MoveExpedition.php: moves an expedition forward (or backward), outside of the Dusk
//! moves too; cards replacing the move (Rune's Testament, Eris, Pegasus) are offered first.

use crate::core::globals;
use crate::framework::*;
use crate::game::{Game, PId};
use crate::helpers::ft;
use crate::managers::{cards, players};
use crate::models::action::{check_after_listeners, get_arg, get_event, get_event_recursive, get_source, insert_as_child, param_req, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct MoveExpedition;

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

/// `getSides()`: the expeditions of the args (`EFFECT` = the source's, `fromEvent` = the event's)
// PHP: Actions/MoveExpedition.php:35
fn get_sides(a: &MoveExpedition, g: &mut Game, ctx: Ctx) -> R<Value> {
    let mut expeditions = get_arg(a, g, ctx, "expedition")?;
    if php::empty(&expeditions) {
        expeditions = php::str_list(&STORMS);
    }
    if expeditions.is_string() {
        expeditions = json!([expeditions]);
    }
    let mut out = php::arr();
    for (k, expe) in php::entries(&expeditions) {
        let mut expe = expe.clone();
        if php::loose_eq(&expe, &json!("EFFECT")) {
            let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
            expe = json!(card::location(g, s)?);
        } else if php::loose_eq(&expe, &json!("fromEvent")) {
            // Resolve expedition from the triggering event context.
            let event = get_event_recursive(g, ctx)?;
            if !event.is_null() && php::in_list(php::get(&event, "method"), &["LeaveExpedition", "LeaveLandmark"]) && php::isset(&event, "from") {
                expe = php::get(&event, "from").clone();
            } else if !event.is_null() && php::loose_eq(php::get(&event, "method"), &json!("MoveExpedition")) && php::isset(&event, "expedition") {
                expe = php::get(&event, "expedition").clone();
            }
        }
        php::set(&mut out, &k, expe);
    }
    Ok(out)
}

/// `argsMoveExpedition()`
// PHP: Actions/MoveExpedition.php:63
fn args_move_expedition(a: &MoveExpedition, g: &mut Game, ctx: Ctx) -> R<Value> {
    let sides = get_sides(a, g, ctx)?;
    let n = get_arg(a, g, ctx, "n")?;
    let force_expedition = get_arg(a, g, ctx, "forceExpedition")?;
    let expeditions = if !force_expedition.is_null() {
        force_expedition.clone()
    } else {
        let mut forced_pid = get_arg(a, g, ctx, "pId")?;
        if php::loose_eq(&forced_pid, &json!("opponent")) {
            let act = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            forced_pid = json!(players::get_next_id(g, act)?);
        } else if php::loose_eq(&forced_pid, &json!("me")) {
            forced_pid = json!(players::get_active_id(g));
        }
        let blocked = players::get_blocked_expeditions(g)?;
        let mut expeditions = vec![];
        for (pid_k, statuses) in php::entries(&blocked) {
            let pid_v = php::key_value(&pid_k);
            if !forced_pid.is_null() && php::loose_ne(&forced_pid, &pid_v) {
                continue;
            }
            for (side, is_blocked) in php::entries(statuses) {
                if php::in_array(&json!(side), &sides) && (php::lt(&n, &json!(0)) || !php::truthy(is_blocked)) {
                    expeditions.push(json!([pid_v, side]));
                }
            }
        }
        Value::Array(expeditions)
    };
    Ok(json!({
        "forceExpedition": force_expedition,
        "expeditions": expeditions,
        "descSuffix": if php::lt(&n, &json!(0)) { "backward" } else { "" },
    }))
}

/// `$expeditionMoves[$pId][$expedition] = ($expeditionMoves[$pId][$expedition] ?? 0) + $n`
fn add_expedition_moves(g: &mut Game, pid: PId, expedition: &str, n: &Value) -> R<()> {
    let mut em = globals::get(g, "expeditionMoves")?;
    let ps = pid.to_string();
    let cur = php::coalesce(php::get(php::get(&em, &ps), expedition), &json!(0)).clone();
    let v = php::intval(&cur) + php::intval(n);
    php::set(php::get_mut(&mut em, &ps), expedition, json!(v));
    globals::set(g, "expeditionMoves", em)?;
    Ok(())
}

impl ActionClass for MoveExpedition {
    fn name(&self) -> &'static str {
        "MoveExpedition"
    }

    // PHP: Actions/MoveExpedition.php:23
    fn default_args(&self) -> Value {
        json!({
            "expedition": [],
            "n": 1,
            "pId": null,
            "force": false,
            "forceExpedition": null,
            "winningBiomes": [],
            "moveOtherExpedition": false,
            "ascended": false,
            "hasMovedFromAscension": false,
        })
    }

    // PHP: Actions/MoveExpedition.php:13
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_MOVE_EXPEDITION))
    }

    // PHP: Actions/MoveExpedition.php:18
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Move expedition"))
    }

    // PHP: Actions/MoveExpedition.php:98
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let args = args_move_expedition(self, g, ctx)?;
        Ok(!php::empty(php::get(&args, "expeditions")))
    }

    // PHP: Actions/MoveExpedition.php:104
    fn is_automatic(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let args = args_move_expedition(self, g, ctx)?;
        Ok(php::count(php::get(&args, "expeditions")) == 1)
    }

    // PHP: Actions/MoveExpedition.php:63
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_move_expedition(self, g, ctx)?))
    }

    // PHP: Actions/MoveExpedition.php:110
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        if globals::is(g, "tieBreakerMode")? {
            g.notify("message", json!({"log": "In tie-breaker this power has no effect."}));
            resolve_action(self, g, ctx, Value::Null, json!(false))?;
            return Ok(None);
        }
        let fe = get_arg(self, g, ctx, "forceExpedition")?;
        if !fe.is_null() {
            // spread as the arguments of actMoveExpedition: `$expe` is the player id
            return Ok(Some(fe));
        }
        let args = args_move_expedition(self, g, ctx)?;
        if php::count(php::get(&args, "expeditions")) == 1 {
            return Ok(Some(json!([php::geti(php::get(&args, "expeditions"), 0)])));
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actmoveexpedition" => {
                let expe = param_req(args, 0, "expe", "ALT\\Actions\\MoveExpedition::actMoveExpedition")?;
                act_move_expedition(self, g, ctx, expe)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\MoveExpedition::{}()", method))),
        }
    }
}

/// `actMoveExpedition($expe)`
// PHP: Actions/MoveExpedition.php:128
fn act_move_expedition(a: &MoveExpedition, g: &mut Game, ctx: Ctx, expe: Value) -> R<()> {
    let args = args_move_expedition(a, g, ctx)?;
    let mut gigantic = false;
    if !php::in_array(&expe, php::get(&args, "expeditions")) {
        return Err(PhpError::new("Invalid expedition all. Should not happen"));
    }
    if let Some(s) = get_source(g, ctx)? {
        if php::truthy(&card::is_gigantic(g, s, false)?) {
            gigantic = true;
        }
    }
    // Combo Diocles & eat me energy bars
    let event = get_event(g, ctx)?;
    if !event.is_null() && php::loose_eq(php::get(&event, "method"), &json!("LeaveExpedition")) && php::loose_eq(php::get(&event, "gigantic"), &json!(true)) {
        gigantic = true;
    }
    let mut expe = expe;
    let fe = get_arg(a, g, ctx, "forceExpedition")?;
    if !fe.is_null() {
        expe = fe;
    }

    let pid_v = php::geti(&expe, 0).clone();
    let expedition = php::strval(php::geti(&expe, 1));
    let expedition = expedition.as_str();
    let token = if expedition == "stormLeft" { "hero" } else { "companion" };
    let source = get_source(g, ctx)?;
    let n = get_arg(a, g, ctx, "n")?;
    let ni = php::intval(&n);
    let p = players::get_by_value(g, &pid_v).ok_or_else(|| PhpError::new("Error: Call to a member function isAscended() on null"))?;
    let winning_biomes = get_arg(a, g, ctx, "winningBiomes")?;
    let ascended = player::is_ascended(g, p, expedition)?;
    let has_moved_from_ascension = get_arg(a, g, ctx, "hasMovedFromAscension")? == json!(true);

    // Rune's testament / Pegasus / Eris (non-dusk forced moves from spells and effects)
    if get_arg(a, g, ctx, "force")? == json!(false) {
        let action_instead_advance = players::get_action_instead_of_advance(g)?;
        let aia = php::coalesce(php::get(php::get(&action_instead_advance, &php::key_of(&pid_v)), expedition), &php::arr()).clone();
        if php::gt(&n, &json!(0)) && !php::empty(&aia) {
            let mut nodes: Vec<Value> = vec![];
            let forced_move_args = json!({
                "pId": pid_v,
                "expedition": [expedition],
                "force": true,
                "forceExpedition": [pid_v, expedition],
                "winningBiomes": winning_biomes,
                "ascended": ascended,
                "hasMovedFromAscension": has_moved_from_ascension,
            });
            let move_n = |n: Value, node: Value| ft::action("MoveExpedition", php::array_merge(&[&forced_move_args, &json!({"n": n})]), node);
            let plus = |k: i64| json!(php::intval(&n) + k);

            // Pegasus is cumulative: all copies apply to a single move instead of being alternatives
            let mut pegasus_cards = vec![];
            for (cid, action) in php::entries(&aia) {
                if php::loose_eq(action, &json!("PegasusCommon")) {
                    pegasus_cards.push(php::key_value(&cid));
                }
            }

            if !php::in_array(&json!("ErisCommon"), &aia) && !php::in_array(&json!("ErisRare"), &aia) && !php::in_array(&json!("PegasusCommon"), &aia) {
                nodes.push(move_n(n.clone(), json!({"pId": pid_v})));
            }
            for (cid, action) in php::entries(&aia) {
                let cid_v = php::key_value(&cid);
                let triggered = cards::get(g, &cid_v)?;
                if php::loose_eq(action, &json!("draw2")) {
                    nodes.push(ft::seq(vec![
                        ft::action("Discard", json!({"cardId": cid_v}), json!({"sourceId": cid_v})),
                        ft::action("Draw", json!({"players": "me", "n": 2}), json!({"pId": pid_v, "sourceId": cid_v})),
                    ]));
                } else if php::loose_eq(action, &json!("look4")) {
                    nodes.push(ft::seq(vec![
                        ft::action("Discard", json!({"cardId": cid_v}), json!({"sourceId": cid_v})),
                        ft::action("SpecialEffect", json!({"effect": "RunesTestamentLook4"}), json!({"pId": pid_v, "sourceId": cid_v})),
                    ]));
                } else if php::loose_eq(action, &json!("ErisCommon")) || php::loose_eq(action, &json!("ErisRare")) {
                    let low = if php::loose_eq(action, &json!("ErisCommon")) { "1-3" } else { "2-3" };
                    if !php::truthy(&card::is_gigantic(g, triggered, false)?) {
                        let mut effect = php::arr();
                        php::set(&mut effect, low, move_n(n.clone(), json!({"pId": pid_v})));
                        php::set(&mut effect, "4+", move_n(plus(1), json!({"pId": pid_v})));
                        nodes.push(ft::action("RollDie", json!({"effect": effect}), json!({"sourceId": cid_v, "pId": pid_v})));
                    } else {
                        nodes.push(move_n(n.clone(), json!({"pId": pid_v})));
                    }
                }
            }
            if !pegasus_cards.is_empty() {
                let pegasus_n = if php::empty(&winning_biomes) && ascended { plus(pegasus_cards.len() as i64) } else { n.clone() };
                nodes.push(move_n(pegasus_n, json!({"pId": pid_v, "sourceId": pegasus_cards[0]})));
            }
            if !nodes.is_empty() {
                let flow = if nodes.len() == 1 { nodes.remove(0) } else { json!({"type": "xor", "childs": nodes, "pId": pid_v}) };
                insert_as_child(g, ctx, &flow)?;
            }
            resolve_action(a, g, ctx, json!([""]), json!(false))?;
            return Ok(());
        }
    }

    let winning_biomes = get_arg(a, g, ctx, "winningBiomes")?;
    let moved = if (ni > 0 && !players::has_opponent_block_move_expedition(g, p, expedition)?) || ni < 0 {
        player::advance_storm(g, p, token, &winning_biomes, ni, has_moved_from_ascension)?
    } else {
        false
    };
    if moved {
        add_expedition_moves(g, p, expedition, &n)?;
        check_after_listeners(a, g, ctx, p, &json!({"moveExpedition": n, "ascended": ascended, "expedition": expedition}), true, None)?;
        if get_arg(a, g, ctx, "moveOtherExpedition")? == json!(true) {
            // Cable Car / Western Wind: move the other expedition the opposite way.
            let other_expedition = if expedition == "stormLeft" { "stormRight" } else { "stormLeft" };
            let other_n = -ni;
            let global_blocked = globals::get(g, "blockedExpeditions")?;
            let ps = php::key_of(&pid_v);
            let other_blocked = players::has_opposite_defender(g, other_expedition)?
                || (php::isset(&global_blocked, &ps) && php::in_array(&json!(other_expedition), php::get(&global_blocked, &ps)));
            if other_n > 0 && !other_blocked && !players::has_opponent_block_move_expedition(g, p, other_expedition)? {
                let mut node_ctx = json!({"pId": pid_v});
                if let Some(s) = source {
                    php::set(&mut node_ctx, "sourceId", json!(s));
                }
                let flow = ft::action(
                    "MoveExpedition",
                    json!({"n": other_n, "pId": pid_v, "force": false, "forceExpedition": [pid_v, other_expedition], "winningBiomes": winning_biomes}),
                    node_ctx,
                );
                insert_as_child(g, ctx, &flow)?;
            } else if other_n < 0 {
                let other_token = if token == "hero" { "companion" } else { "hero" };
                player::advance_storm(g, p, other_token, &winning_biomes, other_n, false)?;
                add_expedition_moves(g, p, other_expedition, &json!(other_n))?;
                let asc = player::is_ascended(g, p, other_expedition)?;
                check_after_listeners(a, g, ctx, p, &json!({"moveExpedition": other_n, "ascended": asc, "expedition": other_expedition}), true, None)?;
            }
        }

        if gigantic {
            // we must move the other expedition
            let expedition = if expedition == "stormLeft" { "stormRight" } else { "stormLeft" };
            let token = if expedition == "stormLeft" { "hero" } else { "companion" };
            if (ni > 0 && !players::has_opponent_block_move_expedition(g, p, expedition)?) || ni < 0 {
                let moved = player::advance_storm(g, p, token, &winning_biomes, ni, false)?;
                if moved {
                    add_expedition_moves(g, p, expedition, &n)?;
                    let asc = player::is_ascended(g, p, expedition)?;
                    check_after_listeners(a, g, ctx, p, &json!({"moveExpedition": n, "ascended": asc, "expedition": expedition}), true, None)?;
                }
            }
        }
    }
    Ok(())
}
