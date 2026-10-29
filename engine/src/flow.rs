//! Interpreter of effect trees, in the format of the original engine's card data:
//!   leaf:  {"action": "Gain", "args": {...}, "optional"?: true, "pId"?: "opponent"}
//!   inner: {"type": "seq" | "xor" | "or" | "parallel", "childs": [...], "optional"?: true}
//! A seq runs its children in order; xor / or let the player pick one child; parallel runs all.

use crate::game::*;
use crate::model::*;
use serde_json::{Map, Value};

/// Actions that ask the player something themselves (an optional one shows a "pass" option)
const DECIDING: &[&str] = &["MarkRegion", "Target", "DiscardDo", "TargetExpedition", "TargetPlayer", "Exchange", "ChooseAssignment", "RollDie", "InvokeToken", "Pay"];

pub fn args(node: &Value) -> Map<String, Value> {
    node.get("args").and_then(Value::as_object).cloned().unwrap_or_default()
}

fn is_optional(node: &Value) -> bool {
    node.get("optional").and_then(Value::as_bool).unwrap_or(false)
}

fn without_optional(node: &Value) -> Value {
    let mut n = node.clone();
    if let Value::Object(m) = &mut n {
        m.remove("optional");
    }
    n
}

/// Player a node is about ("pId" of the original data)
fn adjust_ctx(g: &Game, node: &Value, mut ctx: Ctx) -> Ctx {
    match node.get("pId") {
        Some(Value::String(s)) => match s.as_str() {
            "opponent" | "nextPlayer" => ctx.seat = Game::other(ctx.seat),
            "owner" => {
                if let Some(t) = ctx.target {
                    ctx.seat = g.card(t).owner;
                }
            }
            "source" => {
                if let Some(s) = ctx.source {
                    ctx.seat = g.card(s).controller;
                }
            }
            "active" => ctx.seat = g.active,
            _ => {}
        },
        Some(Value::Number(n)) => {
            if let Some(seat) = n.as_u64().and_then(|id| g.seat_of(id as u32)) {
                ctx.seat = seat;
            }
        }
        _ => {}
    }
    ctx
}

pub fn run_node(g: &mut Game, node: &Value, ctx: Ctx) {
    let mut ctx = adjust_ctx(g, node, ctx);
    // Nodes built by the engine itself name the card they come from
    if let Some(id) = node.get("sourceId").and_then(Value::as_u64) {
        ctx.source = Some(id as CardId);
    }
    if let Some(childs) = node.get("childs").and_then(Value::as_array) {
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("seq");
        if childs.is_empty() {
            return;
        }
        if is_optional(node) {
            // Asked as soon as its first step can be done (the original engine checks no further)
            if !is_doable(g, &childs[0], &ctx) {
                return;
            }
            ask_yes_no(g, node, ctx);
            return;
        }
        match kind {
            "xor" | "or" => {
                let options: Vec<ChoiceOpt> = childs
                    .iter()
                    .map(|c| ChoiceOpt { label: describe(g, c, &ctx), node: Some(c.clone()), enabled: is_doable(g, c, &ctx), source: None })
                    .collect();
                let enabled: Vec<&ChoiceOpt> = options.iter().filter(|o| o.enabled).collect();
                if enabled.is_empty() {
                    return;
                }
                // The original engine only decides alone when there is a single option at all
                if enabled.len() == 1 && options.len() == 1 {
                    let n = enabled[0].node.clone().unwrap();
                    g.stack.push(Task::Run { node: n, ctx });
                    return;
                }
                let source = ctx.source;
                g.ask(Pending {
                    id: 0,
                    seat: ctx.seat,
                    prompt: format!("{}Choose one", source_prefix(g, source)),
                    source,
                    can_pass: false,
                    kind: PendingKind::Choice { options },
                    ctx,
                });
            }
            _ => {
                // Rush: what follows "play a card" in a sequence happens only if a card was played
                let rush = childs.iter().position(|c| c.get("action").and_then(Value::as_str) == Some("ChooseAssignment"));
                let (now, after) = match rush {
                    Some(k) if kind == "seq" && k + 1 < childs.len() => (&childs[..=k], Some(&childs[k + 1..])),
                    _ => (&childs[..], None),
                };
                if let Some(after) = after {
                    let rest = serde_json::json!({"type": "seq", "childs": after});
                    g.stack.push(Task::IfBonusPlayed { node: rest, ctx: ctx.clone() });
                }
                for c in now.iter().rev() {
                    g.stack.push(Task::Run { node: c.clone(), ctx: ctx.clone() });
                }
            }
        }
        return;
    }
    let Some(action) = node.get("action").and_then(Value::as_str) else { return };
    if is_optional(node) && !DECIDING.contains(&action) {
        if !is_doable(g, node, &ctx) {
            return;
        }
        ask_yes_no(g, node, ctx);
        return;
    }
    crate::actions::run(g, action, &args(node), node, ctx, is_optional(node));
}

fn ask_yes_no(g: &mut Game, node: &Value, ctx: Ctx) {
    let label = describe(g, node, &ctx);
    let source = ctx.source;
    g.ask(Pending {
        id: 0,
        seat: ctx.seat,
        prompt: format!("{}{}?", source_prefix(g, source), label),
        source,
        can_pass: false,
        kind: PendingKind::Choice {
            options: vec![
                ChoiceOpt { label, node: Some(without_optional(node)), enabled: true, source: None },
                ChoiceOpt { label: "Skip".into(), node: None, enabled: true, source: None },
            ],
        },
        ctx,
    });
}

pub fn source_prefix(g: &Game, source: Option<CardId>) -> String {
    source.map(|s| format!("[[{}]]: ", g.card(s).name())).unwrap_or_default()
}

/// Can this effect be done now (costs payable, targets available...)?
pub fn is_doable(g: &Game, node: &Value, ctx: &Ctx) -> bool {
    let ctx = adjust_ctx(g, node, ctx.clone());
    if let Some(childs) = node.get("childs").and_then(Value::as_array) {
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("seq");
        return match kind {
            "seq" => childs.iter().all(|c| is_optional(c) || is_doable(g, c, &ctx)),
            _ => childs.iter().any(|c| is_doable(g, c, &ctx)),
        };
    }
    let Some(action) = node.get("action").and_then(Value::as_str) else { return true };
    crate::actions::is_doable(g, action, &args(node), &ctx)
}

/// Short English description of an effect, for choices and logs
pub fn describe(g: &Game, node: &Value, ctx: &Ctx) -> String {
    if let Some(d) = node.get("customDescription").and_then(Value::as_str) {
        return d.replace("${eventCardName}", "it");
    }
    if let Some(childs) = node.get("childs").and_then(Value::as_array) {
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("seq");
        let parts: Vec<String> = childs.iter().map(|c| describe(g, c, ctx)).filter(|s| !s.is_empty()).collect();
        return parts.join(if kind == "seq" { ", then " } else { " or " });
    }
    let a = args(node);
    let action = node.get("action").and_then(Value::as_str).unwrap_or("");
    let n = a.get("n").and_then(Value::as_i64).unwrap_or(1);
    let who = match a.get("cardId").and_then(Value::as_str) {
        Some("me") => ctx.source.map(|s| format!(" [[{}]]", g.card(s).name())).unwrap_or_default(),
        _ => String::new(),
    };
    match action {
        "Gain" => {
            let t = a.get("type").and_then(Value::as_str).unwrap_or("boost");
            if t == "boost" {
                format!("Give{} {} boost(s)", who, n)
            } else {
                format!("Give{} {}", who, capitalize(t))
            }
        }
        "Loose" => format!("Remove {} from{}", a.get("type").and_then(Value::as_str).unwrap_or("boost"), who),
        "Draw" => format!("Draw {} card(s)", n),
        "Target" | "DiscardDo" => {
            let effect = a.get("effect").map(|e| describe(g, e, ctx)).unwrap_or_default();
            if effect.is_empty() {
                "Choose a target".into()
            } else {
                format!("Choose a target: {}", effect)
            }
        }
        "Discard" => {
            let dest = a.get("destination").and_then(Value::as_str).unwrap_or("discard");
            let desc = a.get("desc").and_then(Value::as_str).unwrap_or("");
            let verb = match (dest, desc) {
                (_, "sacrifice") => "Sacrifice",
                ("reserve", _) => "Send to Reserve",
                ("hand", _) => "Return to hand",
                ("mana", _) => "Put in Mana",
                ("topOfDeck", _) => "Put on top of the deck",
                _ => "Discard",
            };
            format!("{}{}", verb, who)
        }
        "InvokeToken" => {
            let t = a.get("tokenType").and_then(Value::as_str).and_then(|r| crate::catalog::catalog().get(r)).map(|d| d.name.as_str()).unwrap_or("token");
            format!("Create a {} token", t)
        }
        "Resupply" => "Resupply".into(),
        "Exhaust" => format!("Exhaust{}", who),
        "Ready" => format!("Ready{}", who),
        "Tap" => format!("Exhaust{}", who),
        "Pay" => format!("Pay {} mana", a.get("pay").and_then(Value::as_i64).unwrap_or(1)),
        "TargetPlayer" => "Choose a player".into(),
        "TargetExpedition" => "Choose an expedition".into(),
        "MoveExpedition" => "Move an expedition forward".into(),
        "RollDie" => "Roll a die".into(),
        "ChooseAssignment" | "PlayCard" => "Play a card".into(),
        "DrawMana" => "Put the top card of your deck in Mana".into(),
        "AfterYou" => "Pass to your opponent (After You)".into(),
        "Exchange" => "Exchange a card of your hand with your Reserve".into(),
        "CheckCondition" => match a.get("description").and_then(Value::as_str) {
            Some(d) => d.to_string(),
            None => a.get("effect").map(|e| describe(g, e, ctx)).unwrap_or_default(),
        },
        "SpecialEffect" => a.get("effect").and_then(Value::as_str).map(humanize).unwrap_or_default(),
        other => humanize(other),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn humanize(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else if i == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// "Independent" effects need no decision and can resolve in any order: among simultaneous
/// effects, the original engine resolves them first without asking (Gain, CheckCondition...)
pub fn is_independent(g: &Game, node: &Value, ctx: &Ctx) -> bool {
    if is_optional(node) || node.get("noIndependent").and_then(Value::as_bool).unwrap_or(false) {
        return false;
    }
    if let Some(childs) = node.get("childs").and_then(Value::as_array) {
        let kind = node.get("type").and_then(Value::as_str).unwrap_or("seq");
        if kind != "seq" && kind != "parallel" && childs.len() > 1 {
            return false;
        }
        return childs.iter().all(|c| is_independent(g, c, ctx));
    }
    let a = args(node);
    match node.get("action").and_then(Value::as_str).unwrap_or("") {
        "Gain" | "CheckCondition" | "ActivateEffect" | "SpellCleanup" => true,
        "Loose" => !a.get("upTo").and_then(Value::as_bool).unwrap_or(false),
        "InvokeToken" => {
            ctx.side.is_some()
                || (a.get("targetLocation").and_then(Value::as_array).map(|l| l.len() == 1).unwrap_or(false)
                    && !a.get("allPlayers").and_then(Value::as_bool).unwrap_or(false))
        }
        _ => false,
    }
}
