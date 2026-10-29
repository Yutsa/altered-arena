//! Actions/NightCleanup.php: the player chooses the Reserve and Landmark cards to discard at night;
//! the Discard reactions are stored in `afterNightCleanup`.

use crate::core::globals;
use crate::framework::ST_NIGHT_CLEANUP;
use crate::game::{CardId, Game, PId};
use crate::helpers::collection;
use crate::managers::cards;
use crate::models::action::{get_arg, log_reactions, param_req, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct NightCleanup;

fn player_req(p: Option<PId>, method: &str) -> R<PId> {
    p.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// `argsNightCleanup()`
// PHP: Actions/NightCleanup.php:34
fn args_night_cleanup(a: &NightCleanup, g: &mut Game, ctx: Ctx) -> R<Value> {
    let p = player_req(a.get_player(g, ctx)?, "getReserveCards")?;
    let mut reserve: Vec<CardId> = vec![];
    for c in player::get_reserve_cards(g, p)? {
        if !card::is(g, c, "ignoreReserveLimit")? || card::is_tapped(g, c)? {
            reserve.push(c);
        }
    }
    let landmarks = player::get_landmarks(g, p)?;
    let exhausted_slots = player::get_exhausted_reserve_slots(g, p)?;
    let n_reserve = get_arg(a, g, ctx, "nReserve")?;
    let n_landmarks = get_arg(a, g, ctx, "nLandmarks")?;
    let suffix = if php::loose_eq(&n_reserve, &json!(0)) {
        "landmarksOnly"
    } else if php::loose_eq(&n_landmarks, &json!(0)) {
        "reserveOnly"
    } else {
        ""
    };
    Ok(json!({
        "nReserve": n_reserve,
        "nLandmarks": n_landmarks,
        "exhaustedSlots": exhausted_slots,
        "descSuffix": suffix,
        "_private": {
            "active": {
                "reserveCards": collection::ids(&reserve),
                "landmarkCards": collection::ids(&landmarks),
                "cards": collection::ids(&collection::merge(&reserve, &landmarks)),
            },
        },
    }))
}

/// `actNightCleanup($reserveCardIds, $landmarkCardIds)`
// PHP: Actions/NightCleanup.php:61
fn act_night_cleanup(a: &NightCleanup, g: &mut Game, ctx: Ctx, reserve_ids: &Value, landmark_ids: &Value) -> R<Value> {
    let p = a.get_player(g, ctx)?;
    let args = args_night_cleanup(a, g, ctx)?;
    let pargs = php::get(php::get(&args, "_private"), "active").clone();

    // Number of cards
    if php::loose_ne(&json!(php::count_strict(reserve_ids)?), php::get(&args, "nReserve")) || php::loose_ne(&json!(php::count_strict(landmark_ids)?), php::get(&args, "nLandmarks")) {
        return Err(PhpError::new("You must select the correct number of cards. Should not happen"));
    }
    // Valid card ids
    if !php::empty(&php::array_diff(reserve_ids, php::get(&pargs, "reserveCards"))) || !php::empty(&php::array_diff(landmark_ids, php::get(&pargs, "landmarkCards"))) {
        return Err(PhpError::new("You selected a card that should not be discarded. Should not happen"));
    }

    let card_ids = php::array_merge(&[reserve_ids, landmark_ids]);
    let cs = cards::get_many(g, &card_ids, true)?;
    let mut deleted_meeples: Vec<i64> = vec![];
    let original_destination = "discard";
    let mut destination = original_destination.to_string();

    // Alizé, some cards can be kept if they are exhausted
    if php::ge(php::get(&args, "exhaustedSlots"), &json!(0)) && php::gt(&json!(php::count(reserve_ids)), php::get(&args, "nReserve")) {
        let mut non_exhausted = 0;
        for c in &cs {
            if !card::is_tapped(g, *c)? {
                non_exhausted += 1;
            }
        }
        let limit = php::intval(php::get(&args, "nReserve")) - php::intval(php::get(&args, "exhaustedSlots"));
        if non_exhausted > limit {
            return Err(PhpError::new("You selected too many ready cards to keep"));
        }
    }

    for c in &cs {
        let c = *c;
        // Save information about original location
        destination = original_destination.to_string();
        let original_location = card::location(g, c)?;
        let new_id = c;
        let mut sacrifice = false;
        let is_token = card::is_token(g, c)?;
        if original_location == "landmark" {
            card::check_leave_listener(g, c, &destination, true, true, &php::arr())?;
            if card::is_token(g, c)? {
                destination = "destroy".into();
            }
            sacrifice = true;
        } else if ["reserve", "stormLeft", "stormRight"].contains(&original_location.as_str()) {
            card::check_leave_listener(g, c, &destination, true, false, &php::arr())?;
        }

        // Discard the card (discardTo checks the leave listener again)
        let m = card::discard_to(g, c, &destination, &php::arr(), true, &php::arr())?;
        deleted_meeples.extend(m);

        let mut after_cleanup = globals::get(g, "afterNightCleanup")?;

        // Check listener
        let pp = player_req(p, "getId")?;
        let reactions = log_reactions(
            a,
            g,
            "Discard",
            pp,
            &json!({
                "discardCard": true,
                "cardsToListen": card_ids,
                "from": original_location,
                "to": destination,
                "sacrifice": sacrifice,
                "cardId": new_id,
                "token": is_token,
            }),
            &json!(true),
        )?;
        if !reactions.is_null() {
            for r in php::vals(&reactions) {
                if php::loose_eq(php::get(php::get(r, "args"), "cardId"), &json!(c)) {
                    // Effect has already been processed via the "check leave listener"
                    continue;
                }
                let k = php::key_of(php::get(r, "pId"));
                php::push(php::get_mut(&mut after_cleanup, &k), r.clone());
            }
        }
        globals::set(g, "afterNightCleanup", after_cleanup)?;
    }

    if !deleted_meeples.is_empty() {
        g.notify("silentKill", json!({"ids": deleted_meeples}));
    }
    g.notify(
        "publicDiscard",
        json!({"player_id": p, "cardIds": cs, "nReserve": php::count(reserve_ids), "nLandmarks": php::count(landmark_ids), "destination": destination}),
    );
    Ok(Value::Null)
}

impl ActionClass for NightCleanup {
    fn name(&self) -> &'static str {
        "NightCleanup"
    }

    // PHP: Actions/NightCleanup.php:29
    fn default_args(&self) -> Value {
        json!({"nReserve": 0, "nLandmarks": 0})
    }

    // PHP: Actions/NightCleanup.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_NIGHT_CLEANUP))
    }

    // PHP: Actions/NightCleanup.php:24
    fn get_description(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!("Night cleanup"))
    }

    // PHP: Actions/NightCleanup.php:34
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_night_cleanup(self, g, ctx)?))
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "actnightcleanup" => {
                let r = param_req(args, 0, "reserveCardIds", "actNightCleanup")?;
                let l = param_req(args, 1, "landmarkCardIds", "actNightCleanup")?;
                act_night_cleanup(self, g, ctx, &r, &l)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\NightCleanup::{}()", method))),
        }
    }
}
