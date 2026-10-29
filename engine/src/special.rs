//! SpecialEffect actions: named effects of the original engine (`Actions/SpecialEffect.php`),
//! one arm per effect name. Args: `effect` (name), `args` (its parameters), and sometimes
//! `expedition` next to them. Most effects build a small flow tree from existing actions
//! (Gain, Discard, Target, InvokeToken...) and queue it, as the original engine inserts child
//! nodes. Effects that last ("the next Character you play...") keep their state in
//! `SpecialMods` (`game.mods.special`), read by the hooks at the end of this file.

use crate::actions::{b, card_arg, i, list, move_with_events, s, zone_key};
use crate::catalog::CardType;
use crate::flow::source_prefix;
use crate::game::*;
use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// State of the special effects that outlast their resolution
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpecialMods {
    /// globalTough until the next Morning: (seat whose cards are tough, card type, minimum hand cost, tough)
    pub global_tough: Vec<(Seat, String, i32, i32)>,
    /// Effects to resolve after Rest (BeforeNight), per player
    pub after_rest: [Vec<(Value, Ctx)>; 2],
    /// Cost reductions that last until used, instead of the end of the turn (costReduction "permanent")
    pub permanent_reductions: Vec<(Seat, String)>,
    pub next_spell_free: [bool; 2],
    /// Next card played that loses Fleeting: "spell", "spellHand", "character", "characterStat0", "songArtist"
    pub remove_fleeting: [Vec<String>; 2],
    pub next_character_asleep: [bool; 2],
    pub next_character_cost3_anchored: [bool; 2],
    pub next_character_base_cost3_anchored: [bool; 2],
    /// Next Character played in a Forest region gains these boosts (nextCharacterVGainsBoost)
    pub next_character_forest_boost: [i32; 2],
    pub next_token_asleep: [bool; 2],
    /// Next Character joining an expedition gains these boosts: [seat][side]
    pub expedition_boost: [[i32; 2]; 2],
    /// Abilities the next Character played activates (triggerEffectOfNextCharacter)
    pub next_effects: Vec<NextEffect>,
    /// Cards whose "once per Afternoon" ability was used (useCard)
    pub used: Vec<CardId>,
    /// Top card of a player's deck revealed by revealTop
    pub revealed: [Option<CardId>; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextEffect {
    pub seat: Seat,
    /// Card type of the next card ("character")
    pub kind: String,
    /// Zone it is played from ("hand", "reserve")
    pub from: String,
    /// Ability to activate ("reserve" → effectReserve)
    pub effect: String,
    /// "sourceLocation" or a zone: where it must be played
    pub to: Option<String>,
    pub source: CardId,
    pub limit1: bool,
    pub boost: i32,
}

pub fn special(g: &mut Game, a: &Map<String, Value>, _node: &Value, ctx: Ctx) {
    let effect = s(a, "effect").unwrap_or("").to_string();
    let inner = a.get("args").and_then(Value::as_object).cloned().unwrap_or_default();
    let src = ctx.source;
    let me = controller(g, &ctx);
    match effect.as_str() {
        // ------------------------------------------------------------------
        // Modifiers of the next cards played
        // ------------------------------------------------------------------
        "costReduction" => {
            let kind = s(&inner, "type").unwrap_or("all").to_string();
            let r = i(&inner, "reduction", 1);
            g.mods.cost_reductions.push((me, kind.clone(), r));
            if b(&inner, "permanent") {
                g.mods.special.permanent_reductions.push((me, kind));
            }
        }
        "nextCharacterGains1Boost" => g.mods.next_character_boost[me] += 1,
        "nextCharacterGains2Boost" => g.mods.next_character_boost[me] += 2,
        "nextCharacterGains1BoostAndAsleep" => {
            g.mods.next_character_boost[me] += 1;
            g.mods.special.next_character_asleep[me] = true;
        }
        "nextCharacterAsleep" => g.mods.special.next_character_asleep[me] = true,
        "nextReserveCharacterGains1Boost" => g.mods.next_reserve_character_boost[me] += 1,
        "nextAnimalGains1Boost" => g.mods.next_animal_boost[me] += 1,
        "nextCharacterVGainsBoost" => g.mods.special.next_character_forest_boost[me] += 1,
        "nextCharacterFleeting" => g.mods.next_character_fleeting[me] = true,
        "nextCharacterAnchored" => g.mods.next_character_anchored[me] = true,
        "nextCharacterCost3Anchored" => g.mods.special.next_character_cost3_anchored[me] = true,
        "nextCharacterBaseCost3Anchored" => g.mods.special.next_character_base_cost3_anchored[me] = true,
        "nextTokenAnchored" => g.mods.next_token_anchored[me] = true,
        "nextTokenAsleep" => g.mods.special.next_token_asleep[me] = true,
        "nextSpellIsFree" => {
            g.mods.special.next_spell_free[me] = true;
            g.log_line(Some(me), format!("The next Spell played this turn is free ({}effect)", source_prefix(g, src)));
        }
        "removeFleetingSpellPlayed" => g.mods.special.remove_fleeting[me].push("spell".into()),
        "removeFleetingIfSpellPlayedHand" => g.mods.special.remove_fleeting[me].push("spellHand".into()),
        "removeFleetingCharacterPlayed" => g.mods.special.remove_fleeting[me].push("character".into()),
        "removeFleetingCharacterStat0Played" => g.mods.special.remove_fleeting[me].push("characterStat0".into()),
        "removeFleetingSongArtistPlayed" => g.mods.special.remove_fleeting[me].push("songArtist".into()),
        "nextCharactInExpeditionBoost" => {
            // X = paid mana is not tracked: the fixed n is used
            if let Some(side) = src.and_then(|c| g.card(c).zone.side()) {
                g.mods.special.expedition_boost[me][side.index()] += i(&inner, "n", 1);
            }
        }
        "triggerEffectOfNextCharacter" => {
            let Some(source) = src else { return };
            let e = NextEffect {
                seat: me,
                kind: s(&inner, "type").unwrap_or("character").into(),
                from: s(&inner, "from").unwrap_or("hand").into(),
                effect: s(&inner, "effect").unwrap_or("reserve").into(),
                to: s(&inner, "to").map(String::from),
                source,
                limit1: i(&inner, "limit", 99) == 1,
                boost: i(&inner, "boost", 0),
            };
            if !g.mods.special.next_effects.contains(&e) {
                g.mods.special.next_effects.push(e);
            }
        }
        "globalTough" => {
            let kind = s(&inner, "type").unwrap_or("character").to_string();
            g.mods.special.global_tough.push((me, kind, i(&inner, "minHandCost", 0), i(&inner, "tough", 1)));
        }
        "useCard" => {
            if let Some(c) = src {
                if !g.mods.special.used.contains(&c) {
                    g.mods.special.used.push(c);
                }
            }
        }
        "unuseCard" => {
            if let Some(c) = src {
                g.mods.special.used.retain(|&x| x != c);
            }
        }
        "armSmokeThemOut" => {
            if let Some(c) = src {
                g.players[me].records.smoke_armed.push(c);
            }
        }
        "disarmSmokeThemOut" => {
            if let Some(c) = src {
                let armed = &mut g.players[me].records.smoke_armed;
                if let Some(k) = armed.iter().position(|&x| x == c) {
                    armed.remove(k);
                }
            }
        }

        // ------------------------------------------------------------------
        // Counters
        // ------------------------------------------------------------------
        "incCounter" | "gainCounter" => {
            let Some(c) = src else { return };
            if g.card(c).zone == Zone::Hand {
                return;
            }
            let mut n = i(&inner, "counter", 1);
            let max = i(&inner, "maxCounter", 99);
            if g.card(c).counter + n > max {
                n = max - g.card(c).counter;
            }
            gain_counter(g, c, n);
        }
        "counterPerCharacter" => {
            let n = characters(g, me).len() as i32;
            if let Some(c) = src {
                gain_counter(g, c, n);
            }
        }

        // ------------------------------------------------------------------
        // "I gain 1 boost per ..."
        // ------------------------------------------------------------------
        "boostXReserve" | "boostReserveCards" => {
            let n = g.players[me].reserve.len() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXReserveAll" => {
            let n = (g.players[0].reserve.len() + g.players[1].reserve.len()) as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostHandCards" => {
            let n = g.players[me].hand.len() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXLandmark" => {
            let n = g.players[me].landmarks.len() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXLandmarkMax3" => {
            let have = src.map(|c| g.card(c).boosts).unwrap_or(0);
            let n = (g.players[me].landmarks.len() as i32).min(3 - have);
            boost_me(g, &ctx, n, 99);
        }
        "boostXFeat" => {
            let n = g.in_play(me).into_iter().filter(|&c| g.card(c).def().card_type == CardType::Permanent && g.card(c).def().has_subtype("feat")).count() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXCompletedFeat" => {
            let n = completed_feats(g, me);
            boost_me(g, &ctx, n, 99);
        }
        "drawXCompletedFeat" => {
            let n = completed_feats(g, me);
            if n > 0 {
                push(g, json!({"action": "Draw", "args": {"players": "me", "n": n}}), Ctx::of(me, src));
            }
        }
        "boostXMana" => {
            let n = g.players[me].mana.len() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXFleetingChar" | "boostXAnchoredChar" | "boostXBoostedChar" => {
            let n = characters(g, me)
                .into_iter()
                .filter(|&c| match effect.as_str() {
                    "boostXFleetingChar" => g.card(c).has(Status::Fleeting),
                    "boostXAnchoredChar" => g.card(c).has(Status::Anchored),
                    _ => g.card(c).boosts > 0,
                })
                .count() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXAnimalsInExpeditions" => {
            let n = characters(g, me).into_iter().filter(|&c| Some(c) != src && g.card(c).zone.is_expedition() && g.card(c).def().has_subtype("animal")).count() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXAnimalsMax2" => {
            let n = g.in_play(me).into_iter().filter(|&c| Some(c) != src && g.card(c).def().has_subtype("animal")).count() as i32;
            boost_me(g, &ctx, n, 2);
        }
        "boostXExhaustedMax3" => {
            let p = &g.players[me];
            let permanents = g.in_play(me).into_iter().filter(|&c| g.card(c).def().card_type == CardType::Permanent && g.card(c).exhausted).count();
            let reserve = p.reserve.iter().filter(|&&c| g.card(c).exhausted).count();
            boost_me(g, &ctx, (permanents + reserve) as i32, 3);
        }
        "counterPerOpponentCharacter" => {
            let n = characters(g, Game::other(me)).len() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXOpponentExpedition" => {
            let n = characters(g, Game::other(me)).into_iter().filter(|&c| g.card(c).zone.is_expedition()).count() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXAscended" => {
            let n = g.players[me].ascended.iter().filter(|&&x| x).count() as i32;
            boost_me(g, &ctx, n, 99);
        }
        "boostXForForest" => {
            let mut n = 0;
            for seat in 0..2 {
                for side in Side::BOTH {
                    if g.region_biomes(g.players[seat].position[side.index()]).contains(&Biome::Forest) {
                        n += 1;
                    }
                }
            }
            boost_me(g, &ctx, n, 99);
        }
        "boost3Stat0" | "boost23Stat0" => {
            let zeros: usize = characters(g, me).into_iter().map(|c| g.card(c).def().stats.iter().filter(|&&v| v == 0).count()).sum();
            if zeros >= 3 {
                boost_me(g, &ctx, if effect == "boost23Stat0" { 2 } else { 1 }, 99);
            }
        }
        "boostTargetReserveCards" => {
            let n = g.players[me].reserve.len() as i32;
            if let (Some(t), true) = (card_arg(g, a, &ctx), n > 0) {
                push(g, gain(t, "boost", n, 99), Ctx::of(ctx.seat, src));
            }
        }
        "doppelganger" => {
            // The subtypes copied are not supported (cards have no subtype state): only the boosts
            if let Some(t) = card_arg(g, a, &ctx) {
                let n = g.card(t).stats().into_iter().max().unwrap_or(0);
                boost_me(g, &ctx, n, 99);
            }
        }
        "resetCard" => {}

        // ------------------------------------------------------------------
        // Several cards gain something
        // ------------------------------------------------------------------
        "boostAllSubtype" => {
            let sub = s(&inner, "subType").or(s(&inner, "subtype")).unwrap_or("").to_string();
            let n = i(&inner, "n", 1);
            let cards: Vec<CardId> = g
                .in_play(me)
                .into_iter()
                .filter(|&c| !(b(&inner, "excludeSelf") && Some(c) == src) && g.card(c).def().has_subtype(&sub))
                .collect();
            gain_all(g, &cards, "boost", n, &ctx);
        }
        "boostAllCharacters" => {
            let cards = characters(g, me);
            gain_all(g, &cards, "boost", 1, &ctx);
        }
        "boostAllCharactersExceptSelf" => {
            let cards: Vec<CardId> = characters(g, me).into_iter().filter(|&c| Some(c) != src).collect();
            gain_all(g, &cards, "boost", 1, &ctx);
        }
        "boostAllCharactersInExpedition" | "fleetingAllCharactersInExpedition" | "sleepingAllCharactersinExpedition" => {
            let Some((seat, side)) = target_expedition(g, a, &ctx) else { return };
            let mut cards = expedition_characters(g, seat, side);
            let kind = match effect.as_str() {
                "boostAllCharactersInExpedition" => "boost",
                "fleetingAllCharactersInExpedition" => "fleeting",
                _ => {
                    cards.retain(|&c| !g.card(c).has(Status::Asleep));
                    "asleep"
                }
            };
            gain_all(g, &cards, kind, 1, &ctx);
        }
        "allCharacterFleeting" | "allOtherCharacterFleeting" => {
            let cards: Vec<CardId> = g
                .turn_order()
                .into_iter()
                .flat_map(|seat| characters(g, seat))
                .filter(|&c| !g.card(c).has(Status::Fleeting) && !(effect == "allOtherCharacterFleeting" && Some(c) == src))
                .collect();
            gain_all(g, &cards, "fleeting", 1, &ctx);
        }
        "fleetingAllCharacters" => {
            let seat = ctx.target_seat.unwrap_or(ctx.seat);
            let cards: Vec<CardId> = characters(g, seat).into_iter().filter(|&c| !g.card(c).has(Status::Fleeting)).collect();
            gain_all(g, &cards, "fleeting", 1, &ctx);
        }
        "boostReserve" | "boostReserveSubtype" => {
            let sub = s(&inner, "subType").map(String::from);
            let cards: Vec<CardId> = g.players[ctx.seat]
                .reserve
                .iter()
                .copied()
                .filter(|&c| match &sub {
                    Some(sub) => g.card(c).def().has_subtype(sub),
                    None => g.card(c).def().card_type == CardType::Character,
                })
                .collect();
            gain_all(g, &cards, "boost", 1, &ctx);
        }
        "doubleBoosts" => {
            let p = &g.players[me];
            let nodes: Vec<Value> = p.reserve.iter().copied().chain(g.in_play(me)).filter(|&c| g.card(c).boosts > 0).map(|c| gain(c, "boost", g.card(c).boosts, 99)).collect();
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "PlagueofIntolerance" => {
            let count = g.in_play(ctx.seat).into_iter().filter(|&c| is_character(g, c)).count();
            let mut nodes = vec![];
            if count >= 2 {
                nodes.push(json!({"action": "Draw", "args": {"players": "me"}}));
            }
            if let (Some(c), true) = (src, count >= 4) {
                nodes.push(gain(c, "boost", 2, 99));
            }
            if count >= 6 {
                nodes.extend(characters(g, me).into_iter().filter(|&c| Some(c) != src).map(|c| gain(c, "boost", 1, 99)));
            }
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "boostAndRemoveFromExpedition" => {
            let Some(c) = src else { return };
            let card = g.card(c);
            let (zone, gigantic) = (card.zone, card.is_gigantic());
            let others: Vec<CardId> = characters(g, me).into_iter().filter(|&x| x != c && (g.card(x).zone == zone || (gigantic && g.card(x).zone.is_expedition()))).collect();
            if others.is_empty() {
                return;
            }
            let mut nodes = vec![gain(c, "boost", others.len() as i32, 99)];
            nodes.extend(others.iter().map(|&x| json!({"action": "Discard", "args": {"cardId": x, "destination": "reserve"}})));
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "boostXreserveBoost" | "augmentXreserveBoost" => {
            let n = ctx.event.as_ref().and_then(|e| e.boosts).unwrap_or(0);
            let node = if effect == "boostXreserveBoost" {
                json!({"action": "Target", "args": {"targetLocation": ["reserve"], "targetPlayer": "me", "excludeSelf": true,
                    "effect": {"action": "Gain", "args": {"cardId": "EFFECT", "type": "boost", "n": 1}}}})
            } else {
                json!({"action": "Target", "args": {"targetType": ["character", "spell", "permanent"], "targetLocation": ["stormLeft", "stormRight", "landmark", "reserve"],
                    "upTo": true, "augmentOnly": true, "effect": {"action": "Gain", "args": {"type": "augment"}}}})
            };
            push_seq(g, vec![node; n.max(0) as usize], Ctx::of(ctx.seat, src));
        }

        // ------------------------------------------------------------------
        // Each player...
        // ------------------------------------------------------------------
        "eachPlayerAsleep" => {
            let nodes = for_each_player(g, |_| {
                json!({"action": "Target", "args": {"targetType": ["character", "token"], "targetPlayer": "me",
                    "effect": {"action": "Gain", "args": {"cardId": "EFFECT", "type": "asleep", "n": 1}}}})
            });
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "AllPlayersSacrifice1" => {
            let nodes = for_each_player(g, |_| {
                json!({"action": "Target", "args": {"targetType": ["character", "token"], "targetPlayer": "me",
                    "effect": {"action": "Discard", "args": {"desc": "sacrifice"}}}})
            });
            g.log_line(Some(me), format!("{}All players must sacrifice 1 character", source_prefix(g, src)));
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "eachPlayerExhaust" => {
            let nodes = for_each_player(g, |_| {
                json!({"action": "Target", "args": {"targetType": ["character", "spell", "permanent"], "targetPlayer": "me", "targetLocation": ["reserve"],
                    "effect": {"action": "Exhaust", "args": {}}}})
            });
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "tiktok" => {
            let nodes = for_each_player(g, |_| {
                json!({"type": "xor", "childs": [
                    {"action": "Target", "args": {"targetType": ["permanent"], "targetPlayer": "me", "isNotTapped": true,
                        "effect": {"action": "Exhaust", "args": {"cardId": "EFFECT"}}}},
                    {"action": "Target", "args": {"targetType": ["spell", "character", "permanent"], "targetPlayer": "me", "isNotTapped": true, "targetLocation": ["reserve"],
                        "effect": {"action": "Exhaust", "args": {"cardId": "EFFECT"}}}},
                ]})
            });
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "eachPlayerOptionalHandReserve" | "eachPlayerOptionalHandReserveDraw" => {
            let draw = effect == "eachPlayerOptionalHandReserveDraw";
            let nodes = for_each_player(g, |_| {
                let mut then = json!({"action": "Discard", "args": {"destination": "reserve"}});
                if draw {
                    then = json!({"type": "seq", "childs": [then, {"action": "Draw", "args": {"players": "me"}}]});
                }
                json!({"action": "Target", "optional": true, "args": {"targetType": ["character", "spell", "permanent"], "targetPlayer": "me", "upTo": true,
                    "targetLocation": ["hand"], "effect": then}})
            });
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "playAll1Card" => {
            let nodes = for_each_player(g, |_| json!({"action": "ChooseAssignment", "optional": true, "args": {"actions": ["play"], "maxBaseCost": 3, "free": true}}));
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "eachPlayerResupply" | "eachPlayerOptionalResupply" => {
            let optional = effect == "eachPlayerOptionalResupply";
            let mut nodes = vec![];
            for seat in 0..2 {
                if !optional && g.players[seat].deck.is_empty() {
                    continue;
                }
                nodes.push(json!({"type": "seq", "optional": optional, "pId": g.players[seat].id, "childs": [{"action": "Resupply", "args": {}}]}));
            }
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "targetOpponentOptionalExhaustedResupply" => {
            let opp = Game::other(g.active);
            let childs = vec![json!({"action": "Resupply", "args": {"exhausted": true}}); i(&inner, "n", 1).max(1) as usize];
            push(g, json!({"type": "seq", "optional": true, "pId": g.players[opp].id, "childs": childs}), Ctx::of(ctx.seat, src));
        }
        "discardAllHand" | "discardAllHandReserve" | "discardAllReserve" => {
            for seat in 0..2 {
                let p = &g.players[seat];
                let mut cards = vec![];
                if effect != "discardAllReserve" {
                    cards.extend(p.hand.iter().copied());
                }
                if effect != "discardAllHand" {
                    cards.extend(p.reserve.iter().copied());
                }
                for c in cards {
                    move_with_events(g, c, Zone::Discard, false, false, &Ctx::of(seat, src));
                }
            }
        }
        "readyAllReserve" => {
            let nodes: Vec<Value> = g.players[ctx.seat].reserve.iter().filter(|&&c| g.card(c).exhausted).map(|&c| json!({"action": "Ready", "args": {"cardId": c}})).collect();
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "exhaustAllCards" => {
            let nodes: Vec<Value> = (0..2)
                .flat_map(|seat| g.players[seat].reserve.clone())
                .filter(|&c| !g.card(c).exhausted)
                .map(|c| json!({"action": "Exhaust", "args": {"cardId": c}}))
                .collect();
            push_seq(g, nodes, Ctx::of(me, src));
        }
        "activateAllPermanents" | "activateAllOtherCharacters" => {
            let permanents = effect == "activateAllPermanents";
            let nodes: Vec<Value> = g
                .in_play(me)
                .into_iter()
                .filter(|&c| {
                    let d = g.card(c).def();
                    if permanents {
                        d.card_type == CardType::Permanent
                    } else {
                        d.card_type == CardType::Character && !d.is_token && Some(c) != src && d.effect("effectPlayed").is_some()
                    }
                })
                .map(|c| json!({"action": "ActivateEffect", "optional": true, "args": {"cardId": c}}))
                .collect();
            push_seq(g, nodes, Ctx::of(me, src));
        }
        "allPass" => {
            for seat in [me, Game::other(me)] {
                if !g.players[seat].passed {
                    g.end_turn(seat);
                }
            }
        }
        "switchPlayer" => {
            let seat = ctx.target_seat.unwrap_or(ctx.seat);
            g.first = seat;
            let name = g.players[seat].name.clone();
            g.log_line(Some(seat), format!("{} becomes the first player", name));
        }
        "instantWin" => {
            if g.winner.is_none() {
                let name = g.players[me].name.clone();
                g.log_line(Some(me), format!("{} wins the game with {}effect", name, source_prefix(g, src)));
                g.end_game(me);
            }
        }

        // ------------------------------------------------------------------
        // Expeditions
        // ------------------------------------------------------------------
        "ascend" => {
            let explicit = s(a, "expedition").or(s(&inner, "expedition"));
            let side = match explicit {
                Some("source") => src.and_then(|c| g.card(c).zone.side()),
                Some(z) => Zone::from_engine(z).and_then(Zone::side),
                None => ctx.side,
            };
            let Some(side) = side else { return };
            let player = if explicit.is_some() { ctx.side_seat.unwrap_or(me) } else { ctx.side_seat.or(ctx.target_seat).unwrap_or(me) };
            if !g.players[player].ascended[side.index()] {
                ascend(g, player, side, src);
            } else {
                let cards = g.in_play(player);
                if cards.iter().any(|&c| g.card(c).def().flag("resupplyIfAscended")) {
                    push(g, json!({"action": "Resupply", "args": {}}), Ctx::of(ctx.seat, src));
                }
                let kiwis: Vec<CardId> = cards.into_iter().filter(|&c| g.card(c).def().flag("boostIfAscended")).collect();
                gain_all(g, &kiwis, "boost", 1, &ctx);
            }
        }
        "ascendOnLeave" => {
            let from = ctx.event.as_ref().and_then(|e| e.from).and_then(Zone::side).or_else(|| src.and_then(|c| g.card(c).zone.side()));
            let Some(mut side) = from else { return };
            if s(a, "expedition").or(s(&inner, "expedition")) == Some("oppositeSource") {
                side = side.other();
            }
            if !g.players[ctx.seat].ascended[side.index()] {
                ascend(g, ctx.seat, side, src);
            }
        }
        "invokeXRecruitReserve" => {
            let Some(side) = ctx.side else { return };
            let owner = ctx.side_seat.unwrap_or(ctx.seat);
            let n = g.players[ctx.target_seat.unwrap_or(ctx.seat)].reserve.len() as i32;
            if n > 0 {
                push(g, invoke("OD_Common_OrdisRecruit", n, &[side]), Ctx::of(owner, src));
            }
        }
        "invokeRecruitBehind" | "invokeBrassbugBehind" => {
            let token = if effect == "invokeBrassbugBehind" { "AX_Common_Brassbug" } else { "OD_Common_OrdisRecruit" };
            let opp = Game::other(me);
            let (mine, theirs) = (g.players[me].position, g.players[opp].position);
            let mut nodes = vec![];
            for side in Side::BOTH {
                // The expedition ahead: the hero one goes up, the companion one goes down
                let ahead = match side {
                    Side::Hero => mine[0] > theirs[0],
                    Side::Companion => mine[1] < theirs[1],
                };
                let override_behind = g.in_play(me).into_iter().any(|c| {
                    let card = g.card(c);
                    (card.zone == side.zone() || (card.is_gigantic() && card.zone.is_expedition())) && card.def().flag("overrideBehind")
                });
                if !ahead || g.tiebreaker || override_behind {
                    nodes.push(invoke(token, 1, &[side]));
                }
            }
            push_seq(g, nodes, Ctx::of(me, src));
        }
        "invokeRecruitOnAscendedExpeditions" => {
            let nodes: Vec<Value> = Side::BOTH.into_iter().filter(|s| g.players[me].ascended[s.index()]).map(|s| invoke("OD_Common_OrdisRecruit", 1, &[s])).collect();
            push_seq(g, nodes, Ctx::of(me, src));
        }
        "invokeOrdisRecruitBureaucrat" => {
            let n = g.in_play(me).into_iter().filter(|&c| g.card(c).def().has_subtype("bureaucrat")).count();
            let node = json!({"action": "InvokeToken", "args": {"tokenType": "OD_Common_OrdisRecruit", "targetLocation": ["source"]}});
            push_seq(g, vec![node; n], Ctx::of(me, src));
        }
        "invokeManaMothPerStigmaInDiscard" => {
            let n = g.players[me].discard.iter().filter(|&&c| uid_matches(g.card(c).def().prop("uid"), "YZ_115")).count();
            let node = json!({"action": "InvokeToken", "args": {"tokenType": "YZ_Common_ManaMoth", "targetLocation": ["stormLeft", "stormRight"]}});
            push_seq(g, vec![node; n], Ctx::of(me, src));
        }
        "sendToReserveCharactersAndExhaustInExpedition" => {
            let Some((seat, side)) = target_expedition(g, a, &ctx) else { return };
            let cards = expedition_characters(g, seat, side);
            let mut nodes: Vec<Value> = cards.iter().map(|&c| json!({"action": "Discard", "args": {"cardId": c, "destination": "reserve"}})).collect();
            nodes.extend(cards.iter().map(|&c| json!({"action": "Exhaust", "args": {"cardId": c}})));
            push_seq(g, nodes, Ctx::of(me, src));
        }
        "discardAllAndBackward" => {
            let Some((seat, side)) = target_expedition(g, a, &ctx) else { return };
            let mut nodes: Vec<Value> = expedition_characters(g, seat, side)
                .into_iter()
                .filter(|&c| is_character(g, c))
                .map(|c| json!({"action": "Discard", "args": {"cardId": c}}))
                .collect();
            nodes.push(json!({"action": "MoveExpedition", "args": {"n": -1, "expedition": zone_key(side.zone())}}));
            let mut c = Ctx::of(ctx.seat, src);
            c.target_seat = Some(seat);
            push_seq(g, nodes, c);
        }

        // ------------------------------------------------------------------
        // Delayed until after Rest
        // ------------------------------------------------------------------
        "AfterRestSabotage" | "AfterRestOrdisRecruit" | "AfterRest2OrdisRecruit" | "afterRest" => {
            let node = match effect.as_str() {
                "AfterRestSabotage" => json!({"action": "Target", "args": {"targetType": ["character", "spell", "token", "permanent"], "targetLocation": ["reserve"],
                    "upTo": true, "effect": {"action": "Discard", "args": {}}}}),
                "AfterRestOrdisRecruit" | "AfterRest2OrdisRecruit" => {
                    let here = src.map(|c| g.card(c).zone).filter(|z| z.is_expedition());
                    let locations: Vec<&str> = match (s(a, "expedition").or(s(&inner, "expedition")), here) {
                        (Some("source"), Some(z)) => vec![zone_key(z)],
                        _ => vec!["stormLeft", "stormRight"],
                    };
                    let n = if effect == "AfterRest2OrdisRecruit" { 2 } else { 1 };
                    json!({"action": "InvokeToken", "args": {"pId": "source", "tokenType": "OD_Common_OrdisRecruit", "n": n, "targetLocation": locations}})
                }
                _ => {
                    // The source's expedition now, as it may be in Reserve after Rest
                    let here = src.map(|c| zone_key(g.card(c).zone)).unwrap_or("other");
                    replace_source_location(Value::Object(inner.clone()), here)
                }
            };
            g.mods.special.after_rest[me].push((node, Ctx::of(me, src)));
        }

        // ------------------------------------------------------------------
        // Opponent's cards
        // ------------------------------------------------------------------
        "sacrificeHighestCharacter" | "sacrificeHighestCharacterPermanent" => {
            let opp = Game::other(me);
            let with_permanents = effect == "sacrificeHighestCharacterPermanent";
            let cards: Vec<CardId> = g
                .in_play(opp)
                .into_iter()
                .filter(|&c| is_character(g, c) || (with_permanents && g.card(c).def().card_type == CardType::Permanent))
                .collect();
            let cost = |c: CardId| {
                let card = g.card(c);
                if card.has(Status::Fleeting) {
                    card.def().cost_reserve
                } else {
                    card.def().cost_hand
                }
            };
            let Some(max) = cards.iter().map(|&c| cost(c)).max() else { return };
            let highest: Vec<CardId> = cards.iter().copied().filter(|&c| cost(c) == max).collect();
            let sacrifice = json!({"action": "Discard", "args": {"desc": "sacrifice"}});
            let c = Ctx::of(opp, src);
            if highest.len() == 1 {
                crate::actions::apply_targets(g, &highest, Some(&sacrifice), &c);
            } else {
                ask_select(g, c, highest, 1, 1, sacrifice, false, "Choose the Character to sacrifice");
            }
        }
        "hunger" => {
            let mut nodes = vec![];
            let mut ids: Vec<CardId> = (0..2).flat_map(|seat| g.in_play(seat).into_iter().chain(g.players[seat].reserve.iter().copied())).collect();
            ids.sort();
            for c in ids {
                if Some(c) == src {
                    continue;
                }
                let card = g.card(c);
                if card.zone.is_expedition() && card.is_character_like() {
                    if card.boosts > 0 && protected(g, c, "protectBoostedInExpedition") {
                        nodes.push(json!({"action": "Loose", "args": {"cardId": c, "type": "boost", "n": 99}}));
                        continue;
                    }
                    if card.has(Status::Anchored) && protected(g, c, "protectAnchoredInExpedition") {
                        nodes.push(json!({"action": "Loose", "args": {"cardId": c, "type": "anchored"}}));
                        continue;
                    }
                }
                nodes.push(json!({"action": "Discard", "args": {"cardId": c}}));
            }
            push_seq(g, nodes, Ctx::of(ctx.seat, src));
        }
        "defect" => {
            let Some(t) = card_arg(g, a, &ctx) else { return };
            let card = g.card(t);
            if !card.zone.in_play() {
                return;
            }
            let new = if b(&inner, "takeControl") { me } else { Game::other(card.controller) };
            let mut zone = card.zone;
            if b(&inner, "moveToMe") && !card.def().has_subtype("landmark") {
                if let Some(z) = src.map(|c| g.card(c).zone).filter(|z| z.in_play()) {
                    zone = z;
                }
            }
            let from = card.zone;
            g.move_card(t, zone, Some(new));
            let (name, pname) = (g.card(t).name(), g.players[new].name.clone());
            g.log_line(Some(new), format!("[[{}]] defects to {}", name, pname));
            g.emit(Event { trigger: "MoveCard".into(), seat: new, card: Some(t), from: Some(from), to: Some(zone), listeners: vec![t], ..Default::default() });
        }

        // ------------------------------------------------------------------
        // Revealing and playing cards from the deck
        // ------------------------------------------------------------------
        "reveal" => {
            if let Some(c) = card_arg(g, a, &ctx) {
                let (name, pname) = (g.card(c).name(), g.players[g.card(c).controller].name.clone());
                g.forget_undo();
                g.log_line(Some(ctx.seat), format!("{} reveals [[{}]]", pname, name));
            }
        }
        "revealTop" => {
            if g.mods.special.revealed[me].is_none() {
                if let Some(&c) = g.players[me].deck.last() {
                    g.mods.special.revealed[me] = Some(c);
                    g.forget_undo();
                    let (name, pname) = (g.card(c).name(), g.players[me].name.clone());
                    g.log_line(Some(me), format!("{} reveals [[{}]] from the top of their deck", pname, name));
                }
            }
        }
        "drawRevealed" => {
            let Some(c) = g.mods.special.revealed[me].take() else { return };
            if g.card(c).zone != Zone::Deck {
                return;
            }
            let t = type_name(g.card(c).def().card_type);
            if s(&inner, "type") == Some(t) {
                to_hand(g, me, c, src);
            } else if b(&inner, "reserveIfNotType") || s(&inner, "reserve") == Some(t) {
                g.move_card(c, Zone::Reserve, None);
                g.emit(Event { trigger: "Resupply".into(), seat: me, card: Some(c), to: Some(Zone::Reserve), source: src, ..Default::default() });
            }
        }
        "drawTopIfRoll" | "exhaustPlayFree" => {
            // The die result comes from the RollDie event (selectedRoll), else the "roll" arg (the Wayfarer, OD)
            let event_roll = ctx.event.as_ref().filter(|e| e.trigger == "RollDie").map(|e| e.n);
            let arg_roll = if effect == "drawTopIfRoll" { a.get("roll").and_then(Value::as_i64).map(|r| r as i32) } else { None };
            let Some(roll) = event_roll.or(arg_roll) else { return };
            let Some(c) = g.mods.special.revealed[me] else { return };
            if g.card(c).zone != Zone::Deck || g.card(c).def().cost_hand != roll {
                return;
            }
            g.mods.special.revealed[me] = None;
            if effect == "drawTopIfRoll" {
                to_hand(g, me, c, src);
            } else {
                let nodes = vec![json!({"action": "Tap", "args": {}}), internal("_playFree", json!({"cardId": c, "effectHand": false}))];
                push_seq(g, nodes, Ctx::of(me, src));
            }
        }
        "discardTopDeck" => {
            for _ in 0..i(&inner, "n", 1).max(1) {
                let Some(&c) = g.players[me].deck.last() else { break };
                g.forget_undo();
                move_with_events(g, c, Zone::Discard, false, false, &Ctx::of(me, src));
            }
        }
        "boostedRevealBaseStat" | "boostedRevealArtistSong" | "boostedRevealRobotPermanent" => {
            let boosted = src.map(|c| g.card(c).boosts > 0).unwrap_or(false);
            if !boosted && !b(&inner, "bypass") {
                return;
            }
            let Some(&c) = g.players[me].deck.last() else { return };
            g.forget_undo();
            let d = g.card(c).def();
            let (name, pname) = (d.name.clone(), g.players[me].name.clone());
            g.log_line(Some(me), format!("{} reveals [[{}]] from the top of their deck", pname, name));
            let keep = match effect.as_str() {
                "boostedRevealBaseStat" => d.card_type == CardType::Character && d.stats.contains(&0),
                "boostedRevealArtistSong" => d.has_subtype("artist") || d.has_subtype("song"),
                _ => d.has_subtype("robot") || d.card_type == CardType::Permanent,
            };
            if keep {
                to_hand(g, me, c, src);
            } else {
                move_with_events(g, c, Zone::Discard, false, false, &Ctx::of(me, src));
            }
        }
        "AuraqKibble" => {
            let Some(&c) = g.players[me].deck.last() else { return };
            g.forget_undo();
            g.move_card(c, Zone::Limbo, None);
            let (name, pname) = (g.card(c).name(), g.players[me].name.clone());
            g.log_line(Some(me), format!("{} reveals [[{}]] from the top of their deck", pname, name));
            let options = vec![
                ChoiceOpt { label: format!("Play [[{}]] for free", name), node: Some(internal("_playFree", json!({"cardId": c, "effectHand": false, "orHand": true}))), enabled: true, source: src },
                ChoiceOpt { label: format!("Put [[{}]] into your hand", name), node: Some(internal("_toHand", json!({"cardId": c}))), enabled: true, source: src },
            ];
            g.ask(Pending { id: 0, seat: me, prompt: format!("{}Play the revealed card or put it into your hand", source_prefix(g, src)), source: src, can_pass: false, kind: PendingKind::Choice { options }, ctx: Ctx::of(me, src) });
        }
        "MindApotheosis" => {
            let drawn = g.draw(me, 4, src);
            if g.players[me].deck.is_empty() {
                return;
            }
            push(g, internal("_discardRemaining", json!({"cards": drawn, "zone": "hand"})), Ctx::of(me, src));
            let chars: Vec<CardId> = drawn.iter().copied().filter(|&c| is_character(g, c)).collect();
            if !chars.is_empty() {
                let then = json!({"type": "seq", "childs": [internal("_playFree", json!({"effectHand": false})), {"action": "Gain", "args": {"type": "fleeting"}}]});
                let max = chars.len().min(2);
                ask_select(g, Ctx::of(me, src), chars, 0, max, then, true, "Choose up to two Characters to put in your Expeditions");
            }
        }
        "RomanticEncounter" | "RomanticEncounterRare" => {
            if effect == "RomanticEncounterRare" {
                let nodes = vec![
                    internal("RomanticEncounter", json!({"player": "me", "rare": true})),
                    internal("RomanticEncounter", json!({"player": "opponent", "rare": true})),
                ];
                push_seq(g, nodes, Ctx::of(me, src));
                return;
            }
            let owner = if s(&inner, "player") == Some("me") { me } else { Game::other(me) };
            if g.players[owner].deck.is_empty() {
                return;
            }
            let cards = reveal_to_limbo(g, owner, 4);
            push(g, internal("_discardRemaining", json!({"cards": cards, "zone": "limbo"})), Ctx::of(me, src));
            let max_cost = if b(&inner, "rare") { 3 } else { 99 };
            let chars: Vec<CardId> = cards.iter().copied().filter(|&c| g.card(c).is_character_like() && g.card(c).def().cost_hand <= max_cost).collect();
            if !chars.is_empty() {
                let then = json!({"type": "seq", "childs": [internal("_playFree", json!({"effectHand": false})), {"action": "Gain", "args": {"type": "fleeting"}}]});
                ask_select(g, Ctx::of(me, src), chars, 0, 1, then, true, "You may play a Character for free");
            }
        }
        "RunningwiththeWolves" => {
            let cards = reveal_to_limbo(g, me, 5);
            push(g, internal("_discardRemaining", json!({"cards": cards, "zone": "limbo"})), Ctx::of(me, src));
            let mut args = inner.clone();
            args.insert("cards".into(), json!(cards));
            wolves_pick(g, &args, Ctx::of(me, src));
        }
        "Phoibos" | "PhoibosUnique" => {
            let opp = Game::other(me);
            let hand = g.players[opp].hand.clone();
            if hand.is_empty() {
                return;
            }
            let c = hand[g.rng.below(hand.len())];
            g.forget_undo();
            g.move_card(c, Zone::Limbo, None);
            let (name, pname) = (g.card(c).name(), g.players[opp].name.clone());
            g.log_line(Some(opp), format!("{} reveals [[{}]] from their hand", pname, name));
            let discard = json!({"action": "Discard", "args": {"cardId": c}});
            if effect == "Phoibos" && !crate::conditions::check(g, "isInContact", &ctx) {
                push(g, discard, Ctx::of(me, src));
                return;
            }
            let play = json!({"type": "seq", "childs": [internal("_playFree", json!({"cardId": c, "effectHand": true})), {"action": "Gain", "args": {"cardId": c, "type": "fleeting"}}]});
            let options = vec![
                ChoiceOpt { label: format!("Play [[{}]] for free", name), node: Some(play), enabled: true, source: src },
                ChoiceOpt { label: format!("Discard [[{}]]", name), node: Some(discard), enabled: true, source: src },
            ];
            g.ask(Pending { id: 0, seat: me, prompt: format!("{}Play the revealed card or discard it", source_prefix(g, src)), source: src, can_pass: false, kind: PendingKind::Choice { options }, ctx: Ctx::of(me, src) });
        }

        // ------------------------------------------------------------------
        // Gifts, repeats, boosts removed
        // ------------------------------------------------------------------
        "copyGift" => {
            let Some(e) = ctx.event.clone() else { return };
            let node = match e.trigger.as_str() {
                "InvokeToken" => {
                    let Some(t) = e.card else { return };
                    let locations: Vec<&str> = if e.to == Some(Zone::Landmark) { vec!["landmark"] } else { vec!["stormLeft", "stormRight"] };
                    json!({"action": "InvokeToken", "args": {"tokenType": g.card(t).def().reference, "targetLocation": locations, "targetPlayer": "me"}})
                }
                "Draw" => json!({"action": "Draw", "args": {"n": 1, "players": "me"}}),
                "Resupply" => {
                    let exhausted = e.card.map(|c| g.card(c).exhausted).unwrap_or(false);
                    json!({"action": "Resupply", "args": {"n": 1, "exhausted": exhausted}})
                }
                _ => return,
            };
            push(g, node, Ctx::of(me, src));
        }
        "doCounter1" | "doEachBoost" => {
            let n = if effect == "doCounter1" { src.map(|c| g.card(c).counter).unwrap_or(0) + 1 } else { ctx.event.as_ref().and_then(|e| e.boosts).unwrap_or(0) };
            let Some(e) = inner.get("effect").cloned() else { return };
            push_seq(g, vec![e; n.max(0) as usize], Ctx::of(me, src));
        }
        "ManInTheMaze" | "ManInTheMazeRare" | "ManInTheMazeYzmir" | "manInTheMazeUnique" => {
            let mut removed = 0;
            for seat in 0..2 {
                let ids: Vec<CardId> = g.in_play(seat).into_iter().chain(g.players[seat].reserve.iter().copied()).filter(|&c| g.card(c).is_character_like()).collect();
                for c in ids {
                    removed += std::mem::take(&mut g.card_mut(c).boosts);
                }
            }
            g.log_line(Some(me), format!("{}{} boost(s) removed", source_prefix(g, src), removed));
            let Some(c) = src else { return };
            let gain2 = gain(c, "boost", 2, 99);
            let play = json!({"action": "ChooseAssignment", "args": {"actions": ["play"], "maxBaseCost": 3, "free": true}});
            let side = g.card(c).zone.side();
            let forward = side.map(|s| json!({"action": "MoveExpedition", "args": {"n": 1, "expedition": zone_key(s.zone())}}));
            let mut nodes = vec![];
            match effect.as_str() {
                "ManInTheMaze" | "ManInTheMazeYzmir" => match removed {
                    0 => {}
                    1..=4 => nodes.push(gain2),
                    5..=8 => nodes.push(play),
                    _ if effect == "ManInTheMaze" => nodes.extend(forward),
                    _ => nodes.extend(side.map(|s| json!({"action": "MoveExpedition", "pId": "opponent", "args": {"n": -1, "expedition": zone_key(s.zone())}}))),
                },
                "ManInTheMazeRare" => {
                    if removed >= 1 {
                        nodes.push(gain2);
                    }
                    if removed >= 5 {
                        nodes.push(play);
                    }
                    if removed >= 9 {
                        nodes.extend(forward);
                    }
                }
                _ => {
                    for (k, min) in [("1+", 1), ("4+", 4), ("9+", 9)] {
                        if removed >= min {
                            nodes.extend(inner.get(k).cloned());
                        }
                    }
                }
            }
            push_seq(g, nodes, Ctx::of(me, src));
        }

        // ------------------------------------------------------------------
        // Internal steps of the effects above
        // ------------------------------------------------------------------
        "_playFree" => {
            let Some(c) = card_arg(g, &inner, &ctx) else { return };
            let played = play_free(g, ctx.clone(), c, inner.get("effectHand").and_then(Value::as_bool).unwrap_or(true));
            if !played && b(&inner, "orHand") && g.card(c).zone == Zone::Limbo {
                to_hand(g, ctx.seat, c, src);
            }
        }
        "_toHand" => {
            if let Some(c) = card_arg(g, &inner, &ctx) {
                to_hand(g, ctx.seat, c, src);
            }
        }
        "_discardRemaining" => {
            let zone = if s(&inner, "zone") == Some("hand") { Zone::Hand } else { Zone::Limbo };
            let cards: Vec<CardId> = inner.get("cards").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.as_u64().map(|v| v as CardId)).collect()).unwrap_or_default();
            for c in cards {
                if g.card(c).zone == zone && Some(c) != src {
                    let c2 = Ctx::of(g.card(c).controller, src);
                    move_with_events(g, c, Zone::Discard, false, false, &c2);
                }
            }
        }
        "_wolvesPick" => wolves_pick(g, &inner, ctx),
        // Give Fate a Twist: each result of the roll being resolved is also offered +n
        "addToCurrentRolls" => {
            let n = i(&inner, "n", 1);
            if !g.dice_rolls.is_empty() && n > 0 {
                let mut rolls = g.dice_rolls.clone();
                rolls.extend(g.dice_rolls.iter().map(|r| r + n));
                rolls.sort();
                rolls.dedup();
                g.dice_rolls = rolls;
            }
        }

        other => {
            // Not supported: playAnotherTurn (no turn-order hook), swapBoostsWithSource
            // (BoostExchange), doEachCardInDiscardPileSourceName
            g.unsupported(format!("special {}", other));
            let name = src.map(|c| g.card(c).name()).unwrap_or("?");
            g.log_line(None, format!("(Not implemented yet: [[{}]] effect \"{}\")", name, other));
        }
    }
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

/// Player who controls the source card (the original engine's $card->getPId())
fn controller(g: &Game, ctx: &Ctx) -> Seat {
    ctx.source.map(|c| g.card(c).controller).unwrap_or(ctx.seat)
}

fn push(g: &mut Game, node: Value, ctx: Ctx) {
    g.stack.push(Task::Run { node, ctx });
}

fn push_seq(g: &mut Game, nodes: Vec<Value>, ctx: Ctx) {
    match nodes.len() {
        0 => {}
        1 => push(g, nodes.into_iter().next().unwrap(), ctx),
        _ => push(g, json!({"type": "seq", "childs": nodes}), ctx),
    }
}

fn internal(effect: &str, args: Value) -> Value {
    json!({"action": "SpecialEffect", "args": {"effect": effect, "args": args}})
}

fn gain(card: CardId, kind: &str, n: i32, up_to: i32) -> Value {
    json!({"action": "Gain", "args": {"cardId": card, "type": kind, "n": n, "upTo": up_to}})
}

/// The source card gains n boosts (up to a total of `up_to`)
fn boost_me(g: &mut Game, ctx: &Ctx, n: i32, up_to: i32) {
    if let (Some(c), true) = (ctx.source, n > 0) {
        push(g, gain(c, "boost", n, up_to), Ctx::of(ctx.seat, ctx.source));
    }
}

fn gain_all(g: &mut Game, cards: &[CardId], kind: &str, n: i32, ctx: &Ctx) {
    let nodes = cards.iter().map(|&c| gain(c, kind, n, 99)).collect();
    push_seq(g, nodes, Ctx::of(ctx.seat, ctx.source));
}

fn invoke(token: &str, n: i32, sides: &[Side]) -> Value {
    let locations: Vec<&str> = sides.iter().map(|s| zone_key(s.zone())).collect();
    json!({"action": "InvokeToken", "args": {"tokenType": token, "n": n, "targetLocation": locations}})
}

/// One node per player, starting with the active player, each run by that player
fn for_each_player(g: &Game, f: impl Fn(Seat) -> Value) -> Vec<Value> {
    [g.active, Game::other(g.active)]
        .into_iter()
        .map(|seat| {
            let mut n = f(seat);
            n["pId"] = json!(g.players[seat].id);
            n
        })
        .collect()
}

/// Characters and tokens a player controls (in play)
fn characters(g: &Game, seat: Seat) -> Vec<CardId> {
    g.in_play(seat).into_iter().filter(|&c| g.card(c).is_character_like()).collect()
}

/// A Character card (not a token)
fn is_character(g: &Game, c: CardId) -> bool {
    let d = g.card(c).def();
    d.card_type == CardType::Character && !d.is_token
}

fn type_name(t: CardType) -> &'static str {
    match t {
        CardType::Character => "character",
        CardType::Spell => "spell",
        CardType::Permanent => "permanent",
        CardType::Token => "token",
        CardType::Hero => "hero",
    }
}

/// Characters and tokens of an expedition, with the gigantic ones of the other expedition
fn expedition_characters(g: &Game, seat: Seat, side: Side) -> Vec<CardId> {
    characters(g, seat).into_iter().filter(|&c| g.card(c).zone == side.zone() || (g.card(c).is_gigantic() && g.card(c).zone.is_expedition())).collect()
}

/// The expedition an effect is about: its "expedition" arg, or the one chosen by TargetExpedition
fn target_expedition(g: &Game, a: &Map<String, Value>, ctx: &Ctx) -> Option<(Seat, Side)> {
    match s(a, "expedition") {
        Some("source") => ctx.source.and_then(|c| g.card(c).zone.side().map(|s| (g.card(c).controller, s))),
        Some(z) => Zone::from_engine(z).and_then(Zone::side).map(|s| (ctx.side_seat.unwrap_or(ctx.seat), s)),
        None => ctx.side.map(|s| (ctx.side_seat.or(ctx.target_seat).unwrap_or(ctx.seat), s)),
    }
}

fn completed_feats(g: &Game, seat: Seat) -> i32 {
    g.players[seat].landmarks.iter().filter(|&&c| g.card(c).completed).count() as i32
}

/// A card protected in its expedition by a card of its controller there (Floral Tent...)
fn protected(g: &Game, c: CardId, flag: &str) -> bool {
    let card = g.card(c);
    g.in_play(card.controller).into_iter().any(|x| {
        let z = g.card(x).zone;
        (z == card.zone || (card.is_gigantic() && z.is_expedition())) && g.card(x).def().flag(flag)
    })
}

/// "_YZ_115" in a card uid (Conditions::isCardMatchingSearch)
fn uid_matches(uid: Option<&Value>, name: &str) -> bool {
    let Some(uid) = uid.and_then(Value::as_str) else { return false };
    let needle = format!("_{}", name);
    uid.match_indices(&needle).any(|(k, _)| {
        let rest = &uid[k + needle.len()..];
        rest.is_empty() || rest.starts_with('_')
    })
}

fn gain_counter(g: &mut Game, c: CardId, n: i32) {
    if n <= 0 {
        return;
    }
    g.card_mut(c).counter += n;
    let seat = g.card(c).controller;
    let (name, pname) = (g.card(c).name(), g.players[seat].name.clone());
    g.log_line(Some(seat), format!("[[{}]] ({}) gains {} counter(s)", name, pname, n));
    g.emit(Event { trigger: "SpecialEffect".into(), seat, card: Some(c), source: Some(c), n, what: Some("gainCounter".into()), listeners: vec![c], ..Default::default() });
}

fn ascend(g: &mut Game, seat: Seat, side: Side, src: Option<CardId>) {
    g.players[seat].ascended[side.index()] = true;
    let name = g.players[seat].name.clone();
    let label = if side == Side::Hero { "Hero" } else { "Companion" };
    g.log_line(Some(seat), format!("{}{}'s {} expedition ascends", source_prefix(g, src), name, label));
}

/// Replaces "source" / "initialSource" in targetLocation lists of a tree (afterRest)
fn replace_source_location(mut v: Value, here: &str) -> Value {
    fn walk(v: &mut Value, here: &str) {
        match v {
            Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if k == "targetLocation" {
                        if let Value::Array(l) = x {
                            for loc in l.iter_mut() {
                                if loc == "source" || loc == "initialSource" {
                                    *loc = json!(here);
                                }
                            }
                        }
                    } else {
                        walk(x, here);
                    }
                }
            }
            Value::Array(l) => l.iter_mut().for_each(|x| walk(x, here)),
            _ => {}
        }
    }
    walk(&mut v, here);
    v
}

/// Asks to choose cards among a given list; `effect` runs once per chosen card (as its target)
#[allow(clippy::too_many_arguments)]
fn ask_select(g: &mut Game, ctx: Ctx, cards: Vec<CardId>, min: usize, max: usize, effect: Value, picker: bool, prompt: &str) {
    let src = ctx.source;
    g.ask(Pending {
        id: 0,
        seat: ctx.seat,
        prompt: format!("{}{}", source_prefix(g, src), prompt),
        source: src,
        can_pass: min == 0,
        kind: PendingKind::Select { cards, min, max, costs: vec![], then: SelectThen::Effect { effect: Some(effect), limits: (None, None) }, picker },
        ctx,
    });
}

/// Moves the top n cards of a player's deck to Limbo (revealed)
fn reveal_to_limbo(g: &mut Game, seat: Seat, n: usize) -> Vec<CardId> {
    let mut out = vec![];
    for _ in 0..n {
        if g.players[seat].deck.is_empty() {
            g.reshuffle_discard(seat);
        }
        let Some(&c) = g.players[seat].deck.last() else { break };
        g.move_card(c, Zone::Limbo, None);
        out.push(c);
    }
    g.forget_undo();
    let names: Vec<String> = out.iter().map(|&c| format!("[[{}]]", g.card(c).name())).collect();
    let pname = g.players[seat].name.clone();
    g.log_line(Some(seat), format!("{} reveals {} from their deck", pname, names.join(", ")));
    out
}

/// A card goes to its player's hand, as a draw
fn to_hand(g: &mut Game, seat: Seat, c: CardId, src: Option<CardId>) {
    g.move_card(c, Zone::Hand, Some(seat));
    let (name, pname) = (g.card(c).name(), g.players[seat].name.clone());
    g.log_line(Some(seat), format!("{} puts [[{}]] into their hand", pname, name));
    g.emit(Event { trigger: "Draw".into(), seat, n: 1, source: src, ..Default::default() });
}

/// Plays a card for free (from hand, Reserve, or revealed in Limbo), asking in which expedition
/// when there is a choice. Without `effect_hand`, its {h} abilities don't activate.
fn play_free(g: &mut Game, ctx: Ctx, c: CardId, effect_hand: bool) -> bool {
    let seat = ctx.seat;
    let zone = g.card(c).zone;
    if zone == Zone::Limbo || zone == Zone::Deck || (zone == Zone::Hand && g.card(c).controller != seat) {
        g.move_card(c, Zone::Hand, Some(seat));
    }
    let bonus = BonusPlay { types: vec![], free: true, max_base_cost: None, mandatory: false };
    let dests = g.playable(seat, Some(&bonus)).into_iter().find(|(x, _)| *x == c).map(|(_, d)| d).unwrap_or_default();
    if dests.is_empty() {
        if zone == Zone::Limbo || zone == Zone::Deck {
            g.move_card(c, zone, None);
        }
        return false;
    }
    let dest = match ctx.side {
        Some(side) => dests.iter().copied().find(|d| d.zone() == side.zone()),
        None if dests.len() == 1 => Some(dests[0]),
        None => None,
    };
    let Some(dest) = dest else {
        // Where to play it: an expedition choice, then the play
        let options: Vec<(Seat, Side)> = dests.iter().filter_map(|d| d.zone().side()).map(|s| (seat, s)).collect();
        let effect = internal("_playFree", json!({"cardId": c, "effectHand": effect_hand}));
        let name = g.card(c).name();
        let mut c2 = ctx.clone();
        c2.target = Some(c);
        g.ask(Pending {
            id: 0,
            seat,
            prompt: format!("{}Choose the expedition where [[{}]] is played", source_prefix(g, ctx.source), name),
            source: ctx.source,
            can_pass: false,
            kind: PendingKind::Expedition { options, min: 1, max: 1, effect: Some(effect) },
            ctx: c2,
        });
        return true;
    };
    if g.play_card(seat, c, dest, Some(&bonus)).is_err() {
        return false;
    }
    if !effect_hand {
        strip_effect_hand(g, c);
    }
    true
}

/// Removes the {h} abilities of a card just played from the effects waiting to resolve
fn strip_effect_hand(g: &mut Game, c: CardId) {
    let Some(e) = g.card(c).def().effect("effectHand") else { return };
    let parts: Vec<Value> = match (e.get("childs").and_then(Value::as_array), e.get("type").and_then(Value::as_str)) {
        (Some(childs), None) | (Some(childs), Some("parallel")) => childs.clone(),
        _ => vec![e.clone()],
    };
    for t in g.stack.iter_mut().rev() {
        if let Task::Reactions(list, _) = t {
            for p in &parts {
                if let Some(k) = list.iter().rposition(|(n, x)| x.source == Some(c) && n == p) {
                    list.remove(k);
                }
            }
            return;
        }
    }
}

/// Running with the Wolves: choose one revealed card at a time within the total Hand Cost of 7
fn wolves_pick(g: &mut Game, args: &Map<String, Value>, ctx: Ctx) {
    let cards: Vec<CardId> = args.get("cards").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.as_u64().map(|v| v as CardId)).collect()).unwrap_or_default();
    let spent: i32 = cards.iter().filter(|&&c| g.card(c).zone != Zone::Limbo).map(|&c| g.card(c).def().cost_hand).sum();
    let budget = 7 - spent;
    let subtypes = list(args, "subTypes");
    let eligible: Vec<CardId> = cards
        .iter()
        .copied()
        .filter(|&c| g.card(c).zone == Zone::Limbo && g.card(c).def().cost_hand <= budget)
        .filter(|&c| subtypes.as_ref().map(|l| l.iter().any(|t| g.card(c).def().has_subtype(t))).unwrap_or(true))
        .collect();
    if eligible.is_empty() {
        return;
    }
    let effect_hand = b(args, "effectHand");
    let mut next = args.clone();
    next.remove("effectHand");
    next.insert("effectHand".into(), json!(effect_hand));
    let then = json!({"type": "seq", "childs": [internal("_playFree", json!({"effectHand": effect_hand})), internal("_wolvesPick", Value::Object(next))]});
    ask_select(g, ctx, eligible, 0, 1, then, true, &format!("You may play a revealed card for free (total Hand Cost left: {})", budget));
}

// ----------------------------------------------------------------------
// Hooks called by the rest of the engine
// ----------------------------------------------------------------------

/// The card's "once per Afternoon" ability was used (condition notUsed)
pub fn is_used(g: &Game, c: CardId) -> bool {
    g.mods.special.used.contains(&c)
}

/// Tough added by globalTough to a card (for the player targeting it)
pub fn global_tough(g: &Game, c: CardId) -> i32 {
    let card = g.card(c);
    let d = card.def();
    g.mods
        .special
        .global_tough
        .iter()
        .filter(|(seat, kind, min, _)| *seat == card.controller && kind == type_name(card.card_type()) && d.cost_hand >= *min)
        .map(|(_, _, _, t)| t)
        .sum()
}

/// nextSpellIsFree
pub fn spell_is_free(g: &Game, seat: Seat, c: CardId) -> bool {
    g.mods.special.next_spell_free[seat] && g.card(c).def().card_type == CardType::Spell
}

/// A token was just created (nextTokenAsleep)
pub fn on_token_created(g: &mut Game, c: CardId) {
    let seat = g.card(c).controller;
    if std::mem::take(&mut g.mods.special.next_token_asleep[seat]) {
        g.card_mut(c).add_status(Status::Asleep);
    }
}

/// Start of the Day: what lasts "this Day" or "this Afternoon" ends
pub fn on_morning(g: &mut Game) {
    on_turn_start(g);
    g.mods.cost_reductions.clear();
    let m = &mut g.mods.special;
    m.permanent_reductions.clear();
    m.global_tough.clear();
    m.used.clear();
    m.revealed = [None, None];
}

/// Start of a player's Afternoon turn: the "next card you play this turn" modifiers end
/// (stAssignment of the original engine), except cost reductions marked permanent
pub fn on_turn_start(g: &mut Game) {
    let m = &mut g.mods;
    let kept = m.special.permanent_reductions.clone();
    m.cost_reductions.retain(|(seat, kind, _)| kept.iter().any(|(s, k)| s == seat && k == kind));
    let left = m.cost_reductions.clone();
    m.special.permanent_reductions.retain(|(s, k)| left.iter().any(|(seat, kind, _)| s == seat && k == kind));
    m.next_character_boost = [0; 2];
    m.next_animal_boost = [0; 2];
    m.next_reserve_character_boost = [0; 2];
    m.next_character_fleeting = [false; 2];
    m.next_character_anchored = [false; 2];
    m.next_token_anchored = [false; 2];
    let sp = &mut m.special;
    sp.next_spell_free = [false; 2];
    sp.remove_fleeting = Default::default();
    sp.next_character_asleep = [false; 2];
    sp.next_character_cost3_anchored = [false; 2];
    sp.next_character_base_cost3_anchored = [false; 2];
    sp.next_character_forest_boost = [0; 2];
    sp.next_token_asleep = [false; 2];
    sp.expedition_boost = [[0; 2]; 2];
    sp.next_effects.clear();
}

/// A card was just played (ChooseAssignment): the effects that the modifiers above add to it
pub fn on_play(g: &mut Game, seat: Seat, c: CardId, from: Zone, to: Zone) -> Vec<Value> {
    let mut out = vec![];
    let card = g.card(c);
    let d = card.def();
    let char_like = card.is_character_like();
    let is_char = d.card_type == CardType::Character && !d.is_token;
    let gigantic = card.is_gigantic();
    let sp = &mut g.mods.special;
    let me_gain = |kind: &str, n: i32| json!({"action": "Gain", "args": {"cardId": "me", "type": kind, "n": n, "upTo": 99}});
    let lose_fleeting = json!({"action": "Loose", "args": {"cardId": "me", "type": "fleeting"}});
    if char_like && std::mem::take(&mut sp.next_character_asleep[seat]) {
        out.push(me_gain("asleep", 1));
    }
    if char_like && d.cost_hand <= 3 && std::mem::take(&mut sp.next_character_cost3_anchored[seat]) {
        out.push(me_gain("anchored", 1));
    }
    let base_cost = if from == Zone::Reserve { d.cost_reserve } else { d.cost_hand };
    if char_like && base_cost <= 3 && std::mem::take(&mut sp.next_character_base_cost3_anchored[seat]) {
        out.push(me_gain("anchored", 1));
    }
    // Remove Fleeting from the next spell / character
    let flags = &mut sp.remove_fleeting[seat];
    let song_artist = d.has_subtype("artist") || d.has_subtype("song");
    let wanted: &[&str] = if d.card_type == CardType::Spell {
        if from == Zone::Hand {
            &["spellHand", "spell", "songArtist"]
        } else {
            &["spell", "songArtist"]
        }
    } else if char_like {
        &["character", "characterStat0", "songArtist"]
    } else {
        &["songArtist"]
    };
    for w in wanted {
        let ok = match *w {
            "characterStat0" => d.stats.contains(&0),
            "songArtist" => song_artist,
            _ => true,
        };
        if let (true, Some(k)) = (ok, flags.iter().position(|f| f == w)) {
            flags.remove(k);
            out.push(lose_fleeting.clone());
            break;
        }
    }
    // Expedition-related boosts
    if let Some(side) = to.side() {
        let forest = |s: Side| g.region_biomes(g.players[seat].position[s.index()]).contains(&Biome::Forest);
        let in_forest = forest(side) || (gigantic && forest(side.other()));
        let sp = &mut g.mods.special;
        if is_char && sp.next_character_forest_boost[seat] > 0 && in_forest {
            out.push(me_gain("boost", std::mem::take(&mut sp.next_character_forest_boost[seat])));
        }
        let mut boost_side = side;
        if sp.expedition_boost[seat][side.index()] == 0 && gigantic && sp.expedition_boost[seat][side.other().index()] > 0 {
            boost_side = side.other();
        }
        if is_char && sp.expedition_boost[seat][boost_side.index()] > 0 {
            out.push(me_gain("boost", std::mem::take(&mut sp.expedition_boost[seat][boost_side.index()])));
        }
    }
    // Abilities the next character activates (triggerEffectOfNextCharacter)
    let from_name = match from {
        Zone::Hand => "hand",
        Zone::Reserve => "reserve",
        _ => "other",
    };
    let pending: Vec<NextEffect> = g.mods.special.next_effects.clone();
    let mut done: Vec<NextEffect> = vec![];
    for e in &pending {
        if e.seat != seat || e.kind != type_name(g.card(c).card_type()) || e.from != from_name || done.iter().any(|x| same_group(x, e)) {
            continue;
        }
        let key = format!("effect{}{}", e.effect[..1].to_uppercase(), &e.effect[1..]);
        let Some(ability) = d.effect(&key).cloned() else { continue };
        let place_ok = match e.to.as_deref() {
            None => true,
            Some("sourceLocation") => g.card(e.source).zone == to,
            Some(z) => Zone::from_engine(z) == Some(to),
        };
        if !place_ok {
            continue;
        }
        let mut parts = if e.limit1 {
            match (ability.get("childs").and_then(Value::as_array), ability.get("type").and_then(Value::as_str)) {
                (Some(childs), None) | (Some(childs), Some("parallel")) => childs.clone(),
                _ => vec![ability.clone()],
            }
        } else {
            vec![ability.clone()]
        };
        // Bravos ... (BR_30): the {R} activated this way also gives a boost
        let bonus = e.effect == "reserve"
            && g.in_play(seat).into_iter().any(|x| matches!(g.card(x).def().prop("uid").and_then(Value::as_str), Some("ALT_CORE_B_BR_30_R") | Some("ALT_CORE_B_BR_30_C")));
        let mut node = if e.limit1 {
            if bonus {
                parts.push(me_gain("boost", 1));
            }
            json!({"type": "xor", "childs": parts})
        } else {
            if bonus {
                parts.push(me_gain("boost", 1));
            }
            json!({"type": "seq", "childs": parts})
        };
        if e.boost > 0 {
            node = json!({"type": "seq", "childs": [node, me_gain("boost", e.boost)]});
        }
        out.push(node);
        done.push(e.clone());
    }
    g.mods.special.next_effects.retain(|e| !done.iter().any(|x| same_group(x, e)));
    out
}

fn same_group(a: &NextEffect, b: &NextEffect) -> bool {
    a.seat == b.seat && a.kind == b.kind && a.from == b.from && a.effect == b.effect && a.limit1 == b.limit1
}
