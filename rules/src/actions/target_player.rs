//! Actions/TargetPlayer.php: targets a player, who then runs the effect.

use crate::core::{engine, node};
use crate::framework::*;
use crate::game::Game;
use crate::managers::players;
use crate::models::action::{get_arg, get_ctx_arg, get_ctx_args, get_source, get_source_id, insert_as_child, param_req, ActionClass, Ctx};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct TargetPlayer;

impl ActionClass for TargetPlayer {
    fn name(&self) -> &'static str {
        "TargetPlayer"
    }

    // PHP: Actions/TargetPlayer.php:24
    fn default_args(&self) -> Value {
        json!({"opponentsOnly": true, "onlyMe": false})
    }

    // PHP: Actions/TargetPlayer.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET_PLAYER))
    }

    // PHP: Actions/TargetPlayer.php:29
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let n = php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)).clone();
        let t = engine::build_tree(g, &get_ctx_arg(g, ctx, "effect"))?;
        let effect_desc = node::get_description(g, t)?;
        Ok(json!({"log": "Target player to ${effect_desc}", "args": {"n": n, "effect_desc": effect_desc, "i18n": ["effect_desc"]}}))
    }

    // PHP: Actions/TargetPlayer.php:43
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let _player = players::get_active(g);
        let opponents_only = get_arg(self, g, ctx, "opponentsOnly")?;
        let description = self.get_description(g, ctx)?;
        Ok(Some(json!({"opponentsOnly": opponents_only, "description": description})))
    }

    // PHP: Actions/TargetPlayer.php:53
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        if php::loose_eq(&get_arg(self, g, ctx, "onlyMe")?, &json!(true)) {
            let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            return Ok(Some(json!([p])));
        } else if php::loose_eq(&get_arg(self, g, ctx, "opponentsOnly")?, &json!(true)) && players::count(g) == 2 {
            let p = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            return Ok(Some(json!([players::get_next_id(g, p)?])));
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttargetplayer" => {
                let pid = param_req(args, 0, "pId", "ALT\\Actions\\TargetPlayer::actTargetPlayer")?;
                act_target_player(self, g, ctx, pid)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\TargetPlayer::{}()", method))),
        }
    }
}

/// `actTargetPlayer($pId)`: the effect inserted as a child, run by the targeted player
// PHP: Actions/TargetPlayer.php:62
fn act_target_player(a: &TargetPlayer, g: &mut Game, ctx: Ctx, pid: Value) -> R<Value> {
    let player = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let opponents_only = get_arg(a, g, ctx, "opponentsOnly")?;
    if php::loose_eq(&opponents_only, &json!(true)) && php::loose_eq(&pid, &json!(player)) {
        return Err(PhpError::new("You cannot target yourself. Should not happen"));
    }
    let args = get_ctx_args(g, ctx);

    let mut node = get_arg(a, g, ctx, "effect")?;
    let sid = get_source_id(g, ctx)?;
    php::set(&mut node, "sourceId", sid.clone());
    if php::isset(&args, "expedition") {
        php::set(php::get_mut(&mut node, "args"), "expedition", php::get(&args, "expedition").clone());
    }
    if php::isset(&args, "player") {
        php::set(php::get_mut(&mut node, "args"), "player", php::get(&args, "player").clone());
    }
    if php::isset(&node, "childs") {
        for k in php::keys_str(php::get(&node, "childs")) {
            let child = php::get_mut(php::get_mut(&mut node, "childs"), &k);
            php::set(child, "sourceId", sid.clone());
            if php::isset(&args, "expedition") {
                php::set(php::get_mut(child, "args"), "expedition", php::get(&args, "expedition").clone());
            }
            if php::isset(&args, "player") {
                php::set(php::get_mut(child, "args"), "player", php::get(&args, "player").clone());
            }
            php::set(php::get_mut(&mut node, "args"), "pId", pid.clone());
        }
    }
    php::set(&mut node, "pId", pid.clone());
    php::set(php::get_mut(&mut node, "args"), "pId", pid.clone());

    insert_as_child(g, ctx, &node)?;
    let _card = get_source(g, ctx)?;
    g.notify("message", json!({"log": "${player_name} targets ${player_name2} for ${card_name}'s effect", "player_id": player, "player_id2": pid}));
    Ok(json!([pid]))
}
