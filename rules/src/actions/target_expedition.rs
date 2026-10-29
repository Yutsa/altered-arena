//! Actions/TargetExpedition.php: targets expeditions, then runs the effect on each of them.

use crate::core::{engine, globals, node};
use crate::framework::*;
use crate::game::{Game, PId};
use crate::managers::players;
use crate::models::action::{get_arg, get_ctx_arg, get_ctx_args, get_source, get_source_id, param_req, push_parallel_child, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct TargetExpedition;

/// `argsTargetExpedition()`
// PHP: Actions/TargetExpedition.php:46
fn args_target_expedition(a: &TargetExpedition, g: &mut Game, ctx: Ctx) -> R<Value> {
    let mut expeditions = vec![];
    let expedition_type = php::get(&get_ctx_args(g, ctx), "type").clone();
    let storm_moves = globals::get(g, "stormMoves")?;
    let winners = players::get_winning_player_by_storms(g)?;
    let other_than_me = get_arg(a, g, ctx, "otherThanMe")?;

    let mut source_location = Value::Null;
    let mut source_pid = Value::Null;
    if php::truthy(&other_than_me) {
        let source = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        source_location = json!(card::location(g, source)?);
        source_pid = json!(card::pid(g, source)?);
    }

    let players_arg = get_arg(a, g, ctx, "players")?;
    let pls: Vec<PId> = if php::loose_eq(&players_arg, &json!("all")) {
        players::get_all(g)
    } else if php::loose_eq(&players_arg, &json!("me")) {
        let p = a.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        vec![p]
    } else {
        let p = a.get_player(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        let n = players::get_next(g, p)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        vec![n]
    };

    for p in pls {
        for storm in ["stormLeft", "stormRight"] {
            if php::loose_eq(&expedition_type, &json!("ahead")) && php::loose_ne(php::get(&winners, storm), &json!(p)) {
                continue;
            }
            let moves = php::coalesce(php::get(php::get(php::get(&storm_moves, &p.to_string()), storm), "moves"), &json!(0)).clone();
            if php::loose_eq(&expedition_type, &json!("movedThisDusk")) && php::le(&moves, &json!(0)) {
                continue;
            }
            if other_than_me == json!(true) && php::loose_eq(&source_location, &json!(storm)) && php::loose_eq(&source_pid, &json!(p)) {
                continue;
            }
            expeditions.push(json!(format!("{}-{}", p, storm)));
        }
    }
    let n = get_arg(a, g, ctx, "n")?;
    Ok(json!({"expeditions": expeditions, "n": n}))
}

impl ActionClass for TargetExpedition {
    fn name(&self) -> &'static str {
        "TargetExpedition"
    }

    // PHP: Actions/TargetExpedition.php:24
    fn default_args(&self) -> Value {
        json!({"n": 1, "players": "all", "otherThanMe": false})
    }

    // PHP: Actions/TargetExpedition.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET_EXPEDITION))
    }

    // PHP: Actions/TargetExpedition.php:26
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let mut msg = "Target ${n} expeditions to ${effect_desc}";
        if php::loose_eq(&get_arg(self, g, ctx, "players")?, &json!("opponent")) {
            msg = "Target ${n} opponent's expeditions to ${effect_desc}";
        }
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        let t = engine::build_tree(g, &get_ctx_arg(g, ctx, "effect"))?;
        let effect_desc = node::get_description(g, t)?;
        Ok(json!({"log": msg, "args": {"n": n, "effect_desc": effect_desc}}))
    }

    // PHP: Actions/TargetExpedition.php:46
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_target_expedition(self, g, ctx)?))
    }

    // PHP: Actions/TargetExpedition.php:91
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(php::count(php::get(&args_target_expedition(self, g, ctx)?, "expeditions")) > 0)
    }

    // PHP: Actions/TargetExpedition.php:96
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        if php::loose_eq(php::coalesce(&get_ctx_arg(g, ctx, "expedition"), &json!("")), &json!("all")) {
            return Ok(Some(json!(["all"])));
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttargetexpedition" => {
                let expedition = param_req(args, 0, "expedition", "ALT\\Actions\\TargetExpedition::actTargetExpedition")?;
                act_target_expedition(self, g, ctx, expedition)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\TargetExpedition::{}()", method))),
        }
    }
}

/// `actTargetExpedition($expedition)`: the effect pushed as a parallel child per expedition
// PHP: Actions/TargetExpedition.php:103
fn act_target_expedition(a: &TargetExpedition, g: &mut Game, ctx: Ctx, expedition: Value) -> R<Value> {
    let _player = a.get_player(g, ctx)?;
    let args = args_target_expedition(a, g, ctx)?;
    let mut expedition = expedition;
    if php::loose_eq(&expedition, &json!("all")) {
        expedition = php::get(&args, "expeditions").clone();
    }
    let count = php::count(&expedition);
    for (index, exp) in php::entries(&expedition) {
        let parts = php::explode("-", &php::strval(exp));
        if !php::in_array(exp, php::get(&args, "expeditions")) {
            return Err(PhpError::new("Invalid target expedition. Should not happen"));
        }
        let pid = json!(parts[0]);
        let side = json!(parts.get(1).cloned().unwrap_or_default());

        let mut node = get_arg(a, g, ctx, "effect")?;
        let sid = get_source_id(g, ctx)?;
        php::set(&mut node, "sourceId", sid.clone());
        let nargs = php::get_mut(&mut node, "args");
        php::set(nargs, "player", pid.clone());
        php::set(nargs, "expedition", side.clone());
        php::set(nargs, "forceExpedition", json!([pid, side]));
        if count > 1 && php::gt(&php::key_value(&index), &json!(0)) {
            php::set(php::get_mut(&mut node, "args"), "moreThan1", json!(true));
        }
        if php::isset(&node, "childs") {
            for k in php::keys_str(php::get(&node, "childs")) {
                let child = php::get_mut(php::get_mut(&mut node, "childs"), &k);
                php::set(child, "sourceId", sid.clone());
                let cargs = php::get_mut(child, "args");
                php::set(cargs, "player", pid.clone());
                php::set(cargs, "expedition", side.clone());
                php::set(cargs, "forceExpedition", json!([pid, side]));
            }
        }
        push_parallel_child(g, ctx, &node)?;
    }
    Ok(json!([expedition]))
}
