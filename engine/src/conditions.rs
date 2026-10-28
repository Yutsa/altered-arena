//! Conditions of triggered abilities and of CheckCondition ("name:arg1:arg2" strings of the
//! original engine). `card` is the card whose ability is checked; the event is in the context.
//! Unknown conditions are false (an ability never fires by mistake) and listed by `KNOWN`.

use crate::catalog::CardType;
use crate::game::{Ctx, Game};
use crate::model::*;
use serde_json::Value;

/// Conditions implemented here (for coverage reports)
pub const KNOWN: &[&str] = &[
    "isMe", "isNotMe", "hasSameOwner", "hasControl", "isCardAddedAnyPlayer", "notTapped", "isStillSameLocation",
    "isFirstPlayer", "isNotFirstPlayer", "isCardPlayed", "isCardAdded", "isInStorms", "isSacrificed", "isSacrifice",
    "isOpponentDraw", "isAfternoon", "hasCounterOnCard", "isDiscarded", "isSourceSameLocation", "isToReserve",
    "isNotFirstTurn", "isThisFeatCompleted", "isThisFeatIncomplete", "hasBoost", "hasNoBoost", "isFromReserve",
    "realResupply", "canSacrifice", "hasDiscardPileCards", "isCardOfType", "isMyTurn", "isNotToken", "isSupportEffect",
    "isMyselfDiscarded", "hasDeckCards", "checkReserveCards", "isPlayedInSameLocation", "isPlayedInOpponentExpedition",
    "excludeSelf", "hasCardsInHand", "hasReserve", "isReallyPlayed", "hasCardInDiscardPile", "notUsed", "canPay",
    "isNotGigantic", "isOpponentExpeditionNotEmpty", "isFirstPassing", "myExpeditionHasMoved", "myExpeditionHasNotMoved",
    "isSelfPlayCardEvent", "isNotMeInvoke", "isCharacterBoostedAndUntap",
];

thread_local! {
    static UNKNOWN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Unknown conditions met since the last call (reported as unsupported by the game)
pub fn take_unknown() -> Vec<String> {
    UNKNOWN.with(|u| std::mem::take(&mut *u.borrow_mut()))
}

pub fn check_power(g: &Game, power: &Value, ctx: &Ctx) -> bool {
    let mut conds: Vec<&str> = vec![];
    if let Some(list) = power.get("listeningConditions").and_then(Value::as_array) {
        conds.extend(list.iter().filter_map(Value::as_str));
    }
    if let Some(list) = power.get("conditions").and_then(Value::as_array) {
        conds.extend(list.iter().filter_map(Value::as_str));
    }
    match power.get("condition") {
        Some(Value::String(s)) => conds.push(s),
        Some(Value::Array(list)) => conds.extend(list.iter().filter_map(Value::as_str)),
        _ => {}
    }
    conds.into_iter().all(|c| check(g, c, ctx))
}

fn cmp(value: i32, n: i32, op: &str) -> bool {
    match op {
        "LTE" => value <= n,
        "EQ" | "E" => value == n,
        "LT" => value < n,
        "GT" => value > n,
        _ => value >= n,
    }
}

/// typeCheck of the original engine
fn type_matches(kind: &str, card: &Card) -> bool {
    let t = card.def().card_type;
    let token = card.def().is_token;
    match kind {
        "" => true,
        "character" => matches!(t, CardType::Character | CardType::Token) || token,
        "characterOnly" => t == CardType::Character && !token,
        "token" => token,
        "permanent" => t == CardType::Permanent,
        "spell" => t == CardType::Spell,
        "all" | "hero" => true,
        sub => card.def().has_subtype(sub),
    }
}

pub fn check(g: &Game, cond: &str, ctx: &Ctx) -> bool {
    let parts: Vec<&str> = cond.split(':').collect();
    let name = parts[0];
    let arg = |i: usize| parts.get(i).copied().unwrap_or("");
    let num = |i: usize, d: i32| parts.get(i).and_then(|s| s.parse::<i32>().ok()).unwrap_or(d);
    let Some(me_id) = ctx.source else { return false };
    let me = g.card(me_id);
    let owner = me.controller;
    let default_event = Default::default();
    let ev = ctx.event.as_deref().unwrap_or(&default_event);
    let ev_card = ev.card.map(|c| g.card(c));
    let is_me = ev.seat == owner && ctx.event.is_some();

    match name {
        "isMe" => is_me,
        "isNotMe" => ctx.event.is_some() && ev.seat != owner,
        "hasSameOwner" => ev_card.map(|c| c.owner == me.owner).unwrap_or(false),
        "notTapped" => !me.exhausted,
        "isStillSameLocation" => ev_card.map(|c| Some(c.zone) == ev.to).unwrap_or(false),
        "isFirstPlayer" => is_me && owner == g.first,
        "isNotFirstPlayer" => is_me && owner != g.first,
        "isCardPlayed" | "isCardAdded" | "isCardAddedAnyPlayer" => {
            if !ev.play || (name == "isCardPlayed" && ev.put) {
                return false;
            }
            let Some(c) = ev_card else { return false };
            if name == "isCardAddedAnyPlayer" {
                if c.controller != owner {
                    return false;
                }
            } else if !is_me {
                return false;
            }
            let (kind, cost, op, exclude) = (arg(1), arg(2), arg(3), arg(4));
            if exclude == "true" && c.id == me_id {
                return false;
            }
            if !type_matches(kind, c) {
                return false;
            }
            if !cost.is_empty() {
                let cost: i32 = cost.parse().unwrap_or(0);
                if !cmp(c.def().cost_hand, cost, if op.is_empty() { "GTE" } else { op }) {
                    return false;
                }
            }
            true
        }
        "isCardOfType" => ev_card.map(|c| type_matches(arg(1), c)).unwrap_or(false),
        "isInStorms" => me.zone.is_expedition(),
        "isSacrifice" => ev.sacrifice && (arg(1).is_empty() || ev_card.map(|c| type_matches(arg(1), c)).unwrap_or(false)),
        "isSacrificed" => ev.sacrifice && ev.discard && ev.card == Some(me_id),
        "isMyselfDiscarded" => ev.discard && ev.card == Some(me_id) && discarded_matches(ev, arg(1), arg(2)),
        "isDiscarded" => ev.discard && discarded_matches(ev, arg(1), arg(2)) && (arg(3).is_empty() || ev_card.map(|c| type_matches(arg(3), c)).unwrap_or(false)),
        "isOpponentDraw" => ctx.event.is_some() && ev.seat != owner && ev.to != Some(Zone::Mana),
        "isAfternoon" => g.phase == Phase::Afternoon,
        "hasCounterOnCard" => cmp(me.counter, num(1, 1), arg(2)),
        "hasBoost" => cmp(me.boosts, num(1, 1), arg(2)),
        "hasNoBoost" => me.boosts <= 0,
        "isSourceSameLocation" => ev.from.map(|z| z == me.zone).unwrap_or(false),
        "isToReserve" => ev.to == Some(Zone::Reserve),
        "isFromReserve" => ev.from == Some(Zone::Reserve),
        "isNotFirstTurn" => g.day != 1,
        "isThisFeatCompleted" => me.completed,
        "isThisFeatIncomplete" => !me.completed,
        "realResupply" => true,
        "canSacrifice" => g.in_play(owner).iter().any(|&c| g.card(c).is_character_like()),
        "hasDiscardPileCards" => cmp(g.players[owner].discard.len() as i32, num(1, 1), arg(2)),
        "hasCardInDiscardPile" => !g.players[owner].discard.is_empty(),
        "hasDeckCards" => !g.players[owner].deck.is_empty(),
        "checkReserveCards" => cmp(g.players[owner].reserve.len() as i32, num(1, 1), arg(2)),
        "hasReserve" => !g.players[owner].reserve.is_empty(),
        "hasCardsInHand" => !g.players[owner].hand.is_empty(),
        "isMyTurn" => g.phase == Phase::Afternoon && g.active == owner,
        "isNotToken" => ev_card.map(|c| !c.def().is_token).unwrap_or(true),
        "isSupportEffect" => ev.support,
        "isPlayedInSameLocation" => ev_card.map(|c| c.controller == owner && (Some(me.zone) == ev.to || c.is_gigantic() || me.is_gigantic())).unwrap_or(false),
        "isPlayedInOpponentExpedition" => ev_card.map(|c| c.controller != owner && ev.to.map(|t| t.is_expedition() && t == me.zone).unwrap_or(false)).unwrap_or(false),
        "excludeSelf" => ev.card != Some(me_id),
        "isReallyPlayed" => ev.play && !ev.put,
        "isSelfPlayCardEvent" => ev.play && ev.card == Some(me_id),
        "isNotMeInvoke" => ev.card != Some(me_id),
        "notUsed" => !me.exhausted,
        "canPay" => g.mana_available(owner) >= num(1, 1),
        "isNotGigantic" => ev_card.map(|c| !c.is_gigantic()).unwrap_or(true),
        "isOpponentExpeditionNotEmpty" => {
            let opp = Game::other(owner);
            !g.players[opp].expeditions[0].is_empty() || !g.players[opp].expeditions[1].is_empty()
        }
        "isFirstPassing" => false,
        "myExpeditionHasMoved" => me.zone.side().map(|s| g.players[owner].moved[s.index()]).unwrap_or(false),
        "myExpeditionHasNotMoved" => me.zone.side().map(|s| !g.players[owner].moved[s.index()]).unwrap_or(false),
        "hasControl" => {
            // hasControl:type:n[:excludeMyself[:state[:op[:opponent]]]]
            let kind = arg(1);
            let n = num(2, 1);
            let exclude = arg(3) == "true";
            let op = if arg(5).is_empty() { "GTE" } else { arg(5) };
            let seat = if arg(6) == "true" { Game::other(owner) } else { owner };
            let count = g
                .in_play(seat)
                .iter()
                .filter(|&&c| !(exclude && c == me_id))
                .filter(|&&c| {
                    let card = g.card(c);
                    match kind {
                        "character" => card.is_character_like(),
                        "permanent" => card.def().card_type == CardType::Permanent,
                        "token" => card.def().is_token,
                        sub => card.def().has_subtype(sub) || sub.split('|').any(|s| card.def().has_subtype(s)),
                    }
                })
                .count() as i32;
            cmp(count, n, op)
        }
        "isCharacterBoostedAndUntap" => {
            !me.exhausted && ev.what.as_deref() == Some("boost") && ev_card.map(|c| c.controller == owner && c.is_character_like()).unwrap_or(false)
        }
        _ => {
            UNKNOWN.with(|u| u.borrow_mut().push(format!("condition {}", name)));
            false
        }
    }
}

fn discarded_matches(ev: &crate::game::Event, from: &str, to: &str) -> bool {
    let from_ok = from.is_empty() || ev.from == Zone::from_engine(from);
    let to_ok = to.is_empty() || ev.to == Zone::from_engine(to);
    from_ok && to_ok
}
