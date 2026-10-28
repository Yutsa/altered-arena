//! Leaf actions of effect trees (the `Actions/*.php` of the original engine).
//! Card references in args: "me" = the source card, "EFFECT" = the card of the triggering
//! event, absent = the card chosen by an enclosing Target.

use crate::catalog::{catalog, CardType};
use crate::flow::{describe, source_prefix};
use crate::game::*;
use crate::model::*;
use serde_json::{Map, Value};

pub fn i(a: &Map<String, Value>, k: &str, d: i32) -> i32 {
    a.get(k).and_then(Value::as_i64).map(|v| v as i32).unwrap_or(d)
}
pub fn s<'a>(a: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str)
}
pub fn b(a: &Map<String, Value>, k: &str) -> bool {
    a.get(k).and_then(Value::as_bool).unwrap_or(false)
}
pub fn list(a: &Map<String, Value>, k: &str) -> Option<Vec<String>> {
    match a.get(k) {
        Some(Value::Array(v)) => Some(v.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
        Some(Value::String(x)) => Some(vec![x.clone()]),
        _ => None,
    }
}

/// The card an action is about
pub fn card_arg(g: &Game, a: &Map<String, Value>, ctx: &Ctx) -> Option<CardId> {
    let event_card = ctx.event.as_ref().and_then(|e| e.card);
    let id = match a.get("cardId") {
        None | Some(Value::Null) => ctx.target.or(event_card),
        Some(Value::String(x)) => match x.as_str() {
            "me" | "source" => ctx.source,
            // Inside a Target's effect, the original engine rewrites EFFECT to the chosen card
            "EFFECT" | "effect" => ctx.target.or(event_card),
            "event" => event_card.or(ctx.target),
            "Target" => ctx.target,
            _ => x.parse().ok(),
        },
        Some(Value::Number(n)) => n.as_u64().map(|v| v as CardId),
        _ => None,
    }?;
    if (id as usize) < g.cards.len() {
        Some(id)
    } else {
        None
    }
}

/// "me" / "opponent" / "all" → seats, relative to the effect's controller
fn players_arg(ctx: &Ctx, v: Option<&str>) -> Vec<Seat> {
    match v {
        Some("opponent") | Some("nextPlayer") => vec![Game::other(ctx.seat)],
        Some("all") | Some("ALL") => vec![ctx.seat, Game::other(ctx.seat)],
        _ => vec![ctx.target_seat.unwrap_or(ctx.seat)],
    }
}

pub fn is_doable(g: &Game, action: &str, a: &Map<String, Value>, ctx: &Ctx) -> bool {
    match action {
        "Pay" | "AfterYou" => g.mana_available(ctx.seat) >= i(a, "pay", 0),
        // "Up to" targets can always be chosen (possibly none)
        "Target" | "DiscardDo" => b(a, "upTo") || !target_candidates(g, a, ctx).is_empty(),
        "Tap" => (if a.contains_key("cardId") { card_arg(g, a, ctx) } else { ctx.source }).map(|c| !g.card(c).exhausted).unwrap_or(false),
        "Discard" => {
            if let Some(src) = s(a, "source").filter(|x| !x.is_empty()) {
                Zone::from_engine(src).map(|z| !g.players[ctx.seat].zone(z).is_empty()).unwrap_or(false)
            } else {
                card_arg(g, a, ctx).map(|c| g.card(c).zone != Zone::Gone).unwrap_or(false)
            }
        }
        "Spend" => card_arg(g, a, ctx).map(|c| g.card(c).boosts + g.card(c).counter >= i(a, "n", 1)).unwrap_or(false),
        "Exchange" => !g.players[ctx.seat].hand.is_empty() && !g.players[ctx.seat].reserve.is_empty(),
        // A Rush can only be chosen when a card can be played
        "ChooseAssignment" => {
            let types = list(a, "types").unwrap_or_default().iter().filter_map(|t| match t.as_str() {
                "character" => Some(CardType::Character),
                "spell" => Some(CardType::Spell),
                "permanent" => Some(CardType::Permanent),
                _ => None,
            }).collect();
            let bonus = BonusPlay { types, free: b(a, "free"), max_base_cost: a.get("maxBaseCost").and_then(Value::as_i64).map(|v| v as i32), mandatory: false };
            !g.playable(ctx.seat, Some(&bonus)).is_empty()
        }
        // Doable when its condition holds or it has an effect for the opposite case
        "CheckCondition" => {
            crate::conditions::check_condition_action(g, a, ctx)
                || a.get("oppositeEffect").map(|o| !o.is_null() && o.as_str() != Some("OPPOSITE")).unwrap_or(false)
        }
        _ => true,
    }
}

pub fn run(g: &mut Game, action: &str, a: &Map<String, Value>, node: &Value, ctx: Ctx, optional: bool) {
    match action {
        "Gain" => gain(g, a, &ctx),
        "Loose" => loose(g, a, &ctx),
        "Draw" => {
            for seat in players_arg(&ctx, s(a, "players")) {
                let drawn = g.draw(seat, i(a, "n", 1), ctx.source);
                if !drawn.is_empty() {
                    g.emit(Event { trigger: "Draw".into(), seat, n: drawn.len() as i32, source: ctx.source, ..Default::default() });
                }
            }
        }
        "Target" | "DiscardDo" => target(g, a, ctx, optional),
        "Discard" => discard(g, a, ctx),
        "Resupply" => {
            let seats = match s(a, "player") {
                Some("opponent") | Some("nextPlayer") => vec![Game::other(ctx.seat)],
                _ => vec![ctx.target_seat.unwrap_or(ctx.seat)],
            };
            for seat in seats {
                resupply(g, seat, i(a, "n", 1), b(a, "exhausted"), ctx.source);
            }
        }
        "InvokeToken" => invoke_token(g, a, ctx, optional),
        "Exhaust" | "Tap" => {
            // Tap = exhaust the ability's own card (a cost), unless a card is given
            let card = if action == "Tap" { if a.contains_key("cardId") { card_arg(g, a, &ctx) } else { ctx.source } } else { card_arg(g, a, &ctx) };
            if let Some(c) = card {
                g.card_mut(c).exhausted = true;
                if action == "Tap" {
                    g.note_ability(ctx.seat, "tap");
                }
                g.emit(Event { trigger: "Exhaust".into(), seat: ctx.seat, card: Some(c), source: ctx.source, ..Default::default() });
            }
        }
        "Ready" => {
            // "Ready a Mana Orb" (Manaseed)
            if s(a, "cardId") == Some("mana") {
                let orb = g.players[ctx.seat].mana.iter().copied().find(|&m| g.card(m).exhausted);
                if let Some(m) = orb {
                    g.card_mut(m).exhausted = false;
                }
            } else if let Some(c) = card_arg(g, a, &ctx) {
                g.card_mut(c).exhausted = false;
            }
        }
        "Pay" => {
            let n = i(a, "pay", 0);
            if n > 0 {
                g.pay(ctx.seat, n);
            } else if let Some(max) = a.get("maximum") {
                let max = max.as_i64().unwrap_or(0) as i32;
                let values: Vec<i32> = (0..=g.mana_available(ctx.seat).min(max)).collect();
                g.ask(Pending {
                    id: 0,
                    seat: ctx.seat,
                    prompt: format!("{}How much mana do you pay?", source_prefix(g, ctx.source)),
                    source: ctx.source,
                    can_pass: false,
                    kind: PendingKind::Number { values, then: NumberThen::Pay },
                    ctx,
                });
            }
        }
        "CheckCondition" => {
            let ok = crate::conditions::check_condition_action(g, a, &ctx);
            let next = if ok { a.get("effect") } else { a.get("oppositeEffect") };
            if let Some(n) = next.filter(|n| !n.is_null() && n.as_str() != Some("OPPOSITE")) {
                g.stack.push(Task::Run { node: n.clone(), ctx });
            }
        }
        "TargetPlayer" => {
            let effect = a.get("effect").cloned();
            // opponentsOnly defaults to true in the original engine
            let opponents_only = a.get("opponentsOnly").and_then(Value::as_bool).unwrap_or(true);
            let options: Vec<Seat> = if b(a, "onlyMe") {
                vec![ctx.seat]
            } else if opponents_only {
                vec![Game::other(ctx.seat)]
            } else {
                vec![ctx.seat, Game::other(ctx.seat)]
            };
            if options.len() == 1 {
                if let Some(e) = effect {
                    let mut c = ctx.clone();
                    c.target_seat = Some(options[0]);
                    g.stack.push(Task::Run { node: e, ctx: c });
                }
            } else {
                g.ask(Pending {
                    id: 0,
                    seat: ctx.seat,
                    prompt: format!("{}Choose a player", source_prefix(g, ctx.source)),
                    source: ctx.source,
                    can_pass: optional,
                    kind: PendingKind::Player { options, effect },
                    ctx,
                });
            }
        }
        "TargetExpedition" => {
            let mut seats = match s(a, "players") {
                Some("me") => vec![ctx.seat],
                Some("opponent") => vec![Game::other(ctx.seat)],
                _ if b(a, "opponentsOnly") => vec![Game::other(ctx.seat)],
                _ => vec![ctx.seat, Game::other(ctx.seat)],
            };
            seats.dedup();
            let mut options = vec![];
            for seat in seats {
                for side in Side::BOTH {
                    if b(a, "otherThanMe") {
                        if let Some(src) = ctx.source {
                            if g.card(src).zone == side.zone() && g.card(src).controller == seat {
                                continue;
                            }
                        }
                    }
                    options.push((seat, side));
                }
            }
            let n = i(a, "n", 1).max(1) as usize;
            g.ask(Pending {
                id: 0,
                seat: ctx.seat,
                prompt: format!("{}Choose an expedition: {}", source_prefix(g, ctx.source), a.get("effect").map(|e| describe(g, e, &ctx)).unwrap_or_default()),
                source: ctx.source,
                can_pass: optional,
                kind: PendingKind::Expedition { options, min: n, max: n, effect: a.get("effect").cloned() },
                ctx,
            });
        }
        "MoveExpedition" => {
            let n = i(a, "n", 1);
            let seat = match s(a, "pId") {
                Some("opponent") => Game::other(ctx.seat),
                _ => ctx.target_seat.unwrap_or(ctx.seat),
            };
            let side = s(a, "expedition")
                .and_then(Zone::from_engine)
                .and_then(Zone::side)
                .or(ctx.side)
                .or_else(|| ctx.source.and_then(|c| g.card(c).zone.side()));
            match side {
                Some(side) => {
                    // "If it does, your other Expedition moves the other way" (Cable Car Station)
                    if g.move_expedition(seat, side, n) && b(a, "moveOtherExpedition") {
                        g.move_expedition(seat, side.other(), -n);
                    }
                }
                None => {
                    let options = vec![(seat, Side::Hero), (seat, Side::Companion)];
                    let effect = serde_json::json!({"action": "MoveExpedition", "args": {"n": n}});
                    g.ask(Pending {
                        id: 0,
                        seat: ctx.seat,
                        prompt: format!("{}Choose the expedition that moves", source_prefix(g, ctx.source)),
                        source: ctx.source,
                        can_pass: false,
                        kind: PendingKind::Expedition { options, min: 1, max: 1, effect: Some(effect) },
                        ctx,
                    });
                }
            }
        }
        "AfterYou" => {
            g.pay(ctx.seat, i(a, "pay", 0));
            g.stack.push(Task::PlayDone { turn_play: true });
        }
        "EndAfternoon" => g.players[ctx.seat].passed = true,
        "CompleteFeat" => {
            if let Some(c) = card_arg(g, a, &ctx).or(ctx.source) {
                g.card_mut(c).completed = true;
            }
        }
        "RollDie" => {
            let roll = g.rng.range(1, 6);
            g.forget_undo();
            let name = g.players[ctx.seat].name.clone();
            g.log_line(Some(ctx.seat), format!("{} rolls a {}", name, roll));
            g.emit(Event { trigger: "RollDie".into(), seat: ctx.seat, n: roll, source: ctx.source, ..Default::default() });
            if let Some(Value::Object(effects)) = a.get("effect") {
                if let Some(e) = die_effect(effects, roll) {
                    g.stack.push(Task::Run { node: e, ctx });
                }
            }
        }
        "ChooseAssignment" => {
            let types = list(a, "types")
                .unwrap_or_default()
                .iter()
                .filter_map(|t| match t.as_str() {
                    "character" => Some(CardType::Character),
                    "spell" => Some(CardType::Spell),
                    "permanent" => Some(CardType::Permanent),
                    _ => None,
                })
                .collect();
            let bonus = BonusPlay {
                types,
                free: b(a, "free"),
                max_base_cost: a.get("maxBaseCost").and_then(Value::as_i64).map(|v| v as i32),
                mandatory: b(a, "mandatory"),
            };
            g.mods.bonus_played = false;
            g.ask_turn(ctx.seat, Some(bonus));
        }
        "PlayCard" => {
            if let Some(c) = card_arg(g, a, &ctx) {
                let bonus = BonusPlay { types: vec![], free: b(a, "free"), max_base_cost: None, mandatory: false };
                let options = g.playable(ctx.seat, Some(&bonus));
                if let Some((_, dests)) = options.into_iter().find(|(id, _)| *id == c) {
                    let dest = s(a, "location").and_then(Zone::from_engine).and_then(|z| dests.iter().copied().find(|d| d.zone() == z)).unwrap_or(dests[0]);
                    let _ = g.play_card(ctx.seat, c, dest, Some(&bonus));
                }
            }
        }
        "DrawMana" => {
            for _ in 0..i(a, "n", 1) {
                if g.players[ctx.seat].deck.is_empty() {
                    break;
                }
                let id = *g.players[ctx.seat].deck.last().unwrap();
                g.move_card(id, Zone::Mana, None);
                // "as exhausted Mana Orbs" unless the effect says otherwise
                g.card_mut(id).exhausted = a.get("tapped").and_then(Value::as_bool).unwrap_or(true);
            }
            g.forget_undo();
        }
        "Exchange" => {
            let (hand, reserve) = (g.players[ctx.seat].hand.clone(), g.players[ctx.seat].reserve.clone());
            if hand.is_empty() || reserve.is_empty() {
                return;
            }
            g.ask(Pending {
                id: 0,
                seat: ctx.seat,
                prompt: format!("{}Exchange a card of your hand with a card of your Reserve", source_prefix(g, ctx.source)),
                source: ctx.source,
                can_pass: optional,
                kind: PendingKind::Exchange { hand, reserve },
                ctx,
            });
        }
        "ActivateEffect" => {
            if let Some(c) = card_arg(g, a, &ctx) {
                let kind = s(a, "effectType").unwrap_or("Played");
                if let Some(e) = g.card(c).def().effect(&format!("effect{}", kind)).cloned() {
                    let mut c2 = Ctx::of(ctx.seat, Some(c));
                    c2.event = ctx.event.clone();
                    g.stack.push(Task::Run { node: e, ctx: c2 });
                }
            }
        }
        "MoveCard" => {
            if let Some(c) = card_arg(g, a, &ctx) {
                let to = match s(a, "location").or(s(a, "destination")) {
                    None | Some("opposite") => g.card(c).zone.side().map(|side| side.other().zone()),
                    Some(other) => Zone::from_engine(other),
                };
                if let Some(z) = to {
                    let from = g.card(c).zone;
                    g.move_card(c, z, None);
                    g.emit(Event { trigger: "MoveCard".into(), seat: ctx.seat, card: Some(c), from: Some(from), to: Some(z), play: true, ..Default::default() });
                }
            }
        }
        "Spend" => {
            if let Some(c) = card_arg(g, a, &ctx) {
                let n = i(a, "n", 1);
                let card = g.card_mut(c);
                let from_boosts = n.min(card.boosts);
                card.boosts -= from_boosts;
                card.counter = (card.counter - (n - from_boosts)).max(0);
                if let Some(e) = a.get("effect").cloned() {
                    g.stack.push(Task::Run { node: e, ctx });
                }
            }
        }
        "UseCounter" => {
            if let Some(c) = ctx.source {
                let n = i(a, "consume", 1);
                let card = g.card_mut(c);
                card.counter = (card.counter - n).max(0);
            }
        }
        "SpecialEffect" => crate::special::special(g, a, node, ctx),
        "MarkRegion" => {
            // The hero's remaining terrain markers × the visible regions (key "biome:region")
            let used: Vec<Biome> = g.players[ctx.seat].records.region_markers.iter().map(|(_, b)| *b).collect();
            let markers: Vec<Biome> = [Biome::Ocean, Biome::Forest, Biome::Mountain].into_iter().filter(|b| !used.contains(b)).collect();
            let mut buttons = vec![];
            for b in &markers {
                let name = match b {
                    Biome::Forest => "Forest",
                    Biome::Mountain => "Mountain",
                    Biome::Ocean => "Ocean",
                };
                for (r, _) in g.visible_regions() {
                    buttons.push((format!("{}:{}", name.to_lowercase(), r), format!("{} marker → region {}", name, r)));
                }
            }
            if buttons.is_empty() {
                return;
            }
            g.ask(Pending {
                id: 0,
                seat: ctx.seat,
                prompt: format!("{}Mark a visible region", source_prefix(g, ctx.source)),
                source: ctx.source,
                can_pass: optional,
                kind: PendingKind::Buttons { buttons },
                ctx,
            });
        }
        other => g.unsupported(format!("action {}", other)),
    }
}

fn die_effect(effects: &Map<String, Value>, roll: i32) -> Option<Value> {
    for (k, v) in effects {
        let ok = if let Some(min) = k.strip_suffix('+') {
            min.parse::<i32>().map(|m| roll >= m).unwrap_or(false)
        } else if let Some((lo, hi)) = k.split_once('-') {
            match (lo.parse::<i32>(), hi.parse::<i32>()) {
                (Ok(lo), Ok(hi)) => roll >= lo && roll <= hi,
                _ => false,
            }
        } else {
            k.parse::<i32>().map(|x| x == roll).unwrap_or(false)
        };
        if ok {
            return Some(v.clone());
        }
    }
    None
}

fn gain(g: &mut Game, a: &Map<String, Value>, ctx: &Ctx) {
    let Some(c) = card_arg(g, a, ctx) else { return };
    if g.card(c).zone == Zone::Gone || g.card(c).zone == Zone::Discard {
        return;
    }
    let kind = if b(a, "augment") { "augment".to_string() } else { s(a, "type").unwrap_or("boost").to_string() };
    let n = match a.get("n") {
        Some(Value::Number(x)) => x.as_i64().unwrap_or(1) as i32,
        Some(Value::String(x)) => {
            g.unsupported(format!("Gain n={}", x));
            1
        }
        _ => 1,
    };
    let up_to = i(a, "upTo", 99);
    match kind.as_str() {
        "boost" => {
            let card = g.card_mut(c);
            if card.boosts >= up_to {
                return;
            }
            card.boosts = (card.boosts + n).min(up_to.max(card.boosts));
        }
        "augment" => {
            let card = g.card_mut(c);
            if card.boosts > 0 {
                card.boosts += 1;
            } else if card.counter > 0 {
                card.counter += 1;
                // The original engine reports a counter augment as a SpecialEffect gainCounter event
                let seat = g.card(c).controller;
                g.emit(Event { trigger: "SpecialEffect".into(), seat, card: Some(c), what: Some("gainCounter".into()), source: ctx.source, ..Default::default() });
            }
        }
        other => match Status::from_engine(other) {
            Some(st) => {
                if g.card(c).def().card_type == CardType::Character && st == Status::Fleeting && g.card(c).has(st) {
                    return;
                }
                g.card_mut(c).add_status(st);
            }
            None => g.unsupported(format!("Gain {}", other)),
        },
    }
    let name = g.card(c).name();
    let pname = g.players[g.card(c).controller].name.clone();
    let what = if kind == "boost" { format!("{} boost(s)", n) } else { kind.clone() };
    g.log_line(Some(ctx.seat), format!("[[{}]] ({}) gains {}", name, pname, what));
    g.emit(Event { trigger: "Gain".into(), seat: ctx.seat, card: Some(c), n, what: Some(kind), source: ctx.source, ..Default::default() });
}

fn loose(g: &mut Game, a: &Map<String, Value>, ctx: &Ctx) {
    let Some(c) = card_arg(g, a, ctx) else { return };
    let kind = s(a, "type").unwrap_or("boost");
    let n = i(a, "n", 1);
    match kind {
        "boost" => {
            let card = g.card_mut(c);
            card.boosts = (card.boosts - n).max(0);
        }
        other => {
            if let Some(st) = Status::from_engine(other) {
                g.card_mut(c).remove_status(st);
            }
        }
    }
    g.emit(Event { trigger: "Loose".into(), seat: ctx.seat, card: Some(c), n, what: Some(kind.into()), source: ctx.source, ..Default::default() });
}

/// Cards a Target action may choose, with the extra mana each costs (Tough)
pub fn target_candidates(g: &Game, a: &Map<String, Value>, ctx: &Ctx) -> Vec<(CardId, i32)> {
    let seats: Vec<Seat> = match s(a, "targetPlayer") {
        Some("me") => vec![ctx.seat],
        Some("opponent") => vec![Game::other(ctx.seat)],
        _ => vec![ctx.seat, Game::other(ctx.seat)],
    };
    let types = list(a, "targetType").unwrap_or_else(|| vec!["character".into(), "token".into()]);
    let mut zones: Vec<Zone> = vec![];
    match list(a, "targetLocation") {
        None => zones.extend([Zone::HeroExpedition, Zone::CompanionExpedition, Zone::Landmark]),
        Some(locs) => {
            for l in locs {
                match l.as_str() {
                    "source" | "opponentSource" => {
                        if let Some(z) = ctx.source.map(|c| g.card(c).zone) {
                            zones.push(z);
                        }
                    }
                    "oppositeSource" => {
                        if let Some(side) = ctx.source.and_then(|c| g.card(c).zone.side()) {
                            zones.push(side.other().zone());
                        }
                    }
                    other => {
                        if let Some(z) = Zone::from_engine(other) {
                            zones.push(z);
                        }
                    }
                }
            }
        }
    }
    let mine: Vec<CardId> = g.in_play(ctx.seat).into_iter().filter(|&c| g.card(c).is_character_like()).collect();
    let max_hand_cost: Option<i32> = match a.get("maxHandCost") {
        Some(Value::Number(n)) => n.as_i64().map(|v| v as i32),
        Some(Value::String(x)) if x == "maxCharacter" => Some(mine.iter().map(|&c| g.card(c).def().cost_hand).max().unwrap_or(-1)),
        Some(Value::String(x)) if x == "controlledCharacter" => Some(mine.len() as i32),
        _ => None,
    };
    let mut out = vec![];
    for seat in seats {
        let p = &g.players[seat];
        let mut ids: Vec<CardId> = vec![];
        for &z in &zones {
            if z == Zone::Hand && seat != ctx.seat {
                continue;
            }
            ids.extend(p.zone(z).iter().copied());
        }
        if types.iter().any(|t| t == "hero") {
            ids.extend(p.hero);
        }
        for id in ids {
            let c = g.card(id);
            let def = c.def();
            // Tokens have their printed type (a Brassbug is a character, an Aerolith a permanent)
            let type_name = match def.card_type {
                CardType::Character => "character",
                CardType::Spell => "spell",
                CardType::Permanent => "permanent",
                CardType::Token => "token",
                CardType::Hero => "hero",
            };
            if !types.iter().any(|t| t == type_name || (t == "token" && def.is_token && type_name == "character")) {
                continue;
            }
            if b(a, "excludeSelf") && Some(id) == ctx.source {
                continue;
            }
            if b(a, "excludeTokens") && def.is_token {
                continue;
            }
            if b(a, "isNotTapped") && c.exhausted {
                continue;
            }
            if b(a, "isTapped") && !c.exhausted {
                continue;
            }
            if let Some(max) = max_hand_cost {
                if def.cost_hand > max {
                    continue;
                }
            }
            if let Some(min) = a.get("minHandCost").and_then(Value::as_i64) {
                if def.cost_hand < min as i32 {
                    continue;
                }
            }
            // Base cost: Reserve cost if Fleeting, Hand cost otherwise
            if let Some(max) = a.get("maxBaseCost").and_then(Value::as_i64) {
                let base = if c.has(Status::Fleeting) { def.cost_reserve } else { def.cost_hand };
                if base > max as i32 {
                    continue;
                }
            }
            if let Some(max) = a.get("maxStatistic").and_then(Value::as_i64) {
                if c.stats().iter().any(|&v| v > max as i32) {
                    continue;
                }
            }
            if let Some(sub) = list(a, "subType") {
                if !sub.iter().any(|x| def.has_subtype(x)) {
                    continue;
                }
            }
            if let Some(statuses) = list(a, "statuses") {
                if !statuses.iter().any(|st| Status::from_engine(st).map(|st| c.has(st)).unwrap_or(st == "boost" && c.boosts > 0)) {
                    continue;
                }
            }
            if let Some(statuses) = list(a, "excludedStatuses") {
                if statuses.iter().any(|st| Status::from_engine(st).map(|st| c.has(st)).unwrap_or(false)) {
                    continue;
                }
            }
            if let Some(effects) = list(a, "hasEffects") {
                if !effects.iter().any(|e| def.effect(&format!("effect{}", e)).is_some()) {
                    continue;
                }
            }
            if b(a, "augmentOnly") && c.boosts == 0 && c.counter == 0 {
                continue;
            }
            if b(a, "noBoostIfBoosted") && c.boosts > 0 {
                continue;
            }
            let tough = if c.controller != ctx.seat && !b(a, "ignoreTough") { g.target_cost(id) } else { 0 };
            if tough > g.mana_available(ctx.seat) {
                continue;
            }
            out.push((id, tough));
        }
    }
    out
}

fn target(g: &mut Game, a: &Map<String, Value>, ctx: Ctx, optional: bool) {
    let candidates = target_candidates(g, a, &ctx);
    if candidates.is_empty() {
        return;
    }
    let n = i(a, "n", 1).max(1) as usize;
    let up_to = b(a, "upTo");
    let effect = a.get("effect").cloned();
    let max = n.min(candidates.len());
    let min = if up_to { 0 } else { max };
    let cards: Vec<CardId> = candidates.iter().map(|(c, _)| *c).collect();
    // Nothing to choose: the original engine resolves it right away
    if !up_to && !optional && cards.len() <= n && candidates.iter().all(|(_, t)| *t == 0) {
        apply_targets(g, &cards, effect.as_ref(), &ctx);
        return;
    }
    let what = effect.as_ref().map(|e| describe(g, e, &ctx)).unwrap_or_default();
    let picker = cards.iter().any(|&c| matches!(g.card(c).zone, Zone::Hand | Zone::Mana | Zone::Discard | Zone::Deck));
    g.ask(Pending {
        id: 0,
        seat: ctx.seat,
        prompt: format!("{}Choose {}{} target(s){}", source_prefix(g, ctx.source), if up_to { "up to " } else { "" }, max, if what.is_empty() { String::new() } else { format!(": {}", what) }),
        source: ctx.source,
        can_pass: optional || up_to,
        kind: PendingKind::Select {
            cards,
            min,
            max,
            costs: candidates.into_iter().filter(|(_, t)| *t > 0).collect(),
            then: SelectThen::Effect {
                effect,
                limits: (a.get("totalCost").and_then(Value::as_i64).map(|v| v as i32), a.get("totalMountain").and_then(Value::as_i64).map(|v| v as i32)),
            },
            picker,
        },
        ctx,
    });
}

pub fn apply_targets(g: &mut Game, cards: &[CardId], effect: Option<&Value>, ctx: &Ctx) {
    // A ready Mana Orb chosen as target: an exhausted one is readied in exchange (the player
    // keeps the same available mana, as in the original engine)
    for &c in cards {
        if g.card(c).zone == Zone::Mana && !g.card(c).exhausted {
            let owner = g.card(c).controller;
            if let Some(other) = g.players[owner].mana.iter().copied().find(|&m| m != c && g.card(m).exhausted) {
                g.card_mut(other).exhausted = false;
                g.card_mut(c).exhausted = true;
            }
        }
    }
    let Some(effect) = effect else { return };
    for &c in cards.iter().rev() {
        let mut c2 = ctx.clone();
        c2.target = Some(c);
        c2.target_from = Some(g.card(c).zone);
        g.stack.push(Task::Run { node: effect.clone(), ctx: c2 });
    }
}

/// Moves a card as the Discard action does, with the events cards react to
pub fn move_with_events(g: &mut Game, c: CardId, to: Zone, sacrifice: bool, exhausted: bool, ctx: &Ctx) {
    let from = g.card(c).zone;
    if from == Zone::Gone || from == Zone::Hero {
        return;
    }
    let seat = g.card(c).controller;
    let name = g.card(c).name();
    let pname = g.players[seat].name.clone();
    // What the card was before leaving (event data of the original engine)
    let (fleeting, boosts, ascended) = (g.card(c).has(Status::Fleeting), g.card(c).boosts, g.is_card_expedition_ascended(c));
    if matches!(from, Zone::Hand | Zone::Reserve) {
        g.note_ability(ctx.seat, "discardFromHandOrReserve");
        if from == Zone::Hand {
            g.note_ability(ctx.seat, "discardFromHand");
        }
    }
    if to == Zone::Deck {
        g.move_card(c, Zone::Deck, None);
    } else {
        g.move_card(c, to, None);
    }
    if exhausted {
        g.card_mut(c).exhausted = true;
    }
    let verb = match (to, sacrifice) {
        (_, true) => "sacrifices",
        (Zone::Reserve, _) => "sends to Reserve",
        (Zone::Hand, _) => "returns to hand",
        (Zone::Mana, _) => "puts in Mana",
        (Zone::Deck, _) => "puts on top of the deck",
        _ => "discards",
    };
    g.log_line(Some(seat), format!("{} {} [[{}]]", pname, verb, name));
    // The event's player is the one doing it (an opponent's card can be sabotaged)
    let ev = Event {
        trigger: "Discard".into(),
        seat: ctx.seat,
        card: Some(c),
        from: Some(from),
        to: Some(to),
        sacrifice,
        discard: true,
        listeners: vec![c],
        source: ctx.source,
        fleeting,
        boosts: Some(boosts),
        ..Default::default()
    };
    g.emit(ev.clone());
    g.emit_leave(Event { fleeting, boosts: Some(boosts), ascended, ..ev }, false);
}

fn discard(g: &mut Game, a: &Map<String, Value>, ctx: Ctx) {
    let to = s(a, "destination").and_then(Zone::from_engine).unwrap_or(Zone::Discard);
    let sacrifice = s(a, "desc") == Some("sacrifice");
    let exhausted = b(a, "tapped");
    if let Some(src) = s(a, "source").filter(|x| !x.is_empty()).and_then(Zone::from_engine) {
        let seat = ctx.seat;
        let cards = g.players[seat].zone(src).clone();
        if cards.is_empty() {
            return;
        }
        let n = match a.get("n") {
            Some(Value::Number(x)) => x.as_i64().unwrap_or(1) as usize,
            _ => 1,
        };
        let max = n.min(cards.len());
        let up_to = b(a, "upTo");
        let where_ = match to {
            Zone::Reserve => "to your Reserve",
            Zone::Mana => "to your Mana",
            Zone::Hand => "to your hand",
            _ => "to your discard pile",
        };
        g.ask(Pending {
            id: 0,
            seat,
            prompt: format!("{}Choose {} card(s) to send {}", source_prefix(g, ctx.source), max, where_),
            source: ctx.source,
            can_pass: b(a, "canPass") || up_to,
            kind: PendingKind::Select {
                cards,
                min: if up_to { 0 } else { max },
                max,
                costs: vec![],
                then: SelectThen::Move { to, exhausted, sacrifice },
                picker: false,
            },
            ctx,
        });
        return;
    }
    let Some(c) = card_arg(g, a, &ctx) else { return };
    if to == g.card(c).zone {
        return;
    }
    move_with_events(g, c, to, sacrifice, exhausted, &ctx);
}

pub fn resupply(g: &mut Game, seat: Seat, n: i32, exhausted: bool, source: Option<CardId>) {
    for _ in 0..n {
        if g.players[seat].deck.is_empty() {
            g.reshuffle_discard(seat);
        }
        let Some(&id) = g.players[seat].deck.last() else { return };
        g.move_card(id, Zone::Reserve, None);
        g.card_mut(id).exhausted = exhausted;
        g.forget_undo();
        let name = g.card(id).name();
        let pname = g.players[seat].name.clone();
        g.log_line(Some(seat), format!("{} resupplies [[{}]]", pname, name));
        g.emit(Event { trigger: "Resupply".into(), seat, card: Some(id), to: Some(Zone::Reserve), source, ..Default::default() });
    }
}

fn invoke_token(g: &mut Game, a: &Map<String, Value>, ctx: Ctx, optional: bool) {
    let Some(token) = s(a, "tokenType").and_then(|r| catalog().get(r)) else {
        g.unsupported(format!("token {:?}", s(a, "tokenType")));
        return;
    };
    let n = match a.get("n") {
        Some(Value::String(x)) if x == "paidMana" => g.mods.last_paid,
        Some(Value::String(x)) if x == "landmarks3" => (g.players[ctx.seat].landmarks.len() as i32).min(3),
        _ => i(a, "n", 1).max(1),
    };
    if n <= 0 {
        return;
    }
    let owner = match (s(a, "targetPlayer"), s(a, "pId")) {
        (Some("opponent"), _) | (_, Some("opponent")) => Game::other(ctx.seat),
        _ => ctx.seat,
    };
    let locs = list(a, "targetLocation").unwrap_or_else(|| vec!["stormLeft".into(), "stormRight".into()]);
    let all_players = b(a, "allPlayers") || (a.get("targetPlayer").is_none() && locs.len() > 1);
    let mut options: Vec<(String, String, Seat, Zone)> = vec![];
    let seats = if all_players { vec![ctx.seat, Game::other(ctx.seat)] } else { vec![owner] };
    // In the expedition chosen by an enclosing TargetExpedition
    if let (Some(side), true) = (ctx.side, a.get("targetLocation").is_none()) {
        let seat = ctx.target_seat.unwrap_or(owner);
        for _ in 0..n {
            create_token(g, token.index, seat, side.zone(), &ctx);
        }
        return;
    }
    // Where the source card is, or was when it left (reactions to leaving an expedition)
    let event_from = ctx.event.as_ref().filter(|e| e.card == ctx.source).and_then(|e| e.from).filter(|z| z.is_expedition());
    for &seat in &seats {
        for l in &locs {
            let zone = match l.as_str() {
                "source" | "initialSource" => ctx.source.map(|c| g.card(c).zone).filter(|z| z.is_expedition()).or(event_from).or(ctx.target_from.filter(|z| z.is_expedition())),
                "oppositeSource" => ctx.source.and_then(|c| g.card(c).zone.side()).map(|s| s.other().zone()),
                other => Zone::from_engine(other),
            };
            let Some(zone) = zone else { continue };
            let label = format!(
                "{}{}",
                if seats.len() > 1 { format!("{} — ", g.players[seat].name) } else { String::new() },
                match zone {
                    Zone::HeroExpedition => "Hero expedition",
                    Zone::CompanionExpedition => "Companion expedition",
                    Zone::Landmark => "Landmarks",
                    _ => "?",
                }
            );
            options.push((format!("{}-{}", seat, zone_key(zone)), label, seat, zone));
        }
    }
    options.dedup_by(|x, y| x.0 == y.0);
    if options.is_empty() {
        return;
    }
    if options.len() == 1 && !optional {
        let (_, _, seat, zone) = options[0].clone();
        for _ in 0..n {
            create_token(g, token.index, seat, zone, &ctx);
        }
        return;
    }
    g.ask(Pending {
        id: 0,
        seat: ctx.seat,
        prompt: format!("{}Where do you create the {} token?", source_prefix(g, ctx.source), token.name),
        source: ctx.source,
        can_pass: optional,
        kind: PendingKind::Location { options, token: token.reference.clone(), n },
        ctx,
    });
}

pub fn zone_key(z: Zone) -> &'static str {
    match z {
        Zone::HeroExpedition => "stormLeft",
        Zone::CompanionExpedition => "stormRight",
        Zone::Landmark => "landmark",
        _ => "other",
    }
}

pub fn create_token(g: &mut Game, def: u16, seat: Seat, zone: Zone, ctx: &Ctx) -> CardId {
    let id = g.create_card(def, seat, zone);
    // "The next token gains Anchored" follows the effect's controller, wherever the token goes
    for s in [ctx.seat, seat] {
        if g.mods.next_token_anchored[s] {
            g.card_mut(id).add_status(Status::Anchored);
            g.mods.next_token_anchored[s] = false;
            break;
        }
    }
    crate::special::on_token_created(g, id);
    let name = g.card(id).name();
    let pname = g.players[seat].name.clone();
    g.log_line(Some(seat), format!("A [[{}]] token is created for {}", name, pname));
    g.emit(Event {
        trigger: "InvokeToken".into(),
        seat,
        card: Some(id),
        to: Some(zone),
        play: true,
        put: true,
        listeners: vec![id],
        source: ctx.source,
        ..Default::default()
    });
    id
}
