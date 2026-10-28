//! SpecialEffect actions: named effects of the original engine (`Actions/SpecialEffect.php`),
//! one arm per effect name. Args: `effect` (name) and `args` (its parameters).

use crate::actions::{b, card_arg, i, list, s};
use crate::game::*;
use crate::model::*;
use serde_json::{Map, Value};

#[allow(unused_variables)]
pub fn special(g: &mut Game, a: &Map<String, Value>, _node: &Value, ctx: Ctx) {
    let effect = s(a, "effect").unwrap_or("");
    let inner = a.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
    let seat = ctx.seat;
    match effect {
        "costReduction" => {
            let kind = s(&inner, "type").unwrap_or("all").to_string();
            let r = i(&inner, "reduction", 1);
            g.mods.cost_reductions.push((seat, kind, r));
        }
        "nextCharacterGains1Boost" => g.mods.next_character_boost[seat] += i(&inner, "n", 1),
        "nextReserveCharacterGains1Boost" => g.mods.next_reserve_character_boost[seat] += 1,
        "nextAnimalGains1Boost" => g.mods.next_animal_boost[seat] += 1,
        "nextCharacterFleeting" => g.mods.next_character_fleeting[seat] = true,
        "nextTokenAnchored" => g.mods.next_token_anchored[seat] = true,
        "nextCharacterCost3Anchored" => g.mods.next_character_anchored[seat] = true,
        "incCounter" | "gainCounter" => {
            if let Some(c) = card_arg(g, a, &ctx).or(ctx.source) {
                g.card_mut(c).counter += i(&inner, "counter", 1).max(1);
            }
        }
        "boostAllSubtype" => {
            let sub = s(&inner, "subType").or(s(&inner, "subtype")).unwrap_or("").to_string();
            for c in g.in_play(seat) {
                if g.card(c).def().has_subtype(&sub) && g.card(c).is_character_like() {
                    g.card_mut(c).boosts += 1;
                }
            }
        }
        "allCharacterFleeting" => {
            for c in g.in_play(seat) {
                if g.card(c).is_character_like() {
                    g.card_mut(c).add_status(Status::Fleeting);
                }
            }
        }
        "exhaustAllCards" => {
            for c in g.in_play(seat) {
                g.card_mut(c).exhausted = true;
            }
        }
        other => {
            g.unsupported(format!("special {}", other));
            let name = ctx.source.map(|c| g.card(c).name()).unwrap_or("?");
            g.log_line(None, format!("(Not implemented yet: [[{}]] effect \"{}\")", name, other));
        }
    }
}
