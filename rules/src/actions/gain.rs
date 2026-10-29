//! Actions/Gain.php: gives meeples (boost, fleeting, anchored, asleep) or a counter (`augment`)
//! to a card, respecting `upTo`, the block rules and `dynamicGainReplace`; fires the Gain
//! listeners (with the `gainGroup` of a distribution). `gain()` is also called directly (with no
//! ctx) by ChooseAssignment.

use crate::core::{engine, node};
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::conditions;
use crate::managers::{cards, meeples, players};
use crate::models::action::{self, get_arg, get_ctx_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Gain;

/// `$this->ctx->getSource() ?? null`, falling back to `Cards::getSingle($sourceId)`: the node's
/// `source` info is a raw value (never a card object); using it as a card is an Error
#[derive(Debug, Clone)]
pub enum Source {
    Null,
    Card(CardId),
    Other(Value),
}

impl Source {
    /// The value given to notifications (`card2`)
    pub fn to_value(&self) -> Value {
        match self {
            Source::Null => Value::Null,
            Source::Card(c) => json!(c),
            Source::Other(v) => v.clone(),
        }
    }
}

/// `$source = $this->ctx->getSource() ?? null; $sourceId = ...; if (is_null($source) &&
/// !is_null($sourceId)) $source = Cards::getSingle($sourceId);`
// PHP: Actions/Gain.php:359-363
pub fn ctx_source(g: &mut Game, ctx: Ctx) -> R<Source> {
    let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSource() on null"))?;
    let s = node::get_source(g, n);
    if !s.is_null() {
        return Ok(Source::Other(s));
    }
    let sid = node::get_source_id(g, n);
    if sid.is_null() {
        return Ok(Source::Null);
    }
    Ok(match cards::get_single(g, &sid, true)? {
        Some(c) => Source::Card(c),
        None => Source::Null,
    })
}

/// `Utils::resourcesToStr([$type => $n], true)`: notification text only
pub fn resources_to_str(ty: &Value, n: &Value) -> Value {
    json!({"log": "${n} ${type}", "args": {"n": n, "type": ty}})
}

/// A PHP arithmetic operand: null/bool/int/float/numeric string; other values are a TypeError
fn operand(v: &Value, op: &str, other: &Value) -> R<Value> {
    match v {
        Value::Null => Ok(json!(0)),
        Value::Bool(b) => Ok(json!(*b as i64)),
        Value::Number(_) => Ok(v.clone()),
        Value::String(s) if php::is_numeric_str(s) => {
            let t = s.trim();
            match t.parse::<i64>() {
                Ok(i) => Ok(json!(i)),
                Err(_) => Ok(json!(php::floatval(v))),
            }
        }
        _ => Err(PhpError::new(format!("TypeError: Unsupported operand types: {} {} {}", php::type_name(v), op, php::type_name(other)))),
    }
}

/// `$a + $b` / `$a - $b` on PHP numbers
pub fn num_op(a: &Value, b: &Value, op: &str) -> R<Value> {
    let x = operand(a, op, b)?;
    let y = operand(b, op, a)?;
    if let (Some(i), Some(j)) = (x.as_i64(), y.as_i64()) {
        let r = if op == "+" { i.checked_add(j) } else { i.checked_sub(j) };
        if let Some(r) = r {
            return Ok(json!(r));
        }
    }
    let (i, j) = (php::floatval(&x), php::floatval(&y));
    Ok(json!(if op == "+" { i + j } else { i - j }))
}

/// `getCard()`: `cardId` (ME = the source, EFFECT = the card of the closest event)
// PHP: Actions/Gain.php:162
fn get_card(g: &mut Game, ctx: Ctx) -> R<Option<CardId>> {
    let mut card_id = get_ctx_arg(g, ctx, "cardId");
    if php::loose_eq(&card_id, &json!("me")) {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getSourceId() on null"))?;
        card_id = node::get_source_id(g, n);
    } else if php::loose_eq(&card_id, &json!("EFFECT")) {
        let event = action::get_event_recursive(g, ctx)?;
        card_id = Value::Null;
        if !event.is_null() {
            card_id = php::get(&event, "cardId").clone();
            if card_id.is_null() {
                card_id = php::get(php::get(&event, "gain"), "cardId").clone();
            }
        }
    }
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Gain). Should not happen"));
    }
    cards::get_single(g, &card_id, true)
}

fn card_req(c: Option<CardId>, method: &str) -> R<CardId> {
    c.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

fn player_req(p: Option<PId>, method: &str) -> R<PId> {
    p.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `getGain()`: `[type, n]` (`['augment', 1]` for an augment; `sourceCounter2` = the source's
/// counter + 2)
// PHP: Actions/Gain.php:191
fn get_gain(a: &Gain, g: &mut Game, ctx: Ctx) -> R<(Value, Value)> {
    if php::strict_eq(&get_arg(a, g, ctx, "augment")?, &json!(true)) {
        return Ok((json!("augment"), json!(1)));
    }
    let mut n = get_arg(a, g, ctx, "n")?;
    if php::loose_eq(&n, &json!("sourceCounter2")) {
        if let Some(s) = action::get_source(g, ctx)? {
            let data = card::get(g, s, "extraDatas")?;
            n = num_op(php::coalesce(php::get(&data, "counter"), &json!(0)), &json!(2), "+")?;
        }
    }
    Ok((get_arg(a, g, ctx, "type")?, n))
}

/// `getUpTo()`: in Reserve, the controller's `reserveAdd` is added to a limited `upTo`
// PHP: Actions/Gain.php:206
fn get_up_to(a: &Gain, g: &mut Game, ctx: Ctx) -> R<Value> {
    let up_to = get_arg(a, g, ctx, "upTo")?;
    if php::ge(&up_to, &json!(99)) {
        return Ok(up_to);
    }
    let cid = get_ctx_arg(g, ctx, "cardId");
    let in_reserve = |g: &mut Game| -> R<bool> {
        let c = card_req(get_card(g, ctx)?, "getLocation")?;
        Ok(card::location(g, c)? == "reserve")
    };
    if php::loose_ne(&cid, &json!("EFFECT")) && !cid.is_null() && in_reserve(g)? {
        if php::loose_eq(&get_ctx_arg(g, ctx, "cardId"), &json!("me")) && action::get_source_id(g, ctx)?.is_null() {
            return Ok(up_to);
        }
        let c = card_req(get_card(g, ctx)?, "getPlayer")?;
        let p = player_req(card::get_player(g, c)?, "getReserveAdd")?;
        let add = player::get_reserve_add(g, p)?;
        num_op(&json!(add), &up_to, "+")
    } else {
        Ok(up_to)
    }
}

/// `getGainGroup()`: the id of the enclosing distribution (`groupGains` node), created on first
/// use (PHP: `'gains-' . substr(md5(uniqid('', true)), 0, 12)`; any unique id does)
// PHP: Actions/Gain.php:229
fn get_gain_group(g: &mut Game, ctx: Ctx) -> R<Value> {
    let mut nd = ctx;
    while let Some(n) = nd {
        if php::truthy(php::coalesce(&node::info(g, n, "groupGains"), &json!(false))) {
            if node::info(g, n, "gainGroupId").is_null() {
                let id = format!("gains-{:06x}{:06x}", (g.notif_seq & 0xff_ffff), (n as u64 & 0xff_ffff) ^ ((g.next_meeple_id as u64) & 0xff_ffff));
                node::set_info(g, n, "gainGroupId", json!(id));
                engine::save(g)?;
            }
            return Ok(node::info(g, n, "gainGroupId"));
        }
        nd = node::parent(g, n);
    }
    Ok(Value::Null)
}

/// `gain($player, $card, $resource, $amount, $source, $args)` (`$ctx` is null when called by
/// ChooseAssignment through `Actions::get(GAIN)`)
// PHP: Actions/Gain.php:246
#[allow(clippy::too_many_arguments)]
pub fn gain(g: &mut Game, ctx: Ctx, player: Option<PId>, c: CardId, resource: &Value, amount: &Value, source: &Source, args: Value) -> R<()> {
    let a = &Gain;
    let mut args = args;
    let mut resource = resource.clone();
    let dynamic_replace = card::get(g, c, "dynamicGainReplace")?;
    php::set(&mut args, "cardId", json!(c));

    if php::in_list(&json!(card::location(g, c)?), &["hand", "discard"]) {
        return Ok(());
    }

    let source_id = match source {
        Source::Null => json!(-1),
        Source::Card(s) => json!(s),
        Source::Other(v) => return Err(PhpError::new(format!("Error: Call to a member function getId() on {}", php::type_name(v)))),
    };

    // Some effects change what is gained
    if php::isset(&dynamic_replace, &php::key_of(&resource)) {
        let old = resource.clone();
        resource = php::get(&dynamic_replace, &php::key_of(&old)).clone();
        g.notify("mediumMessage", json!({"log": "${old_resource} is replaced by ${resource} (${card_name}'s effect)", "resource": resource, "old_resource": old, "cardId": c}));
        php::set(&mut args, "type", resource.clone());
    }
    php::set(&mut args, "type", resource.clone());

    if php::in_list(&resource, &["fleeting", "asleep", "anchored"]) && card::has_token(g, c, &php::strval(&resource))? {
        if card::is(g, c, "canAlwaysGainFleeting")? {
            let p = player_req(player, "getId")?;
            let tok = card::is_token(g, c)?;
            action::check_after_listeners(a, g, ctx, p, &json!({"gain": args, "sourceId": source_id, "token": tok}), true, None)?;
        }
        // a card cannot have more than one fleeting/anchored token
        return Ok(());
    }
    if php::le(amount, &json!(0)) {
        return Ok(());
    }

    let mut initial_boost = 0;
    if php::loose_eq(&resource, &json!("boost")) {
        initial_boost = card::count_token(g, c, "boost")?;
    }

    let p = player_req(player, "getId")?;
    let tokens = meeples::create_on_card(g, &php::strval(&resource), c, &json!(p), php::intval(amount))?;
    g.notify("addMeeples", json!({"cardId": c, "power": resource, "meeples": tokens, "card2": source.to_value(), "n": tokens.len()}));

    let loc = card::location(g, c)?;
    let ty = card::get(g, c, "type")?;
    let add = card::get_additional_type(g, c)?;
    let tok = card::is_token(g, c)?;
    let mut listeners = json!({"gain": args, "cardId": c, "location": loc, "initialBoost": initial_boost, "cardType": ty, "additionalType": add, "sourceId": source_id, "token": tok});
    let group = get_gain_group(g, ctx)?;
    if !group.is_null() {
        php::set(&mut listeners, "gainGroup", group);
    }
    action::check_after_listeners(a, g, ctx, p, &listeners, true, None)
}

/// `isCantGainBoostRuleActive($sourceCard, $rule)`: `character:excludeSelf[:cond|cond...]`
// PHP: Actions/Gain.php:305
fn is_cant_gain_boost_rule_active(g: &mut Game, source_card: CardId, rule: &Value) -> R<bool> {
    let Value::String(rule) = rule else { return Ok(false) };
    let prefix = "character:excludeSelf";
    if !rule.starts_with(prefix) {
        return Ok(false);
    }
    let mut cs = &rule[prefix.len()..];
    if cs.is_empty() {
        return Ok(true);
    }
    if cs.starts_with(':') {
        cs = &cs[1..];
    }
    if cs.is_empty() {
        return Ok(true);
    }
    let conds: Vec<String> = if cs.contains('|') { php::explode("|", cs) } else { vec![cs.to_string()] };
    conditions::check(g, &json!({"conditions": conds}), Some(source_card), &php::arr())
}

/// `isBoostGainBlockedByPassive($card, $resource)`: another played card of the controller has
/// a `cantGainBoost` rule that applies
// PHP: Actions/Gain.php:329
fn is_boost_gain_blocked_by_passive(g: &mut Game, c: CardId, resource: &Value) -> R<bool> {
    if php::loose_ne(resource, &json!("boost")) || !php::in_list(&card::get(g, c, "type")?, &["character", "token"]) {
        return Ok(false);
    }
    let p = player_req(card::get_player(g, c)?, "getPlayedCards")?;
    for sc in player::get_played_cards(g, p, &Value::Null)? {
        if sc == c {
            continue;
        }
        let cgb = card::get_property(g, sc, "cantGainBoost")?;
        if cgb.is_null() {
            continue;
        }
        let rules: Vec<Value> = if php::is_arr(&cgb) { php::vals(&cgb).into_iter().cloned().collect() } else { vec![cgb] };
        for rule in rules {
            if is_cant_gain_boost_rule_active(g, sc, &rule)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `$block = $card->getBlockAutomaticAction(); isset($block[$action][$gain])` over the played
/// cards of every player
pub fn blocks_automatic_action(g: &mut Game, action_name: &str, gain: &Value) -> R<bool> {
    for c in cards::get_played_cards(g, &Value::Null, &Value::Null)? {
        let block = card::get(g, c, "blockAutomaticAction")?;
        let b = php::get(&block, action_name);
        if php::isset(&block, action_name) && php::is_arr(b) && php::isset(b, &php::key_of(gain)) {
            return Ok(true);
        }
    }
    Ok(false)
}

impl ActionClass for Gain {
    fn name(&self) -> &'static str {
        "Gain"
    }

    fn default_args(&self) -> Value {
        json!({"n": 1, "augment": false, "type": "", "upTo": 99})
    }

    // PHP: Actions/Gain.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_GAIN))
    }

    // PHP: Actions/Gain.php:21
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let p = self.get_player(g, ctx)?;
        let (ty, n) = get_gain(self, g, ctx)?;
        let desc = resources_to_str(&ty, &n);
        let up_to = get_up_to(self, g, ctx)?;

        if php::loose_eq(&get_arg(self, g, ctx, "augment")?, &json!(true)) {
            return Ok(json!({"log": "augment", "args": []}));
        }

        let p = player_req(p, "getId")?;
        let mine = p == players::get_active_id(g);
        if php::ge(&up_to, &json!(99)) {
            if mine {
                Ok(json!({"log": "Gain ${resources_desc}", "args": {"resources_desc": desc}}))
            } else {
                Ok(json!({"log": "Let ${player_name} gain ${resources_desc}", "args": {"player_name": player::get_name(g, p)?, "resources_desc": desc}}))
            }
        } else if mine {
            Ok(json!({"log": "Gain ${resources_desc}  (up to ${upTo})", "args": {"resources_desc": desc, "upTo": up_to}}))
        } else {
            Ok(json!({"log": "Let ${player_name} gain ${resources_desc} (up to ${upTo})", "args": {"player_name": player::get_name(g, p)?, "resources_desc": desc, "upTo": up_to}}))
        }
    }

    // PHP: Actions/Gain.php:78
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Gain.php:91
    fn is_doable(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        if !php::loose_eq(&get_ctx_arg(g, ctx, "cardId"), &json!("me")) {
            return Ok(true);
        }
        let event = action::get_event_recursive(g, ctx)?;
        if !event.is_null() && php::isset(&event, "action") && php::loose_eq(php::get(&event, "action"), &json!("Discard")) && php::loose_eq(php::get(&event, "sourceLocation"), &json!("discard")) {
            return Ok(false);
        }
        if !event.is_null() && php::isset(&event, "action") && php::loose_eq(php::get(&event, "action"), &json!("ChooseAssignment")) && php::loose_eq(php::get(&event, "sourceLocation"), &json!("reserve")) {
            let sid = action::get_source_id(g, ctx)?;
            let c = cards::get(g, &sid)?;
            if php::in_array(&json!(c), php::coalesce(php::get(&event, "reserveToListen"), &php::arr())) && card::location(g, c)? != "reserve" {
                return Ok(false);
            }
        }
        let sid = action::get_source_id(g, ctx)?;
        let c = cards::get(g, &sid)?;
        let (gain, _n) = get_gain(self, g, ctx)?;
        if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) && php::loose_eq(&gain, &json!("fleeting")) && card::has_token(g, c, "fleeting")? {
            return Ok(false);
        }
        Ok(true)
    }

    // PHP: Actions/Gain.php:138
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        let gain = if php::truthy(&get_arg(self, g, ctx, "augment")?) { json!("boost") } else { get_arg(self, g, ctx, "type")? };
        Ok(!blocks_automatic_action(g, "Gain", &gain)?)
    }

    // PHP: Actions/Gain.php:156
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let pid = get_ctx_arg(g, ctx, "pId");
        let pid = if pid.is_null() { json!(players::get_active_id(g)) } else { pid };
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/Gain.php:356
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let player = self.get_player(g, ctx)?;
        let source = ctx_source(g, ctx)?;
        let c = get_card(g, ctx)?;
        let args = get_ctx_args(g, ctx);
        let up_to = get_up_to(self, g, ctx)?;

        let (mut resource, mut amount) = get_gain(self, g, ctx)?;

        let c = card_req(c, "getLocation")?;
        if card::location(g, c)? == "reserve" && players::has_block_opponent_reserve_gain(g, player_req(player, "getId")?)? {
            g.notify("mediumMessage", json!({"log": "No counter can be gained in Reserve"}));
            action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
            return Ok(None);
        }

        let in_game = ["stormLeft", "stormRight", "landmark", "reserve"];
        if php::loose_eq(&resource, &json!("augment")) {
            if in_game.contains(&card::location(g, c)?.as_str()) && players::has_block_gain_new_counters(g)? {
                g.notify("mediumMessage", json!({"log": "No new counter can be added to cards"}));
                action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
                return Ok(None);
            }

            if card::count_token(g, c, "boost")? > 0 {
                resource = json!("boost");
            } else {
                // we need to increase the counter
                let mut data = card::get(g, c, "extraDatas")?;
                let counter = num_op(php::coalesce(php::get(&data, "counter"), &json!(0)), &json!(1), "+")?;
                php::set(&mut data, "counter", counter.clone());
                card::set_property(g, c, "extraDatas", data, true)?;

                g.notify("gainCounter", json!({"cardId": c, "value": counter, "increase": 1}));
                let cp = player_req(card::get_player(g, c)?, "getId")?;
                let tok = card::is_token(g, c)?;
                action::check_after_listeners(self, g, ctx, cp, &json!({"specialEffect": "gainCounter", "augment": true, "cardId": c, "token": tok}), true, Some("SpecialEffect"))?;
                action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
                return Ok(None);
            }
        }

        if php::loose_eq(&resource, &json!("boost")) && in_game.contains(&card::location(g, c)?.as_str()) && card::has_counters(g, c)? && players::has_block_gain_new_counters(g)? {
            g.notify("mediumMessage", json!({"log": "No new boost can be added to cards"}));
            action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
            return Ok(None);
        }

        if is_boost_gain_blocked_by_passive(g, c, &resource)? {
            g.notify("mediumMessage", json!({"log": "This Character can't gain boosts", "cardId": c}));
            action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
            return Ok(None);
        }

        // check that we are not going to gain more than necessary
        let owned = json!(card::count_token(g, c, &php::strval(&resource))?);
        if php::ge(&owned, &up_to) {
            action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
            return Ok(None);
        } else if php::gt(&num_op(&owned, &amount, "+")?, &up_to) {
            amount = num_op(&up_to, &owned, "-")?;
        }

        gain(g, ctx, player, c, &resource, &amount, &source, args)?;
        action::resolve_action(self, g, ctx, Value::Null, Value::Null)?;
        Ok(None)
    }
}
