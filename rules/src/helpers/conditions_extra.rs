//! Helpers/Conditions.php: the predicates that actions call directly with their own signatures
//! (not through the `"name:arg"` dispatcher of `conditions.rs`)

use crate::core::globals;
use crate::game::{CardId, Game, PId};
use crate::models::card;
use crate::php;
use crate::R;
use serde_json::{json, Value};

/// `getSmokeThemOutCharges($pId, $cardId)`: charges per player then per card (legacy saves: one
/// player-wide count)
// PHP: Helpers/Conditions.php:1362
pub fn get_smoke_them_out_charges_pid(g: &mut Game, pid: PId, card_id: Option<CardId>) -> R<i64> {
    let armed = globals::get(g, "smokeThemOutArmed")?;
    let val = php::coalesce(php::get(&armed, &pid.to_string()), &json!(0)).clone();
    if php::is_arr(&val) {
        return Ok(match card_id {
            None => php::vals(&val).into_iter().map(php::intval).sum(),
            Some(c) => php::intval(php::get(&val, &c.to_string())),
        });
    }
    Ok(php::intval(&val))
}

/// `armSmokeThemOut($pId, $cardId)`
// PHP: Helpers/Conditions.php:1379
pub fn arm_smoke_them_out_pid(g: &mut Game, pid: PId, card_id: CardId) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    let cur = php::get(&armed, &pid.to_string()).clone();
    let mut charges = if php::is_arr(&cur) { cur } else { php::arr() };
    let k = card_id.to_string();
    let n = php::intval(php::coalesce(php::get(&charges, &k), &json!(0))) + 1;
    php::set(&mut charges, &k, json!(n));
    php::set(&mut armed, &pid.to_string(), charges);
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `consumeSmokeThemOutCharge($pId, $cardId)`
// PHP: Helpers/Conditions.php:1388
pub fn consume_smoke_them_out_charge_pid(g: &mut Game, pid: PId, card_id: Option<CardId>) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    let pk = pid.to_string();
    let mut charges = php::get(&armed, &pk).clone();
    if !php::is_arr(&charges) {
        let remaining = get_smoke_them_out_charges_pid(g, pid, card_id)? - 1;
        if remaining < 0 {
            return Ok(());
        }
        if remaining == 0 {
            php::unset(&mut armed, &pk);
        } else {
            php::set(&mut armed, &pk, json!(remaining));
        }
        globals::set(g, "smokeThemOutArmed", armed)?;
        return Ok(());
    }
    let Some(c) = card_id else { return Ok(()) };
    let ck = c.to_string();
    if php::intval(php::coalesce(php::get(&charges, &ck), &json!(0))) <= 0 {
        return Ok(());
    }
    let n = php::intval(php::get(&charges, &ck)) - 1;
    php::set(&mut charges, &ck, json!(n));
    if n <= 0 {
        php::unset(&mut charges, &ck);
    }
    if php::empty(&charges) {
        php::unset(&mut armed, &pk);
    } else {
        php::set(&mut armed, &pk, charges);
    }
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `isCardMatchingSearch($card, $name)`: the uid matches `/_<name>(_|$)/`
// PHP: Helpers/Conditions.php:738
pub fn is_card_matching_search_name(g: &mut Game, card_id: CardId, name: &Value) -> R<bool> {
    if name.is_null() || name == &json!("") {
        return Ok(false);
    }
    let uid = card::get_str(g, card_id, "uid")?;
    if uid.is_empty() {
        return Ok(false);
    }
    let needle = format!("_{}", php::strval(name));
    let mut from = 0;
    while let Some(i) = uid[from..].find(&needle) {
        let end = from + i + needle.len();
        if end == uid.len() || uid.as_bytes()[end] == b'_' {
            return Ok(true);
        }
        from += i + 1;
    }
    Ok(false)
}
