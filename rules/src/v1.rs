//! server/src/V1.php: protocol v1 decisions on top of the engine, answers turned into the
//! engine's actions (through Arena::act), and Arena::oracleState.
//!
//! The GameView itself (V1::build: cards, players, regions, log) is not built: it needs
//! `getAllDatas()`, whose storm and biome computations are not translated yet. Only the parts
//! the decisions read are computed (`undo`, the player ids).

use crate::core::globals;
use crate::framework::{self, StateDef};
use crate::game::{Game, PId};
use crate::game_methods;
use crate::managers::{cards, meeples, players};
use crate::models::card;
use crate::php;
use crate::states;
use crate::{PhpError, R};
use serde_json::{json, Map, Value};

const DESTINATIONS: &[(&str, &str)] = &[
    ("stormLeft", "heroExpedition"),
    ("stormRight", "companionExpedition"),
    ("landmark", "landmark"),
    ("reserve", "reserve"),
    ("limbo", "spell"),
    ("stormLeft_scout", "heroExpeditionScout"),
    ("stormRight_scout", "companionExpeditionScout"),
];

fn destination(l: &Value) -> Value {
    let s = php::strval(l);
    match DESTINATIONS.iter().find(|(k, _)| *k == s) {
        Some((_, v)) => json!(v),
        None => l.clone(),
    }
}

/// `Arena::privateArgs($args, $playerId)`: BGA sends each player only their `_private` entry
pub fn private_args(args: &Value, pid: PId) -> Value {
    if !php::is_arr(args) {
        return args.clone();
    }
    let mut args = args.clone();
    if php::key_exists(&args, "_private") {
        let private = php::get(&args, "_private").clone();
        if php::is_arr(&private) && php::key_exists(&private, &pid.to_string()) {
            php::set(&mut args, "_private", php::get(&private, &pid.to_string()).clone());
        } else {
            php::unset(&mut args, "_private");
        }
    }
    args
}

/// The state as `Arena::view` gives it to a player: name, type, args (private part resolved,
/// JSON round trip), active player, multiactive players, whether the player is active
pub struct RawState {
    pub def: Option<&'static StateDef>,
    pub args: Value,
    pub active_player: PId,
    pub multiactive: Vec<PId>,
    pub is_active: bool,
}

/// `Arena::view($gameId, $playerId)` (state part), run as a request of the player
pub fn view_state(g: &mut Game, pid: PId) -> R<RawState> {
    g.begin_request(Some(pid))?;
    let (def, args) = framework::state_with_args(g)?;
    let args = php::round_trip(&private_args(&args, pid));
    Ok(RawState { def, args, active_player: g.active_player_id(), multiactive: framework::get_active_player_list(g), is_active: framework::is_player_active(g, pid) })
}

/// `$waiting` of V1::build
pub fn waiting_for(g: &Game) -> Vec<PId> {
    match framework::state(g) {
        Some(s) if s.kind == "multipleactiveplayer" => framework::get_active_player_list(g),
        Some(s) if s.kind == "activeplayer" => vec![g.active_player_id()],
        _ => vec![],
    }
}

fn ints(v: &Value) -> Value {
    Value::Array(php::vals(v).into_iter().map(|x| json!(php::intval(x))).collect())
}

/// `V1::decision($state, $g, $you, $seq)`
// PHP: server/src/V1.php:265
pub fn decision(g: &mut Game, raw: &RawState, you: PId) -> R<Option<Value>> {
    if !raw.is_active {
        return Ok(None);
    }
    let Some(def) = raw.def else { return Ok(Some(json!({"id": format!("{}:", g.notif_seq), "prompt": "", "kind": "buttons", "buttons": []}))) };
    let args = &raw.args;
    let private = php::coalesce(php::get(args, "_private"), &php::arr()).clone();
    let name = def.name;
    let mut base = Map::new();
    base.insert("id".into(), json!(format!("{}:{}", g.notif_seq, name)));
    base.insert("prompt".into(), json!(prompt(def, args)));
    if !php::empty(php::get(args, "sourceId")) {
        base.insert("sourceCardId".into(), json!(php::intval(php::get(args, "sourceId"))));
    }
    if php::get(args, "optionalAction") == &json!(true) {
        base.insert("canPass".into(), json!(true));
    }
    let undo = php::truthy(&globals::get(g, "undo")?);
    if php::gt(php::coalesce(php::get(args, "previousEngineChoices"), &json!(0)), &json!(0)) && undo {
        base.insert("canUndo".into(), json!(true));
    }
    let with = |base: &Map<String, Value>, extra: Value| -> Value {
        // `$base + [...]`: keys of $base win
        let mut m = base.clone();
        if let Value::Object(e) = extra {
            for (k, v) in e {
                m.entry(k).or_insert(v);
            }
        }
        Value::Object(m)
    };
    let d = match name {
        "selectPrecoDeck" => {
            if !php::get(&private, "selection").is_null() {
                return Ok(None);
            }
            return Err(PhpError::nt("server/src/V1.php:288 selectPrecoDeck decision (protocol/data/decks.json)"));
        }
        "firstDayManaSelection" => {
            if !php::empty(php::get(&private, "selection")) {
                return Ok(None);
            }
            let n = php::intval(php::coalesce(php::get(&private, "n"), &json!(3)));
            let mut b = base.clone();
            b.insert("prompt".into(), json!(format!("Choose {} cards to put in your Mana", n)));
            with(&b, json!({"kind": "select", "cardIds": php::coalesce(php::get(&private, "cards"), &php::arr()), "min": n, "max": n}))
        }
        "chooseAssignment" => {
            let mut play = vec![];
            for (cid, locs) in php::entries(php::coalesce(php::get(&private, "play"), &php::arr())) {
                let dests: Vec<Value> = php::vals(locs).into_iter().map(destination).collect();
                play.push(json!({"cardId": php::intval(&json!(cid)), "destinations": dests}));
            }
            with(
                &base,
                json!({
                    "kind": "turn",
                    "play": play,
                    "support": ints(php::coalesce(php::get(&private, "support"), &php::arr())),
                    "tap": ints(php::coalesce(php::get(&private, "tap"), &php::arr())),
                    "canEndTurn": php::loose_eq(php::coalesce(php::get(args, "additionalAction"), &json!(false)), &json!(false)),
                }),
            )
        }
        "playCard" => {
            let cid = php::intval(php::get(args, "cardId"));
            let locs = php::coalesce(php::get(php::get(&private, "play"), &cid.to_string()), &php::arr()).clone();
            let dests: Vec<Value> = php::vals(&locs).into_iter().map(destination).collect();
            with(&base, json!({"kind": "turn", "play": [{"cardId": cid, "destinations": dests}], "support": [], "tap": [], "canEndTurn": false}))
        }
        "confirmTurn" | "confirmPartialTurn" => {
            let mut b = base.clone();
            b.insert("prompt".into(), json!("Confirm your turn"));
            with(&b, json!({"kind": "confirm"}))
        }
        "resolveChoice" => {
            let mut choices: Vec<(i64, Value)> = vec![];
            for (id, c) in php::entries(php::coalesce(php::get(args, "allChoices"), &php::arr())) {
                let iid = php::intval(&json!(id));
                let mut e = json!({"id": iid, "label": label(c), "enabled": php::isset(php::get(args, "choices"), &id)});
                if !php::empty(php::get(c, "sourceId")) {
                    php::set(&mut e, "sourceCardId", json!(php::intval(php::get(c, "sourceId"))));
                }
                match choices.iter_mut().find(|(k, _)| *k == iid) {
                    Some(x) => x.1 = e,
                    None => choices.push((iid, e)),
                }
            }
            for (id, c) in php::entries(php::coalesce(php::get(args, "choices"), &php::arr())) {
                let iid = php::intval(&json!(id));
                if !choices.iter().any(|(k, _)| *k == iid) {
                    choices.push((iid, json!({"id": iid, "label": label(c), "enabled": true})));
                }
            }
            with(&base, json!({"kind": "choice", "choices": choices.into_iter().map(|(_, v)| v).collect::<Vec<_>>()}))
        }
        "target" | "discardDo" => {
            let ids = ints(php::coalesce(php::get(args, "cardIds"), &php::arr()));
            let n = if php::is_numeric(php::get(args, "n")) { php::intval(php::get(args, "n")) } else { 1 };
            let max = n.min(php::count(&ids) as i64);
            let costs = {
                let mut m = Map::new();
                for (k, v) in php::entries(php::coalesce(php::get(args, "targetCosts"), &php::arr())) {
                    m.insert(k, json!(php::intval(v)));
                }
                Value::Object(m)
            };
            let up_to = php::truthy(php::coalesce(php::get(args, "upTo"), &json!(false)));
            with(&base, json!({"kind": "select", "cardIds": ids, "min": if up_to { 0 } else { max }, "max": max, "costs": costs, "showInPicker": php::truthy(php::coalesce(php::get(args, "manaOrbs"), &json!(false)))}))
        }
        "discard" => {
            let ids = ints(php::coalesce(php::get(&private, "cards"), &php::arr()));
            let n = if php::is_numeric(php::get(args, "n")) { php::intval(php::get(args, "n")) } else { php::count(&ids) as i64 };
            let max = n.min(php::count(&ids) as i64);
            let up_to = php::truthy(php::coalesce(php::get(args, "upTo"), &json!(false)));
            with(&base, json!({"kind": "select", "cardIds": ids, "min": if up_to { 0 } else { max }, "max": max, "showInPicker": php::get(args, "source") == &json!("mana")}))
        }
        "nightCleanup" => with(
            &base,
            json!({"kind": "cleanup",
                "reserve": {"cardIds": php::coalesce(php::get(&private, "reserveCards"), &php::arr()), "n": php::intval(php::get(args, "nReserve"))},
                "landmarks": {"cardIds": php::coalesce(php::get(&private, "landmarkCards"), &php::arr()), "n": php::intval(php::get(args, "nLandmarks"))}}),
        ),
        "moveExpedition" => {
            let mut opts = vec![];
            for e in php::vals(php::coalesce(php::get(args, "expeditions"), &php::arr())) {
                if php::is_arr(e) && php::count(e) == 2 {
                    let side = if php::geti(e, 1) == &json!("stormLeft") { "hero" } else { "companion" };
                    opts.push(json!({"key": serde_json::to_string(e).unwrap(), "player": php::intval(php::geti(e, 0)), "side": side}));
                }
            }
            with(&base, json!({"kind": "expedition", "expeditions": opts, "min": 1, "max": 1}))
        }
        "targetExpedition" => {
            let mut opts = vec![];
            for ex in php::vals(php::coalesce(php::get(args, "expeditions"), &php::arr())) {
                let s = php::strval(ex);
                let parts: Vec<&str> = s.splitn(2, '-').collect();
                let side = if parts.get(1) == Some(&"stormLeft") { "hero" } else { "companion" };
                opts.push(json!({"key": ex, "player": php::intval(&json!(parts[0])), "side": side}));
            }
            let n = php::intval(php::coalesce(php::get(args, "n"), &json!(1)));
            with(&base, json!({"kind": "expedition", "expeditions": opts, "min": n.min(php::count(&Value::Array(opts.clone())) as i64), "max": n}))
        }
        "blockExpedition" => {
            let mut opts = vec![];
            for p in players::get_all(g) {
                for (loc, side) in [("stormLeft", "hero"), ("stormRight", "companion")] {
                    opts.push(json!({"key": format!("board-{}-{}", loc, p), "player": p, "side": side}));
                }
            }
            with(&base, json!({"kind": "expedition", "expeditions": opts, "min": 1, "max": 1}))
        }
        "targetPlayer" => {
            let mut ps: Vec<PId> = players::get_all(g);
            if !php::empty(php::get(args, "opponentsOnly")) {
                ps.retain(|p| *p != you);
            }
            with(&base, json!({"kind": "player", "players": ps}))
        }
        "invokeToken" => {
            let names = |l: &str| -> String {
                match l {
                    "stormLeft" => "Hero expedition",
                    "stormRight" => "Companion expedition",
                    "source" | "initialSource" => "Source expedition",
                    "oppositeSource" => "Opposite expedition",
                    "landmark" => "Landmarks",
                    _ => l,
                }
                .to_string()
            };
            let mut locations = vec![];
            let owner = php::coalesce(php::get(args, "invokePlayerId"), &json!(you)).clone();
            let locs = php::coalesce(php::get(args, "locations"), &php::arr()).clone();
            if !php::empty(php::get(args, "allPlayers")) {
                for p in players::get_all(g) {
                    let pname = g.player(p)?.name.clone();
                    for l in php::vals(&locs) {
                        let ls = php::strval(l);
                        let mut e = json!({"key": format!("{}-{}", ls, p), "label": format!("{} — {}", pname, names(&ls)), "player": p});
                        if DESTINATIONS.iter().any(|(k, _)| *k == ls) {
                            php::set(&mut e, "zone", destination(l));
                        }
                        locations.push(e);
                    }
                }
            } else {
                for l in php::vals(&locs) {
                    let ls = php::strval(l);
                    let key = if ls == "stormLeft" || ls == "stormRight" { format!("{}-{}", ls, php::strval(&owner)) } else { ls.clone() };
                    let mut e = json!({"key": key, "label": names(&ls), "player": php::intval(&owner)});
                    if DESTINATIONS.iter().any(|(k, _)| *k == ls) {
                        php::set(&mut e, "zone", destination(l));
                    }
                    locations.push(e);
                }
            }
            with(&base, json!({"kind": "location", "locations": locations}))
        }
        "rollDie" => with(&base, json!({"kind": "number", "values": ints(php::coalesce(php::get(args, "rolls"), &php::arr()))})),
        "pay" => {
            let max = php::intval(php::coalesce(php::get(args, "mana"), &json!(0))).min(php::intval(php::coalesce(php::get(args, "maximum"), &json!(0))));
            let values: Vec<i64> = (0..=max.max(0)).collect();
            with(&base, json!({"kind": "number", "values": values}))
        }
        "spend" => {
            let n = php::intval(php::coalesce(php::get(args, "n"), &json!(1))).max(1);
            let values: Vec<i64> = (1..=n).collect();
            with(&base, json!({"kind": "number", "values": values}))
        }
        "exchange" => with(&base, json!({"kind": "exchange", "hand": php::coalesce(php::get(args, "handIds"), &php::arr()), "reserve": php::coalesce(php::get(args, "reserveIds"), &php::arr())})),
        "markRegion" => {
            let mut buttons = vec![];
            for m in php::vals(php::coalesce(php::get(args, "markers"), &php::arr())) {
                for (rid, _) in php::entries(php::coalesce(php::get(args, "regions"), &php::arr())) {
                    buttons.push(json!({"key": format!("{}:{}", php::strval(php::get(m, "id")), rid), "label": format!("{} marker → region {}", php::ucfirst(&php::strval(php::get(m, "type"))), rid)}));
                }
            }
            with(&base, json!({"kind": "buttons", "buttons": buttons}))
        }
        "moveRegionMarker" => {
            let buttons: Vec<Value> = php::vals(php::coalesce(php::get(args, "markers"), &php::arr()))
                .into_iter()
                .map(|m| json!({"key": php::strval(php::get(m, "id")), "label": format!("Move {} marker", php::strval(php::get(m, "type")))}))
                .collect();
            with(&base, json!({"kind": "buttons", "buttons": buttons}))
        }
        "interuptReveal" => with(&base, json!({"kind": "buttons", "buttons": [{"key": "ok", "label": "Confirm"}]})),
        _ => with(&base, json!({"kind": "buttons", "buttons": []})),
    };
    Ok(Some(d))
}

/// `V1::prompt` (texts are not compared: a short form)
fn prompt(def: &StateDef, args: &Value) -> String {
    if php::isset(args, "description") && (def.name == "target" || def.name == "discardDo") {
        return "Choose target".into();
    }
    def.name.to_string()
}

/// `V1::label($choice)`: never empty ('Option <id>' when the description gives no text)
fn label(choice: &Value) -> String {
    let d = php::get(choice, "description");
    let text = if php::is_arr(d) { php::strval(php::get(d, "log")) } else { php::strval(d) };
    if text.trim().is_empty() {
        format!("Option {}", php::strval(php::get(choice, "id")))
    } else {
        text
    }
}

/// `V1::toAction($state, $answer)`: the game method and its arguments
// PHP: server/src/V1.php:463
pub fn to_action(raw: &RawState, a: &Value) -> R<(String, Value)> {
    let ty = php::strval(php::coalesce(php::get(a, "type"), &json!("")));
    let name = raw.def.map(|d| d.name).unwrap_or("");
    let atomic = |action: &str, args: Vec<Value>| Ok((String::from("actTakeAtomicAction"), json!([action, args])));
    let bad = || Err(PhpError::new(format!("Answer '{}' does not fit decision '{}'", ty, name)));
    match ty.as_str() {
        "pass" => Ok(("actPassOptionalAction".into(), json!([]))),
        "undo" => Ok(("actRestart".into(), json!([]))),
        "confirm" => Ok((if name == "confirmPartialTurn" { "actConfirmPartialTurn" } else { "actConfirmTurn" }.into(), json!([]))),
        "deck" => {
            for deck in php::vals(php::get(php::get(&raw.args, "_private"), "decks")) {
                if php::loose_eq(php::get(deck, "deckId"), php::get(a, "deck")) {
                    return Ok(("actSelectPrecoDeck".into(), json!([php::get(deck, "deckNumber")])));
                }
            }
            Err(PhpError::new("Unknown deck"))
        }
        "play" => {
            let dest = php::get(a, "destination");
            let Some((loc, _)) = DESTINATIONS.iter().find(|(_, v)| Value::String(v.to_string()) == *dest) else {
                return Err(PhpError::new("Unknown destination"));
            };
            atomic(if name == "playCard" { "actPlayCard" } else { "actPlay" }, vec![json!(php::intval(php::get(a, "cardId"))), json!(loc)])
        }
        "support" => atomic("actSupport", vec![json!(php::intval(php::get(a, "cardId")))]),
        "tap" => atomic("actTap", vec![json!(php::intval(php::get(a, "cardId")))]),
        "endTurn" => atomic("actPass", vec![]),
        "choice" => Ok(("actChooseAction".into(), json!([php::intval(php::get(a, "id"))]))),
        "select" => {
            let ids = ints(php::coalesce(php::get(a, "cardIds"), &php::arr()));
            if name == "firstDayManaSelection" {
                return Ok(("actFirstDayManaSelection".into(), json!([ids])));
            }
            if name == "discard" {
                return atomic("actDiscard", vec![ids]);
            }
            atomic("actTarget", vec![ids])
        }
        "cleanup" => atomic("actNightCleanup", vec![ints(php::coalesce(php::get(a, "reserve"), &php::arr())), ints(php::coalesce(php::get(a, "landmarks"), &php::arr()))]),
        "expedition" => {
            let keys = php::coalesce(php::get(a, "keys"), &php::arr()).clone();
            if name == "moveExpedition" {
                let k = php::strval(php::geti(&keys, 0));
                let v: Value = serde_json::from_str(&k).unwrap_or(Value::Null);
                return atomic("actMoveExpedition", vec![v]);
            }
            if name == "blockExpedition" {
                return atomic("actBlockExpedition", vec![php::geti(&keys, 0).clone()]);
            }
            atomic("actTargetExpedition", vec![keys])
        }
        "player" => atomic("actTargetPlayer", vec![json!(php::intval(php::get(a, "player")))]),
        "location" => atomic("actInvokeToken", vec![json!(php::strval(php::get(a, "key")))]),
        "number" => {
            let m = match name {
                "rollDie" => "actRollDie",
                "pay" => "actPay",
                "spend" => "actSpend",
                _ => return Err(PhpError::new("Unexpected number")),
            };
            atomic(m, vec![json!(php::intval(php::get(a, "value")))])
        }
        "exchange" => atomic("actExchange", vec![json!(php::intval(php::get(a, "reserve"))), json!(php::intval(php::get(a, "hand")))]),
        "button" => {
            let key = php::strval(php::get(a, "key"));
            if name == "markRegion" {
                let parts: Vec<&str> = key.splitn(2, ':').collect();
                return atomic("actMarkRegion", vec![json!(php::intval(&json!(parts[0]))), json!(php::intval(&json!(parts.get(1).copied().unwrap_or(""))))]);
            }
            if name == "moveRegionMarker" {
                return atomic("actMoveRegionMarker", vec![json!(php::intval(&json!(key)))]);
            }
            if name == "interuptReveal" {
                return atomic("actInteruptReveal", vec![]);
            }
            bad()
        }
        _ => bad(),
    }
}

/// `Arena::ACTIONS`: the client methods and their argument names
const ACTIONS: &[(&str, &[&str])] = &[
    ("actSelectPrecoDeck", &["choice"]),
    ("actSelectRandomDeck", &["faction"]),
    ("actConfirmAPIDeck", &["deckContent"]),
    ("actCancelPrecoDeckSelection", &[]),
    ("actFirstDayManaSelection", &["cardIds"]),
    ("actCancelFirstDayManaSelection", &[]),
    ("actNewDayManaSelection", &["cardIds"]),
    ("actPassNewDayManaSelection", &[]),
    ("actConfirmTurn", &[]),
    ("actConfirmPartialTurn", &[]),
    ("actRestart", &[]),
    ("actUndoToStep", &["stepId"]),
    ("actTakeAtomicAction", &["actionName", "actionArgs"]),
    ("actChooseAction", &["id"]),
    ("actPassOptionalAction", &[]),
    ("actAnytimeAction", &["id"]),
    ("actChangePreference", &["pref", "value"]),
];

fn validate_alpha_num(v: &Value) -> R<()> {
    match v {
        Value::Array(_) | Value::Object(_) => {
            for (k, x) in php::entries(v) {
                validate_alpha_num(&json!(k))?;
                validate_alpha_num(x)?;
            }
            Ok(())
        }
        Value::Number(n) if n.is_i64() || n.is_u64() => Ok(()),
        Value::Bool(_) | Value::Null => Ok(()),
        _ => {
            let s = php::strval(v);
            if s.chars().all(|c| c.is_ascii_alphanumeric() || " _-@.".contains(c)) {
                Ok(())
            } else {
                Err(PhpError::new("Bad argument value"))
            }
        }
    }
}

/// `Arena::act($gameId, $playerId, $action, $args)`: one request of the player
// PHP: server/src/Arena.php:304
pub fn act(g: &mut Game, pid: PId, action: &str, args: &Value) -> R<()> {
    if !g.players.contains_key(&pid) {
        return Err(PhpError::new("Unknown player"));
    }
    let Some((_, expected)) = ACTIONS.iter().find(|(a, _)| *a == action) else {
        return Err(PhpError::new(format!("Unknown action {}", action)));
    };
    let mut args: Vec<Value> = match args {
        Value::Array(a) => a.iter().take(expected.len()).cloned().collect(),
        Value::Object(o) => expected.iter().map(|n| o.get(*n).cloned().unwrap_or(Value::Null)).collect(),
        _ => vec![],
    };
    if action == "actTakeAtomicAction" {
        let a0 = php::strval(args.first().unwrap_or(&Value::Null));
        if !(a0.starts_with("act") && a0.len() > 3 && a0[3..].chars().all(|c| c.is_ascii_alphabetic())) {
            return Err(PhpError::new("Bad atomic action"));
        }
        while args.len() < 2 {
            args.push(Value::Null);
        }
        if !php::is_arr(&args[1]) {
            args[1] = json!([]);
        }
        validate_alpha_num(&args[1])?;
    }
    // Arena::request(..., act: true): mt_srand for this act
    g.seed_rng();
    g.begin_request(Some(pid))?;
    if action != "actChangePreference" && !framework::is_player_active(g, pid) {
        return Err(PhpError::new("It is not your turn"));
    }
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
    match action {
        "actSelectPrecoDeck" => states::setup::act_select_preco_deck(g, &arg(0)),
        "actCancelPrecoDeckSelection" => states::setup::act_cancel_preco_deck_selection(g),
        "actFirstDayManaSelection" => states::new_day::act_first_day_mana_selection(g, &arg(0)),
        "actCancelFirstDayManaSelection" => states::new_day::act_cancel_first_day_mana_selection(g),
        "actConfirmTurn" => states::engine::act_confirm_turn(g, false),
        "actConfirmPartialTurn" => states::engine::act_confirm_partial_turn(g),
        "actRestart" => states::engine::act_restart(g),
        "actUndoToStep" => states::engine::act_undo_to_step(g, php::intval(&arg(0))),
        "actTakeAtomicAction" => states::engine::act_take_atomic_action(g, &php::strval(&arg(0)), &arg(1)),
        "actChooseAction" => states::engine::act_choose_action(g, php::intval(&arg(0))),
        "actPassOptionalAction" => game_methods::act_pass_optional_action(g, false),
        "actAnytimeAction" => states::engine::act_anytime_action(g, &arg(0)),
        "actChangePreference" => game_methods::act_change_preference(g, php::intval(&arg(0)), &arg(1)),
        "actNewDayManaSelection" | "actPassNewDayManaSelection" => Err(PhpError::new(format!("Error: Call to undefined method altered::{}()", action))),
        _ => Err(PhpError::nt(&format!("States/SetupTrait.php {}", action))),
    }
}

/// `V1::answer($gameId, $playerId, $answer)`: the decision must be the current one, then the
/// answer's action runs as a request, then the view is built again
// PHP: server/src/V1.php:82
pub fn answer(g: &mut Game, pid: PId, a: &Value) -> R<()> {
    let backup = g.clone();
    let res = (|| {
        let raw = view_state(g, pid)?;
        let current = decision(g, &raw, pid)?;
        let Some(current) = current else { return Err(PhpError::new("Nothing to decide")) };
        if php::get(a, "decisionId") != php::get(&current, "id") {
            return Err(PhpError::new("This decision is no longer current"));
        }
        let (action, args) = to_action(&raw, a)?;
        act(g, pid, &action, &args)
    })();
    if let Err(e) = res {
        *g = backup;
        return Err(e);
    }
    // The view after the action (its errors make the answer fail, the action is committed)
    let raw = view_state(g, pid)?;
    decision(g, &raw, pid)?;
    Ok(())
}

/// `Arena::oracleState($gameId)`
// PHP: server/src/Arena.php:461
pub fn oracle_state(g: &mut Game) -> R<Value> {
    g.begin_request(None)?;
    let mut tokens_on: std::collections::HashMap<i64, Vec<String>> = Default::default();
    let mut positions: std::collections::HashMap<i64, (i64, i64)> = Default::default();
    for m in meeples::get_all(g) {
        let mm = g.meeples[&m].clone();
        if let Some(c) = mm.location.strip_prefix("card-") {
            tokens_on.entry(php::intval(&json!(c))).or_default().push(mm.kind.clone());
        } else if let Some(s) = mm.location.strip_prefix("storm-") {
            if mm.kind == "hero" || mm.kind == "companion" {
                let p = php::intval(&json!(mm.pid));
                let e = positions.entry(p).or_insert((0, 7));
                if mm.kind == "hero" {
                    e.0 = php::intval(&json!(s));
                } else {
                    e.1 = php::intval(&json!(s));
                }
            }
        }
    }
    let mut out = vec![];
    for id in cards::get_all(g) {
        let c = g.cards[&id].clone();
        let loc = c.location.clone();
        let zone = if loc.starts_with("deck-") {
            "deck".to_string()
        } else if loc.starts_with("board-hero") {
            "hero".into()
        } else if loc.starts_with("reveal-") {
            "reveal".into()
        } else if loc == "stormLeft" {
            "heroExpedition".into()
        } else if loc == "stormRight" {
            "companionExpedition".into()
        } else {
            loc.clone()
        };
        let toks = tokens_on.get(&id).cloned().unwrap_or_default();
        let r = if php::loose_eq(c.db_props.get("rarity").unwrap_or(&Value::Null), &json!(2)) { "Card".to_string() } else { cards::class_name(&c.db_props) };
        let extra = card::get(g, id, "extraDatas")?;
        out.push(json!({
            "id": id,
            "ref": r,
            "owner": c.pid,
            "zone": zone,
            "order": c.state,
            "exhausted": card::is_tapped(g, id)?,
            "boosts": toks.iter().filter(|t| *t == "boost").count(),
            "statuses": toks.iter().filter(|t| ["fleeting", "anchored", "asleep"].contains(&t.as_str())).collect::<Vec<_>>(),
            "counter": php::intval(php::coalesce(php::get(&extra, "counter"), &json!(0))),
        }));
    }
    out.sort_by(|a, b| {
        let ka = (php::intval(&a["owner"]), a["zone"].as_str().unwrap_or("").to_string(), php::intval(&a["order"]), php::intval(&a["id"]));
        let kb = (php::intval(&b["owner"]), b["zone"].as_str().unwrap_or("").to_string(), php::intval(&b["order"]), php::intval(&b["id"]));
        ka.cmp(&kb)
    });
    let mut ps = vec![];
    for p in players::get_all(g) {
        let pos = positions.get(&p).copied().unwrap_or((0, 7));
        ps.push(json!({"id": p, "name": g.player(p)?.name, "bot": false, "position": [pos.0, pos.1]}));
    }
    let storm: Vec<Value> = php::vals(&globals::get_storm(g, false))
        .into_iter()
        .map(|t| json!({"card": php::intval(php::get(t, "cardId")), "rotated": php::truthy(php::get(t, "rotated")), "revealed": php::truthy(php::get(t, "visible"))}))
        .collect();
    Ok(json!({
        "players": ps,
        "first": globals::get_int(g, "firstPlayer")?,
        "day": globals::get_int(g, "day")?,
        "phase": globals::get_int(g, "phase")?,
        "state": framework::state(g).map(|s| s.name).unwrap_or(""),
        "storm": storm,
        "cards": out,
        "dice": g.dice,
        "diceUsed": g.dice_used,
        "nextCardId": g.next_card_id,
    }))
}
