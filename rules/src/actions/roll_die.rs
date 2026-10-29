//! Actions/RollDie.php: rolls dice (Lyra Bastion adds dice, Martingale / Fragrant Meerkat add
//! +1 options, Luck counters raise the result, Bastion discards a Reserve card for +2), then
//! runs the effect of the selected result.

use crate::core::globals;
use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::{ft, log, utils};
use crate::managers::{cards, players};
use crate::models::action::{
    check_after_listeners, check_immediate_listeners, duplicate_action, get_arg, get_ctx_arg, insert_as_child, param_req, resolve_action, ActionClass, Ctx,
};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct RollDie;

/// The value of the overridden `getSource()`: the node's `source` info (not a card object when
/// set), else the card of `sourceId`
enum Source {
    None,
    Card(CardId),
    Other(Value),
}

impl Source {
    /// `$source->...()`: a method call on the source
    fn card(&self, method: &str) -> R<CardId> {
        match self {
            Source::Card(c) => Ok(*c),
            Source::None => Err(PhpError::new(format!("Error: Call to a member function {}() on null", method))),
            Source::Other(v) => Err(PhpError::new(format!("Error: Call to a member function {}() on {}", method, php::type_name(v)))),
        }
    }
}

fn ctx_node(ctx: Ctx, method: &str) -> R<node::NodeId> {
    ctx.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `getSource()`
// PHP: Actions/RollDie.php:66
fn get_source(g: &Game, ctx: Ctx) -> R<Source> {
    let n = ctx_node(ctx, "getSource")?;
    let source = node::get_source(g, n);
    let source_id = node::get_source_id(g, n);
    if source.is_null() && !source_id.is_null() {
        return Ok(match cards::get_single(g, &source_id, true)? {
            Some(c) => Source::Card(c),
            None => Source::None,
        });
    }
    if source.is_null() {
        return Ok(Source::None);
    }
    Ok(Source::Other(source))
}

/// `$this->getPlayer() ?? Players::getActive()`
fn player_or_active(a: &RollDie, g: &mut Game, ctx: Ctx) -> R<PId> {
    match a.get_player(g, ctx)? {
        Some(p) => Ok(p),
        None => players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getAddDice() on null")),
    }
}

/// `getN($player)`: `[n + extra dice, extra dice]`
// PHP: Actions/RollDie.php:36
fn get_n(a: &RollDie, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<(i64, i64)> {
    let n = php::intval(php::coalesce(&get_ctx_arg(g, ctx, "n"), &json!(1)));
    let p = match player {
        Some(p) => p,
        None => player_or_active(a, g, ctx)?,
    };
    // Lyra Bastion management
    let extra_rolls = player::get_add_dice(g, p)?;
    Ok((n + extra_rolls, extra_rolls))
}

/// `getGain($roll)`: the effect whose range contains the roll (`X-Y`, `X+`, `X`)
// PHP: Actions/RollDie.php:76
fn get_gain(a: &RollDie, g: &mut Game, ctx: Ctx, roll: i64) -> R<Value> {
    let mut effect = Value::Null;
    for (qty, gain) in php::entries(&get_arg(a, g, ctx, "effect")?) {
        let (lower, upper) = if qty.contains('-') {
            let t = php::explode("-", &qty);
            (php::intval(&json!(t[0])), Some(php::intval(&json!(t.get(1).cloned().unwrap_or_default()))))
        } else if qty.contains('+') {
            let t = php::explode("+", &qty);
            (php::intval(&json!(t[0])), None)
        } else {
            let v = php::intval(&json!(qty));
            (v, Some(v))
        };
        if roll >= lower && upper.is_none_or(|u| roll <= u) {
            if php::loose_ne(&effect, &Value::Null) {
                return Err(PhpError::new(format!("Duplicate effect found for roll : {}", roll)));
            }
            effect = gain.clone();
        }
    }
    if php::loose_eq(&effect, &json!("OPPOSITE")) {
        return Ok(Value::Null);
    }
    Ok(effect)
}

/// `getSourceCounterValue()`: the Luck counters of the source, when they can raise the roll
// PHP: Actions/RollDie.php:114
fn get_source_counter_value(a: &RollDie, g: &mut Game, ctx: Ctx) -> R<i64> {
    if !php::truthy(&get_arg(a, g, ctx, "allowCounterIncrease")?) {
        return Ok(0);
    }
    let source = get_source(g, ctx)?;
    if let Source::None = source {
        return Ok(0);
    }
    let c = source.card("getExtraDatas")?;
    Ok(php::intval(php::coalesce(php::get(&card::get(g, c, "extraDatas")?, "counter"), &json!(0))))
}

/// `getSelectableRolls($rolls, $counterValue)`: each roll up to roll + counters, sorted, unique
// PHP: Actions/RollDie.php:128
fn get_selectable_rolls(rolls: &Value, counter_value: i64) -> Vec<i64> {
    let mut selectable = vec![];
    for roll in php::vals(rolls) {
        let r = php::intval(roll);
        selectable.push(r);
        for i in 1..=counter_value {
            selectable.push(r + i);
        }
    }
    selectable.sort();
    selectable.dedup();
    selectable
}

/// `getCounterSpendForRoll($selectedRoll, $rawRolls, $counterValue)`
// PHP: Actions/RollDie.php:141
fn get_counter_spend_for_roll(a: &RollDie, g: &mut Game, ctx: Ctx, selected: i64, raw_rolls: &Value, counter_value: i64) -> R<i64> {
    if counter_value <= 0 || !php::truthy(&get_arg(a, g, ctx, "allowCounterIncrease")?) {
        return Ok(0);
    }
    let mut min_spend: Option<i64> = None;
    for roll in php::vals(raw_rolls) {
        let r = php::intval(roll);
        if selected < r {
            continue;
        }
        let needed = selected - r;
        if needed <= counter_value && min_spend.is_none_or(|m| needed < m) {
            min_spend = Some(needed);
        }
    }
    min_spend.ok_or_else(|| PhpError::new("Selected die value cannot be reached with available Luck counters."))
}

/// `argsRollDie()`
// PHP: Actions/RollDie.php:207
fn args_roll_die(a: &RollDie, g: &mut Game, ctx: Ctx) -> R<Value> {
    let p = player_or_active(a, g, ctx)?;
    let source = get_source(g, ctx)?;
    let counter_value = get_source_counter_value(a, g, ctx)?;
    let raw_rolls = globals::get(g, "diceRolls")?;
    let rolls = get_selectable_rolls(&raw_rolls, counter_value);
    let can_discard = php::truthy(&get_arg(a, g, ctx, "canDiscard")?) && !player::get_reserve_cards(g, p)?.is_empty();
    let allow = get_arg(a, g, ctx, "allowCounterIncrease")?;
    let card_ids = player::get_reserve_cards(g, p)?;
    // the source card is serialized by BGA: only its id is kept here
    let source_v = match source {
        Source::None => json!(""),
        Source::Card(c) => json!({"id": c}),
        Source::Other(v) => v,
    };
    Ok(json!({
        "rolls": rolls,
        "rawRolls": raw_rolls,
        "canDiscard": can_discard,
        "source": source_v,
        "counterValue": counter_value,
        "allowCounterIncrease": allow,
        "cardIds": card_ids,
        "descSuffix": if can_discard { "bastion" } else { "" },
    }))
}

impl ActionClass for RollDie {
    fn name(&self) -> &'static str {
        "RollDie"
    }

    // PHP: Actions/RollDie.php:58
    fn default_args(&self) -> Value {
        json!({"n": 1, "effect": [], "canDiscard": false, "allowCounterIncrease": false, "hasRolled": false})
    }

    // PHP: Actions/RollDie.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_ROLL_DIE))
    }

    // PHP: Actions/RollDie.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let (n_total, _extra) = get_n(self, g, ctx, None)?;
        Ok(json!({"log": "Roll ${n} die", "args": {"n": n_total}}))
    }

    // PHP: Actions/RollDie.php:48
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/RollDie.php:53
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/RollDie.php:207
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_roll_die(self, g, ctx)?))
    }

    // PHP: Actions/RollDie.php:234
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let args = args_roll_die(self, g, ctx)?;
        if !php::truthy(php::get(&args, "canDiscard")) && php::count(php::get(&args, "rolls")) == 1 {
            return Ok(Some(json!([php::geti(php::get(&args, "rolls"), 0)])));
        }
        Ok(None)
    }

    fn has_st_pre(&self) -> bool {
        true
    }

    /// `stPreRollDie()`: the dice are rolled (once per node), the +1 options added
    // PHP: Actions/RollDie.php:165
    fn st_pre(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let p = player_or_active(self, g, ctx)?;
        let (n_total, extra_rolls) = get_n(self, g, ctx, Some(p))?;
        let mut rolls = vec![];
        let source = get_source(g, ctx)?;
        let n = ctx_node(ctx, "getFlag")?;
        if php::truthy(&get_arg(self, g, ctx, "hasRolled")?) || php::loose_eq(&node::get_flag(g, n), &json!("preActionDone")) {
            return Ok(Value::Null);
        }
        if extra_rolls > 0 {
            g.notify("message", json!({"log": "${n} dice are added to the roll (Ouroboros Lyra Bastion's effect)", "n": extra_rolls}));
        }
        for _ in 0..n_total {
            let mut roll = g.bga_rand(1, 6);
            if g.environment == "studio" {
                roll = 5;
            }
            rolls.push(roll);
        }

        // Martengale / Fragrant Meerkat: offer +1 (per card) after seeing the result
        let mut new_rolls = vec![];
        let add_roll = player::get_add_roll(g, p)?;
        for roll in &rolls {
            new_rolls.push(*roll);
            for i in 1..=add_roll {
                new_rolls.push(roll + i);
            }
        }

        // Notifications::roll: `card2` is the source (getId on a non-card fails)
        if let Source::Other(_) = source {
            source.card("getId")?;
        }
        g.notify("roll", json!({"player_id": p, "rolls": rolls}));
        globals::set(g, "diceRolls", json!(new_rolls))?;
        log::checkpoint(g)?;
        let sid = source.card("getId")?;
        let dr = globals::get(g, "diceRolls")?;
        check_immediate_listeners(self, g, p, &json!({"rolls": dr, "sourceId": sid}))?;
        Ok(json!(true))
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actrolldie" => {
                let v = param_req(args, 0, "dieValue", "ALT\\Actions\\RollDie::actRollDie")?;
                act_roll_die(self, g, ctx, &v)?;
                Ok(Value::Null)
            }
            "actdiscardadd" => {
                let v = param_req(args, 0, "cardId", "ALT\\Actions\\RollDie::actDiscardAdd")?;
                act_discard_add(self, g, ctx, &v)?;
                Ok(Value::Null)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\RollDie::{}()", method))),
        }
    }
}

/// `actRollDie($dieValue)`: the effect of the selected value, Luck counters spent
// PHP: Actions/RollDie.php:245
fn act_roll_die(a: &RollDie, g: &mut Game, ctx: Ctx, die_value: &Value) -> R<()> {
    let die_value = php::intval(die_value);
    let p = player_or_active(a, g, ctx)?;
    let source = get_source(g, ctx)?;
    let args = args_roll_die(a, g, ctx)?;
    let mut effects = vec![];

    if !php::in_array_strict(&json!(die_value), php::get(&args, "rolls")) {
        return Err(PhpError::new("You cannot select this die value. Should not happen"));
    }

    let counter_value = php::intval(php::get(&args, "counterValue"));
    let counter_spend = get_counter_spend_for_roll(a, g, ctx, die_value, php::get(&args, "rawRolls"), counter_value)?;

    if counter_spend > 0 {
        let c = source.card("getExtraDatas")?;
        let mut data = card::get(g, c, "extraDatas")?;
        let cur = php::intval(php::coalesce(php::get(&data, "counter"), &json!(0)));
        php::set(&mut data, "counter", json!((cur - counter_spend).max(0)));
        card::set_property(g, c, "extraDatas", data, true)?;
        g.notify("spendCounter", json!({"player_id": p, "cardId": c, "n": counter_spend}));
    }

    if php::count(php::get(&args, "rolls")) > 1 {
        g.notify("message", json!({"log": "Die ${dieValue} is selected", "dieValue": die_value}));
    }

    let effect = get_gain(a, g, ctx, die_value)?;
    if !effect.is_null() {
        let mut effect = utils::update_tree(&effect, &json!("die"), &json!(die_value), &[]);
        let sid = source.card("getId")?;
        php::set(&mut effect, "sourceId", json!(sid));
        let mut effect = utils::tag_tree(&effect, &json!({"sourceId": sid}), false);
        let card_id = get_ctx_arg(g, ctx, "cardId");
        if !card_id.is_null() && php::isset(php::get(&effect, "args"), "cardId") && php::loose_ne(php::get(php::get(&effect, "args"), "cardId"), &json!("me")) {
            php::set(php::get_mut(&mut effect, "args"), "cardId", card_id);
        }
        // Snowball commando's effect
        if php::isset(&effect, "pId") && php::loose_eq(php::get(&effect, "pId"), &json!("nextPlayer")) {
            let next = players::get_next(g, p)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
            php::set(&mut effect, "pId", json!(next));
        }
        effects.push(effect);
    }

    if effects.len() > 1 {
        insert_as_child(g, ctx, &json!({"type": "xor", "childs": effects}))?;
    } else if effects.len() == 1 {
        insert_as_child(g, ctx, &effects[0])?;
    }

    let dr = globals::get(g, "diceRolls")?;
    let sid = source.card("getId")?;
    check_after_listeners(a, g, ctx, p, &json!({"rolls": dr, "selectedRoll": die_value, "sourceId": sid}), true, None)?;

    globals::set(g, "diceRolls", json!([]))?;
    resolve_action(a, g, ctx, json!([die_value]), json!(false))
}

/// `actDiscardAdd($cardId)`: Bastion, a Reserve card discarded for +2 on the rolls
// PHP: Actions/RollDie.php:299
fn act_discard_add(a: &RollDie, g: &mut Game, ctx: Ctx, card_id: &Value) -> R<()> {
    let p = player_or_active(a, g, ctx)?;
    let _source = get_source(g, ctx)?;
    let args = args_roll_die(a, g, ctx)?;

    if !php::is_arr(card_id) {
        return Err(PhpError::new("TypeError: array_diff(): Argument #1 ($array) must be of type array"));
    }
    if php::count(&php::array_diff(card_id, php::get(&args, "cardIds"))) != 0 {
        return Err(PhpError::new("You cannot target this card. should not happen"));
    }
    duplicate_action(g, ctx, &json!({"canDiscard": false, "hasRolled": true}), false)?;
    insert_as_child(g, ctx, &ft::action("Discard", json!({"cardId": card_id}), json!({})))?;

    let rolls = globals::get(g, "diceRolls")?;
    let mut new_rolls = Value::Null;
    for roll in php::vals(&rolls) {
        php::push(&mut new_rolls, json!(php::intval(roll) + 2));
    }
    globals::set(g, "diceRolls", new_rolls)?;
    let c = cards::get(g, card_id)?;
    g.notify("message", json!({"log": "${player_name} discards ${card_name} to add 2 to its rolls.", "player_id": p, "cardId": c}));
    Ok(())
}
