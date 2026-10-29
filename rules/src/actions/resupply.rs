//! Actions/Resupply.php: the top n deck cards go to the Reserve (The Ouroboros, Lyra Bastion: draw
//! 2, keep 1), with the follow-ups of some sources, firing the Resupply listeners.

use crate::actions::draw_mana::ctx_source;
use crate::core::engine;
use crate::core::node;
use crate::framework::ST_RESUPPLY;
use crate::game::{Game, PId};
use crate::helpers::{conditions, ft};
use crate::managers::players;
use crate::models::action::{check_after_listeners, get_arg, get_event_recursive, get_source, get_source_id, insert_as_child, push_after_finishing_childs, resolve_action, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Resupply;

fn player_req(p: Option<PId>, method: &str) -> R<PId> {
    p.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

/// The TARGET node of the Ouroboros choice (`$destination` null: plain discard)
fn ouroboros_target(destination: Option<&str>, drawn: &[i64], source_id: &Value, pid: PId) -> Value {
    let discard_args = match destination {
        Some(d) => json!({"destination": d}),
        None => php::arr(),
    };
    ft::action(
        "Target",
        json!({
            "effect": ft::action("Discard", discard_args, php::arr()),
            "targetType": ["character", "token", "spell", "permanent"],
            "targetLocation": ["limbo"],
            "targetPlayer": "me",
            "cards": drawn,
        }),
        json!({"sourceId": source_id, "pId": pid}),
    )
}

impl ActionClass for Resupply {
    fn name(&self) -> &'static str {
        "Resupply"
    }

    // PHP: Actions/Resupply.php:71
    fn default_args(&self) -> Value {
        json!({
            "n": 1,
            "exhausted": false,
            "character2Less": false,
            "characterHand": false,
            "boostIfMatchCondition": null,
            "ownerId": null,
            "player": null,
        })
    }

    // PHP: Actions/Resupply.php:18
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_RESUPPLY))
    }

    // PHP: Actions/Resupply.php:23
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let p = self.get_player(g, ctx)?;
        let exhausted = get_arg(self, g, ctx, "exhausted")?;
        let p = player_req(p, "getResupply2")?;
        if !php::truthy(&exhausted) {
            if player::get_resupply2(g, p)? {
                Ok(json!("Lyra Bastion's resupply"))
            } else {
                Ok(json!({"log": "Resupply ${n}", "args": {"n": get_arg(self, g, ctx, "n")?}}))
            }
        } else if player::get_resupply2(g, p)? {
            Ok(json!("Lyra Bastion's resupply (exhausted)"))
        } else {
            Ok(json!({"log": "Exhausted resupply ${n}", "args": {"n": get_arg(self, g, ctx, "n")?}}))
        }
    }

    // PHP: Actions/Resupply.php:53
    fn is_automatic(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getInfos() on null"))?;
        let auto = node::info(g, n, "automatic");
        if !auto.is_null() {
            return Ok(php::truthy(&auto));
        }
        Ok(!self.is_optional(g, ctx, player)?)
    }

    // PHP: Actions/Resupply.php:61
    fn is_irreversible(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Resupply.php:66
    fn is_doable(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    // PHP: Actions/Resupply.php:81
    fn get_player(&self, g: &mut Game, ctx: Ctx) -> R<Option<PId>> {
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getPId() on null"))?;
        let mut pid = node::get_pid(g, n)?;
        let arg_player = get_arg(self, g, ctx, "player")?;
        if php::loose_eq(&arg_player, &json!("owner")) || php::loose_eq(&pid, &json!("owner")) {
            pid = get_arg(self, g, ctx, "ownerId")?;
        } else if php::loose_eq(&pid, &json!("active")) {
            pid = json!(players::get_active_id(g));
        } else if php::loose_eq(&arg_player, &json!("nextPlayer")) {
            let active = player_req(players::get_active(g), "getId")?;
            pid = json!(players::get_next_id(g, active)?);
        } else if pid.is_null() {
            // Prefer the leave-event controller only when no node pId was set
            let event = get_event_recursive(g, ctx)?;
            if php::isset(&event, "pId") && php::loose_eq(php::coalesce(php::get(&event, "cardId"), &json!(-1)), &get_source_id(g, ctx)?) {
                pid = php::get(&event, "pId").clone();
            } else {
                pid = match get_source(g, ctx)? {
                    None => json!(players::get_active_id(g)),
                    Some(c) => json!(card::pid(g, c)?),
                };
            }
        }
        Ok(players::get_by_value(g, &pid))
    }

    // PHP: Actions/Resupply.php:108
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let mut n = get_arg(self, g, ctx, "n")?;
        let p = self.get_player(g, ctx)?;
        let mut not_resupply = false;
        let exhausted = get_arg(self, g, ctx, "exhausted")?;
        let mut checkpoint = true;
        let source = ctx_source(g, ctx)?;
        let source_id = match ctx {
            Some(nd) => node::get_source_id(g, nd),
            None => Value::Null,
        };
        let mut nodes: Vec<Value> = vec![];
        let p = player_req(p, "getResupply2")?;
        let cards: Vec<i64>;

        // manage of AX_Rare_TheOuroborosLyraBastion
        if player::get_resupply2(g, p)? {
            engine::checkpoint(g)?;
            not_resupply = true;
            // draw 2, 1 goes to reserve, the other one is discarded
            let drawn = player::draw(g, p, 2, Some(&format!("deck-{}", p)), Some("limbo"), source, false, None)?;
            if drawn.len() == 2 {
                let flow = ft::seq(vec![ouroboros_target(Some("reserve"), &drawn, &source_id, p), ouroboros_target(None, &drawn, &source_id, p)]);
                insert_as_child(g, ctx, &flow)?;
                cards = drawn;
                checkpoint = false;
            } else if drawn.len() == 1 {
                let flow = ft::xor(vec![ouroboros_target(Some("reserve"), &drawn, &source_id, p), ouroboros_target(None, &drawn, &source_id, p)]);
                insert_as_child(g, ctx, &flow)?;
                cards = drawn;
                checkpoint = false;
            } else {
                cards = drawn;
            }
        } else {
            cards = player::draw(g, p, php::intval(&n), Some(&format!("deck-{}", p)), Some("reserve"), source, php::truthy(&exhausted), Some(p))?;
            if php::truthy(&get_arg(self, g, ctx, "character2Less")?) {
                for c in &cards {
                    if php::loose_eq(&card::get(g, *c, "type")?, &json!("character")) {
                        nodes.push(ft::seq_optional(vec![
                            json!({
                                "action": "SpecialEffect",
                                "args": {"effect": "costReduction", "args": {"type": "all", "reduction": 2}},
                                "sourceId": source_id,
                                "pId": p,
                            }),
                            ft::action("PlayCard", json!({"cardId": c}), json!({"sourceId": source_id, "pId": p})),
                        ]));
                    }
                    // `foreach ($node as &$n)`: sets pId, and leaves `$n` (the draw count used by
                    // the listeners below) a reference to the last node
                    if !nodes.is_empty() {
                        for nd in nodes.iter_mut() {
                            php::set(nd, "pId", json!(p));
                        }
                        n = nodes.last().unwrap().clone();
                    }
                }
            }
            if php::truthy(&get_arg(self, g, ctx, "characterHand")?) {
                for c in &cards {
                    if php::loose_eq(&card::get(g, *c, "type")?, &json!("character")) {
                        nodes.push(ft::action("Discard", json!({"cardId": c, "destination": "hand"}), json!({"optional": true, "pId": p})));
                    }
                }
            }
            let bimc = get_arg(self, g, ctx, "boostIfMatchCondition")?;
            if php::loose_ne(&bimc, &Value::Null) {
                // Extract number of boost and condition
                let s = php::strval(&bimc);
                let mut parts = s.splitn(2, ':');
                let boost_n = parts.next().unwrap_or("").to_string();
                let condition = parts.next().map(|x| json!(x)).unwrap_or(Value::Null);
                for c in &cards {
                    let matched = conditions::check(g, &json!({"condition": condition}), Some(*c), &Value::Null)?;
                    if matched {
                        nodes.push(ft::gain(json!(c), "boost", json!(boost_n), json!(99)));
                    }
                }
            }
        }

        // Machine in the ice effect
        for c in &cards {
            if card::is(g, *c, "resupplyExhaust")? {
                nodes.push(ft::action("Exhaust", json!({"cardId": c}), json!({"sourceId": c})));
            }
        }
        if !nodes.is_empty() {
            push_after_finishing_childs(g, &Value::Array(nodes))?;
        }
        if !cards.is_empty() {
            let sid = get_source_id(g, ctx)?;
            check_after_listeners(self, g, ctx, p, &json!({"draw": n, "sourceId": sid, "notResupply": not_resupply, "exhausted": exhausted}), true, None)?;
        }
        resolve_action(self, g, ctx, Value::Null, json!(checkpoint))?;
        Ok(None)
    }
}
