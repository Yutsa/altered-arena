//! Actions/ActivateCard.php: runs a passive reaction. The card's flow is rebuilt from the event
//! (conditions re-checked), tagged with the source, the event and the player, and replaces the
//! node.

use crate::core::engine;
use crate::core::globals;
use crate::core::node::{self, NodeId};
use crate::framework::ST_ACTIVATE_CARD;
use crate::game::{CardId, Game, PId};
use crate::helpers::utils;
use crate::managers::cards;
use crate::models::action::{base_get_player, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct ActivateCard;

/// `getCard()`: null when missing, or destroyed and not listened to
// PHP: Actions/ActivateCard.php:20
fn get_card(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let cid = get_ctx_arg(g, ctx, "cardId");
    let c = cards::get_single(g, &cid, false)?;
    if let Some(c) = c {
        let ctl = php::coalesce(php::get(php::get(&get_ctx_args(g, ctx), "event"), "cardsToListen"), &php::arr()).clone();
        if card::location(g, c)? == "destroy" && !php::in_array(&cid, &ctl) {
            return Ok(None);
        }
    }
    Ok(c)
}

/// `getFlow($player)`: the tagged reaction flow, or null
// PHP: Actions/ActivateCard.php:32
fn get_flow(g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<Value> {
    let Some(c) = get_card(g, ctx)? else { return Ok(Value::Null) };
    let event = get_ctx_arg(g, ctx, "event");
    let cid = get_ctx_arg(g, ctx, "cardId");
    let args_event = php::get(&get_ctx_args(g, ctx), "event").clone();
    let ctl = php::coalesce(php::get(&args_event, "cardsToListen"), &php::arr()).clone();
    let rtl = php::coalesce(php::get(&args_event, "reserveToListen"), &php::arr()).clone();
    let listen = card::is_played(g, c)? || card::location(g, c)? == "reserve" || php::in_array(&cid, &ctl) || php::in_array(&cid, &rtl);
    let flow = if listen {
        let p = player.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        cards::apply_effect(g, c, p, php::get(&event, "method"), &event)?
    } else {
        Value::Null
    };
    if php::loose_eq(&flow, &Value::Null) {
        return Ok(Value::Null);
    }
    let tag_pid = if php::loose_eq(php::coalesce(php::get(&flow, "pId"), &json!(-1)), &json!("controller")) {
        php::coalesce(php::get(&event, "controller"), php::get(&event, "pId")).clone()
    } else if php::isset(&event, "owner") {
        php::coalesce(php::get(&event, "owner"), php::get(&event, "pId")).clone()
    } else if php::isset(&event, "cardPId") && php::strict_eq(php::coalesce(php::get(&event, "cardId"), &Value::Null), &cid) {
        php::get(&event, "cardPId").clone()
    } else {
        let cc = cards::get(g, &cid)?;
        json!(g.card(cc)?.pid)
    };
    let mut flow = utils::tag_tree(&flow, &json!({"sourceId": cid, "event": event, "pId": tag_pid}), false);
    if php::isset(&event, "from") && php::in_list(php::get(&event, "from"), &["stormLeft", "stormRight"]) {
        let from = php::coalesce(php::get(&event, "from"), &json!(card::location(g, c)?)).clone();
        flow = utils::update_tree(&flow, &json!(["source"]), &json!([from]), &["targetLocation"]);
    }
    Ok(flow)
}

/// `getFlowTree($player)`: the flow built as a (detached) tree
// PHP: Actions/ActivateCard.php:96
fn get_flow_tree(g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<Option<NodeId>> {
    let flow = get_flow(g, ctx, player)?;
    if flow.is_null() {
        return Ok(None);
    }
    Ok(Some(engine::build_tree(g, &flow)?))
}

impl ActionClass for ActivateCard {
    fn name(&self) -> &'static str {
        "ActivateCard"
    }

    // PHP: Actions/ActivateCard.php:15
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_ACTIVATE_CARD))
    }

    // PHP: Actions/ActivateCard.php:104
    fn is_optional(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let player = base_get_player(g, ctx)?;
        if get_flow_tree(g, ctx, player)?.is_none() {
            return Ok(true);
        }
        let t = get_flow_tree(g, ctx, player)?.unwrap();
        if node::is_optional(g, t, player)? {
            return Ok(true);
        }
        let t = get_flow_tree(g, ctx, player)?.unwrap();
        if !node::is_doable(g, t, player)? {
            return Ok(true);
        }
        Ok(globals::get_int(g, "engineAutomatic")? > 20)
    }

    // PHP: Actions/ActivateCard.php:113
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/ActivateCard.php:118
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let player = base_get_player(g, ctx)?;
        match get_flow_tree(g, ctx, player)? {
            None => Ok(false),
            Some(t) => node::is_doable(g, t, player),
        }
    }

    // PHP: Actions/ActivateCard.php:126
    fn is_irreversible(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let player = base_get_player(g, ctx)?;
        match get_flow_tree(g, ctx, player)? {
            None => Ok(false),
            Some(t) => node::is_irreversible(g, t, None),
        }
    }

    // PHP: Actions/ActivateCard.php:133
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let player = base_get_player(g, ctx)?;
        match get_flow_tree(g, ctx, player)? {
            None => Ok(false),
            Some(t) => Ok(node::is_independent(g, t, player)? && globals::get_int(g, "engineAutomatic")? < 20),
        }
    }

    // PHP: Actions/ActivateCard.php:140
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let player = base_get_player(g, ctx)?;
        let Some(t) = get_flow_tree(g, ctx, player)? else { return Ok(json!("")) };
        let flow_desc = node::get_description(g, t)?;
        let c = get_card(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getName() on null"))?;
        let name = card::get(g, c, "name")?;
        Ok(json!({"log": "${flowDesc} (${source})", "args": {"i18n": ["flowDesc", "source"], "flowDesc": flow_desc, "source": name}}))
    }

    // PHP: Actions/ActivateCard.php:159
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = base_get_player(g, ctx)?;
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function isMandatory() on null"))?;
        let mut flow = get_flow(g, ctx, player)?;
        if node::is_mandatory(g, n) {
            php::set(&mut flow, "optional", json!(false));
        }
        let built = engine::build_tree(g, &flow)?;
        node::replace(g, n, built)?;
        engine::save(g)?;
        engine::proceed(g, false, false)?;
        Ok(None)
    }
}
