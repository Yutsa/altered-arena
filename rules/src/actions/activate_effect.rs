//! Actions/ActivateEffect.php: re-activates the `{J}` / `{R}` / `{D}` effect (`effectType`
//! Played / Reserve / Support) of one or several cards, pushed as a parallel child (an OR with
//! `n`, a XOR, or a single node); `{D}` also records the ability activation and fires the
//! ChooseAssignment listeners.
//!
//! PHP defect kept (PHP_ENGINE.md §8.1): with one resulting node, `$node` (the last raw tagged
//! effect, a whole PARALLEL when its childs were spread) is pushed.

use super::tap::record_ability_activated;
use crate::core::node;
use crate::framework::*;
use crate::game::{CardId, Game, PId};
use crate::helpers::{ft, utils};
use crate::managers::{cards, players};
use crate::models::action::{self, get_arg, get_ctx_args, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct ActivateEffect;

/// `resolveTargetPickCardId()`: the first pick of the closest resolved parent Target
// PHP: Actions/ActivateEffect.php:138
fn resolve_target_pick_card_id(g: &Game, ctx: Ctx) -> R<Value> {
    let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getParent() on null"))?;
    let mut cur = node::parent(g, n);
    while let Some(c) = cur {
        if php::strict_eq(&node::get_action(g, c), &json!("Target")) && node::is_action_resolved(g, c) {
            let resolved = node::get_action_resolution_args(g, c);
            if php::is_arr(&resolved) && !php::empty(&resolved) {
                let pick = php::geti(&resolved, 0);
                let card_id = if php::is_arr(pick) { php::geti(pick, 0).clone() } else { pick.clone() };
                if !card_id.is_null() {
                    return Ok(card_id);
                }
            }
        }
        cur = node::parent(g, c);
    }
    Ok(Value::Null)
}

/// `resolveActivateCardId($cardId)`: explicit id, listener event (EFFECT), parent Target pick,
/// or the source when `cardId` is omitted
// PHP: Actions/ActivateEffect.php:102
fn resolve_activate_card_id(g: &mut Game, ctx: Ctx, card_id: &Value) -> R<Value> {
    if php::loose_eq(card_id, &json!("me")) {
        return Ok(match action::get_source(g, ctx)? {
            Some(s) => json!(s),
            None => Value::Null,
        });
    }
    if !card_id.is_null() && php::loose_ne(card_id, &json!("EFFECT")) {
        return Ok(card_id.clone());
    }
    if php::loose_eq(card_id, &json!("EFFECT")) {
        let event = action::get_event_recursive(g, ctx)?;
        if !event.is_null() {
            let resolved = php::get(&event, "cardId").clone();
            if !resolved.is_null() {
                return Ok(resolved);
            }
        }
    }
    let from_target = resolve_target_pick_card_id(g, ctx)?;
    if !from_target.is_null() {
        return Ok(from_target);
    }
    if card_id.is_null() {
        return Ok(match action::get_source(g, ctx)? {
            Some(s) => json!(s),
            None => Value::Null,
        });
    }
    Ok(Value::Null)
}

/// `getCard()`: a card, or a Collection when `cardId` lists several cards
// PHP: Actions/ActivateEffect.php:73
fn get_card(a: &ActivateEffect, g: &mut Game, ctx: Ctx) -> R<cards::Got> {
    let args = get_ctx_args(g, ctx);
    let card_id = php::get(&args, "cardId").clone();
    if php::truthy(&get_arg(a, g, ctx, "ownEffect")?) && php::strict_eq(&get_arg(a, g, ctx, "effectType")?, &json!("Support")) {
        if !card_id.is_null() && php::loose_ne(&card_id, &json!("me")) && php::loose_ne(&card_id, &json!("EFFECT")) {
            return cards::get_raw(g, &card_id, true);
        }
        let owner_id = action::get_source_id(g, ctx)?;
        if !owner_id.is_null() {
            return cards::get_raw(g, &owner_id, true);
        }
    }

    let card_id = resolve_activate_card_id(g, ctx, &card_id)?;

    if card_id.is_null() {
        return Err(PhpError::new("no card in args (Activate effect). Should not happen"));
    }
    cards::get_raw(g, &card_id, true)
}

/// `clienttranslate('activate {J|R|D} effect')` by effectType (null for another type)
fn short_description(effect_type: &Value) -> Value {
    if php::loose_eq(effect_type, &json!("Played")) {
        json!("activate {J} effect")
    } else if php::loose_eq(effect_type, &json!("Reserve")) {
        json!("activate {R} effect")
    } else if php::loose_eq(effect_type, &json!("Support")) {
        json!("activate {D} effect")
    } else {
        Value::Null
    }
}

impl ActionClass for ActivateEffect {
    fn name(&self) -> &'static str {
        "ActivateEffect"
    }

    fn default_args(&self) -> Value {
        json!({"effectType": "Played", "n": 1000, "ownEffect": false})
    }

    // PHP: Actions/ActivateEffect.php:16
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_ACTIVATE_EFFECT))
    }

    // PHP: Actions/ActivateEffect.php:21
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let ctx_card_id = php::get(&get_ctx_args(g, ctx), "cardId").clone();
        // cardId EFFECT is unresolved until a parent Target picks (e.g. output 880 under 910).
        if ctx_card_id.is_null() || php::strict_eq(&ctx_card_id, &json!("EFFECT")) {
            return Ok(short_description(&get_arg(self, g, ctx, "effectType")?));
        }
        let got = get_card(self, g, ctx)?;
        let effect_type = get_arg(self, g, ctx, "effectType")?;
        if let cards::Got::Many(_) = got {
            let d = short_description(&effect_type);
            if !d.is_null() {
                return Ok(d);
            }
        }
        let log = match short_description(&effect_type) {
            Value::String(s) => format!("{} of ${{card_name}}", s),
            _ => return Ok(Value::Null),
        };
        let c = match get_card(self, g, ctx)? {
            cards::Got::One(c) => c,
            cards::Got::Many(_) => return Err(PhpError::new("Error: Call to undefined method ALT\\Helpers\\Collection::getName()")),
        };
        let name = card::get(g, c, "name")?;
        Ok(json!({"log": log, "args": {"card_name": name, "i18n": ["card_name"]}}))
    }

    // PHP: Actions/ActivateEffect.php:63
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/ActivateEffect.php:68
    fn is_independent(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/ActivateEffect.php:164
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let list: Vec<CardId> = match get_card(self, g, ctx)? {
            cards::Got::One(c) => vec![c],
            cards::Got::Many(v) => v,
        };
        let mut nodes: Vec<Value> = vec![];
        let mut last_node: Option<Value> = None;

        for c in list {
            let ty = card::get(g, c, "type")?;
            let mut ok = false;
            if php::loose_eq(&ty, &json!("character")) {
                let p = card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
                let loc = card::location(g, c)?;
                let gig = php::truthy(&card::is_gigantic(g, c, false)?);
                ok = !players::has_opponent_blocking_power(g, p, &loc, gig)?;
            }
            let ok = ok || php::loose_ne(&card::get(g, c, "type")?, &json!("character")) || !php::in_array(&json!("character"), &card::get_additional_type(g, c)?);
            if ok {
                let effect_type = get_arg(self, g, ctx, "effectType")?;
                let prop = format!("effect{}", php::strval(&effect_type));
                if short_description(&effect_type).is_null() {
                    return Err(PhpError::new(format!("Unknown effectType in ActivateEffect: {}", php::strval(&effect_type))));
                }

                if !php::empty(&card::get(g, c, &prop)?) {
                    let active = players::get_active(g);
                    g.notify("mediumMessage", json!({"log": format!("${{player_name}} activates ${{card_name}} {} effect", prop), "player_id": active, "cardId": c}));
                    let raw = card::get(g, c, &prop)?;
                    // ownEffect: nested effects run as the source card (Thomas Edison), not the target.
                    let tag_source_id = if php::truthy(&get_arg(self, g, ctx, "ownEffect")?) {
                        let sid = action::get_source_id(g, ctx)?;
                        if sid.is_null() {
                            json!(c)
                        } else {
                            sid
                        }
                    } else {
                        json!(c)
                    };
                    let tagged = utils::tag_tree(&raw, &json!({"sourceId": tag_source_id}), false);
                    let tagged = utils::tag_tree(&tagged, &json!({"pId": card::pid(g, c)?}), false);
                    if !php::isset(&tagged, "action") && php::loose_eq(php::coalesce(php::get(&tagged, "type"), &json!("parallel")), &json!("parallel")) {
                        for n in php::vals(php::get(&tagged, "childs")) {
                            nodes.push(n.clone());
                        }
                    } else {
                        nodes.push(tagged.clone());
                    }
                    last_node = Some(tagged);

                    // Some cards (e.g. Yeong-Gi & Ember) activate a {D} ability without going through
                    // ChooseAssignment::actSupport. Mirror the same "discard ability activated" accounting
                    // and listener event so cards tracking {D} activations can react.
                    if php::loose_eq(&get_arg(self, g, ctx, "effectType")?, &json!("Support")) {
                        let ap = players::get_active(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
                        record_ability_activated(g, ap, "discard")?;
                        action::check_after_listeners(
                            self,
                            g,
                            ctx,
                            ap,
                            &json!({"cardId": c, "playCard": false, "isSupport": true, "sourceId": c, "pId": ap}),
                            true,
                            Some("ChooseAssignment"),
                        )?;
                    }
                }
            } else {
                g.notify("mediumMessage", json!({"log": "Effects are not triggered, due to an effect in the opponent's expedition"}));
            }
        }

        if !nodes.is_empty() {
            let n = get_arg(self, g, ctx, "n")?;
            let flow = if nodes.len() > 1 && php::gt(&n, &json!(1)) {
                json!({"type": "or", "optional": true, "args": {"n": n}, "childs": nodes})
            } else if php::loose_eq(&n, &json!(1)) && nodes.len() > 1 {
                // only 1 potential choice
                ft::xor(nodes)
            } else {
                // only 1 node
                last_node.unwrap_or(Value::Null)
            };
            action::push_parallel_child(g, ctx, &flow)?;
        }

        action::resolve_action(self, g, ctx, php::arr(), Value::Null)?;
        Ok(None)
    }
}
