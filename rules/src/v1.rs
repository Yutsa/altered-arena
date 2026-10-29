//! server/src/V1.php: the protocol v1 GameView (V1::build on getAllDatas), decisions on top of
//! the engine, answers turned into the engine's actions (through Arena::act), the game log, and
//! Arena::oracleState.

use crate::core::globals;
use crate::framework::{self, StateDef};
use crate::game::{Game, LogLine, PId};
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
            let known = deck_summaries();
            let mut decks = vec![];
            for d in php::vals(php::coalesce(php::get(&private, "decks"), &php::arr())) {
                let id = php::get(d, "deckId");
                match known.iter().find(|k| php::loose_eq(&k["id"], id)) {
                    Some(k) => decks.push(k.clone()),
                    None => decks.push(json!({
                        "id": id,
                        "name": php::coalesce(php::get(php::get(php::get(d, "hero"), "properties"), "name"), id),
                        "faction": php::get(d, "faction"),
                        "heroRef": null,
                        "size": 40,
                    })),
                }
            }
            with(&base, json!({"kind": "deck", "decks": decks, "prompt": "Choose your deck"}))
        }
        "firstDayManaSelection" => {
            if !php::empty(php::get(&private, "selection")) {
                return Ok(None);
            }
            let n = php::intval(php::coalesce(php::get(&private, "n"), &json!(3)));
            with(&base, json!({"kind": "select", "cardIds": php::coalesce(php::get(&private, "cards"), &php::arr()), "min": n, "max": n, "prompt": format!("Choose {} cards to put in your Mana", n)}))
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
        "confirmTurn" | "confirmPartialTurn" => with(&base, json!({"kind": "confirm", "prompt": "Confirm your turn"})),
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

/// states.inc.php: `descriptionmyturn` of each state, else its `description`
fn state_description(name: &str) -> &'static str {
    match name {
        "selectPrecoDeck" => "${you} must select the faction you want to play with",
        "selectDeck" => "${you} must select the deck you want to play",
        "firstDayManaSelection" => "${you} must select ${n} cards to discard as mana",
        "chooseAssignment" => "${you} must choose an action or pass",
        "rollDie" => "${you} must choose 1 die result",
        "moveExpedition" => "${you} must move one expedition forward",
        "discard" => "${you} must discard ${n} card(s) from ${source} to ${destination}",
        "nightCleanup" => "${you} must discard ${nReserve} reserve card(s) and ${nLandmarks} landmark card(s)",
        "target" => "${you} must select the target(s): ${description}",
        "discardDo" => "${you} may discard cards to ${effect_desc}",
        "targetPlayer" => "${you} ${description}",
        "invokeToken" => "${you} may invoke ${tokenType}",
        "blockExpedition" => "${you} must block an expedition from moving this day",
        "targetExpedition" => "${you} must target ${n} expedition(s)",
        "playCard" => "${you} may play ${card_name}",
        "pay" => "${you} must pay mana",
        "exchange" => "${you} must select 1 card from Hand and Reserve to switch position",
        "spend" => "${you} spends 1 counter",
        "boostExchange" => "${source}: ${you} exchange boosts on 2 cards",
        "markRegion" => "${you} may mark a visible region",
        "moveRegionMarker" => "${you} may move a ${markerType} terrain marker to its region",
        "interuptReveal" => "${you} must confirm that the card is seen",
        "confirmTurn" => "${you} must confirm or restart your turn",
        "confirmPartialTurn" => "${you} must confirm the switch of player. You will not be able to restart turn",
        "resolveChoice" => "${you} must choose which effect to resolve",
        "impossibleAction" => "${you} can't take the mandatory action. Restart your turn or exchange/cook to make it possible",
        _ => "",
    }
}

/// `$args + ['you' => 'You', 'actplayer' => 'Opponent']`
fn with_you(args: &Value) -> Value {
    let mut a = if let Value::Object(_) = args { args.clone() } else { Value::Object(Map::new()) };
    if let Value::Array(l) = args {
        let mut m = Map::new();
        for (i, x) in l.iter().enumerate() {
            m.insert(i.to_string(), x.clone());
        }
        a = Value::Object(m);
    }
    if let Value::Object(m) = &mut a {
        m.entry("you").or_insert(json!("You"));
        m.entry("actplayer").or_insert(json!("Opponent"));
    }
    a
}

/// `V1::prompt($state, $args, $g)`: the state's description, formatted with its args
// PHP: server/src/V1.php:563
fn prompt(def: &StateDef, args: &Value) -> String {
    let desc = state_description(def.name);
    let mut text = format(desc, &with_you(args), 0);
    if php::isset(args, "description") && (def.name == "target" || def.name == "discardDo") {
        let d = php::get(args, "description");
        let t = if php::is_arr(d) { format(&php::strval(php::get(d, "log")), php::get(d, "args"), 0) } else { format(&php::strval(d), &json!({}), 0) };
        if !t.is_empty() && t != "0" {
            text = t;
        }
        text = format!("Choose target: {}", text);
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text == "0" {
        let mut s = String::new();
        for c in def.name.chars() {
            if c.is_ascii_uppercase() {
                s.push(' ');
            }
            s.push(c);
        }
        return php::ucfirst(&s);
    }
    text
}

/// `V1::label($choice)`: never empty ('Option <id>' when the description gives no text)
// PHP: server/src/V1.php:578
fn label(choice: &Value) -> String {
    let d = php::get(choice, "description");
    let mut text = if php::is_arr(d) { format(&php::strval(php::get(d, "log")), php::get(d, "args"), 0) } else { format(&php::strval(d), &json!({}), 0) };
    let source = php::get(choice, "source");
    if !php::empty(source) && source.is_string() {
        text.push_str(&format!(" ({})", php::strval(source)));
    }
    let text = text.trim().to_string();
    if text.is_empty() || text == "0" {
        format!("Option {}", php::strval(php::get(choice, "id")))
    } else {
        text
    }
}

/// `V1::format($log, $args)`: BGA log substitution, `${key}` by `args[key]`, recursively for
/// nested `{log, args}`; card names in [[double brackets]]
// PHP: server/src/V1.php:589
pub fn format(log: &str, args: &Value, depth: usize) -> String {
    if depth > 6 {
        return log.to_string();
    }
    let mut out = String::new();
    let mut rest = log;
    while let Some(i) = rest.find("${") {
        let after = &rest[i + 2..];
        let key_len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(after.len());
        if key_len == 0 || !after[key_len..].starts_with('}') {
            out.push_str(&rest[..i + 2]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..i]);
        let key = &after[..key_len];
        let v = php::get(args, key);
        if php::is_arr(v) {
            if php::isset(v, "log") {
                out.push_str(&format(&php::strval(php::get(v, "log")), php::get(v, "args"), depth + 1));
            } else {
                let parts: Vec<String> = php::vals(v).into_iter().map(|x| if x.is_array() || x.is_object() { String::new() } else { php::strval(x) }).collect();
                out.push_str(&parts.join(", "));
            }
        } else if ["card_name", "card_name2", "source", "cardName", "eventCardName"].contains(&key) {
            out.push_str(&format!("[[{}]]", php::strval(v)));
        } else {
            out.push_str(&php::strval(v));
        }
        rest = &after[key_len + 1..];
    }
    out.push_str(rest);
    out
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
    call_action(g, action, &args)?;
    crate::arena::store_notifications(g);
    Ok(())
}

/// `$game->$action(...$args)`: a client method of the game
pub fn call_action(g: &mut Game, action: &str, args: &[Value]) -> R<()> {
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

// ---------------------------------------------------------------------------
// GameView (V1::build)
// ---------------------------------------------------------------------------

/// protocol/data/decks.json (`V1::decks()`)
pub fn deck_summaries() -> &'static [Value] {
    static DECKS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    DECKS.get_or_init(|| serde_json::from_str(include_str!("../../protocol/data/decks.json")).expect("protocol/data/decks.json"))
}

const ZONES: &[(&str, &str)] = &[
    ("hand", "hand"),
    ("reserve", "reserve"),
    ("landmark", "landmark"),
    ("stormLeft", "heroExpedition"),
    ("stormRight", "companionExpedition"),
    ("discard", "discard"),
    ("mana", "mana"),
    ("limbo", "limbo"),
    ("deck", "deck"),
];

const PHASES: &[&str] = &["morning", "noon", "afternoon", "dusk", "night"];

/// The engine class of a card (`ReflectionClass::getShortName()`): `Card` for Uniques
pub fn card_ref(g: &Game, id: i64) -> R<String> {
    let c = g.card(id)?;
    if php::loose_eq(c.db_props.get("rarity").unwrap_or(&Value::Null), &json!(2)) {
        return Ok("Card".into());
    }
    let name = cards::class_name(&c.db_props);
    if cards::classes().contains_key(&name) {
        return Ok(name);
    }
    // getCardInstance found the class by faction, rarity and name
    let found = cards::classes().iter().find(|(_, p)| {
        p.get("faction") == c.db_props.get("faction") && php::loose_eq(p.get("rarity").unwrap_or(&Value::Null), c.db_props.get("rarity").unwrap_or(&Value::Null)) && p.get("name") == c.db_props.get("name")
    });
    Ok(found.map(|(k, _)| k.clone()).unwrap_or(name))
}

/// `Cards::getUiData($pId)`: cards in play, Reserves, heroes, limbo, discards, revealed cards,
/// the viewer's hand and mana, revealed cards of other hands; ordered by state
// PHP: Managers/Cards.php:625
fn ui_cards(g: &mut Game, viewer: Option<PId>) -> R<Vec<i64>> {
    use crate::helpers::collection::merge;
    let mut ids = cards::where_location(g, cards::get_all(g), &cards::in_play());
    for loc in ["reserve", "board-hero-%", "limbo", "discard", "reveal-%"] {
        let more = cards::get_in_location(g, loc, None)?;
        ids = merge(&ids, &more);
    }
    if let Some(p) = viewer {
        let hand = cards::get_hand(g, &json!(p), &Value::Null)?;
        ids = merge(&ids, &hand);
        let mana = cards::get_filtered(g, &json!(p), &json!("mana"), &Value::Null, false)?;
        ids = merge(&ids, &mana);
    }
    let current = viewer.unwrap_or(-1);
    let mut revealed = vec![];
    for c in cards::where_location(g, cards::get_all(g), &json!("hand")) {
        if g.cards[&c].pid != current && card::is(g, c, "revealed")? {
            revealed.push(c);
        }
    }
    ids = merge(&ids, &revealed);
    cards::order_by_state(g, &mut ids, true);
    Ok(ids)
}

/// `V1::card($c, $tokens, &$positions)`
// PHP: server/src/V1.php:227
fn card_view(g: &Game, id: i64, tokens: &[String], positions: &mut std::collections::HashMap<String, i64>) -> R<Value> {
    let c = g.card(id)?;
    let p = &c.props;
    let location = c.location.as_str();
    let zone = match ZONES.iter().find(|(k, _)| *k == location) {
        Some((_, z)) => z.to_string(),
        None if location.starts_with("board-hero") => "hero".into(),
        None if location.starts_with("reveal") => "limbo".into(),
        None => location.to_string(),
    };
    let owner = c.pid;
    let key = format!("{}|{}", owner, zone);
    let pos = positions.entry(key).or_insert(-1);
    *pos += 1;
    let position = *pos;
    let boosts = tokens.iter().filter(|t| *t == "boost").count() as i64;
    let mut statuses: Vec<String> = vec![];
    for t in tokens {
        if !["boost", "forest", "mountain", "ocean"].contains(&t.as_str()) && !statuses.contains(t) {
            statuses.push(t.clone());
        }
    }
    for kw in ["gigantic", "tough", "defender", "eternal", "seasoned"] {
        if !php::empty(p.get(kw).unwrap_or(&Value::Null)) && !statuses.iter().any(|s| s == kw) {
            statuses.push(kw.to_string());
        }
    }
    let mut v = json!({
        "id": id,
        "ref": card_ref(g, id)?,
        "owner": owner,
        "zone": zone,
        "position": position,
        "exhausted": php::truthy(p.get("tapped").unwrap_or(&Value::Null)),
        "boosts": boosts,
        "counters": {},
        "statuses": statuses,
    });
    let token = !php::empty(p.get("token").unwrap_or(&Value::Null));
    if token {
        v["token"] = json!(true);
    }
    if p.get("type") == Some(&json!("character")) || token {
        if p.get("forest").is_some_and(|x| !x.is_null()) {
            let stat = |k: &str| php::intval(p.get(k).unwrap_or(&Value::Null)) + boosts;
            v["stats"] = json!({"forest": stat("forest"), "mountain": stat("mountain"), "ocean": stat("ocean")});
        }
    }
    for k in ["costHand", "costReserve"] {
        if let Some(x) = p.get(k) {
            if !x.is_null() && php::is_numeric(x) {
                v[k] = json!(php::intval(x));
            }
        }
    }
    let extra = p.get("extraDatas").cloned().unwrap_or(Value::Null);
    if !php::empty(php::get(&extra, "counter")) {
        let name = php::strval(php::coalesce(php::get(&extra, "counterName"), &json!("counter")));
        let mut m = Map::new();
        m.insert(name, json!(php::intval(php::get(&extra, "counter"))));
        v["counters"] = Value::Object(m);
    }
    Ok(v)
}

/// The GameView of a player (`V1::build(Arena::view($gameId, $playerId))`); `you = None` is a
/// spectator (no hidden cards, no decision). Built on a copy of the game; what the view request
/// writes in PHP is kept: the preferences `getAllDatas` creates and the globals it reads for the
/// first time (rows created with their default value).
// PHP: server/src/V1.php:96
pub fn build(g: &mut Game, you: Option<PId>, game_id: &str) -> R<Value> {
    let mut copy = g.clone();
    crate::core::preferences::check_existence(&mut copy);
    let view = build_view(&mut copy, you, game_id)?;
    g.prefs = copy.prefs.clone();
    for (k, v) in &copy.globals {
        if !g.globals.contains_key(k) && Some(v) == globals::var_type(k).map(globals::default_of).as_ref() {
            g.globals.insert(k.clone(), v.clone());
            globals::touched(g, k);
        }
    }
    Ok(view)
}

fn build_view(g: &mut Game, you: Option<PId>, game_id: &str) -> R<Value> {
    let raw = match you {
        Some(p) => Some(view_state(g, p)?),
        None => {
            g.begin_request(None)?;
            None
        }
    };
    let state = framework::state(g);
    let state_name = state.map(|s| s.name).unwrap_or("");
    let state_type = state.map(|s| s.kind).unwrap_or("");

    let phase_n = globals::get_int(g, "phase")?;
    let mut phase = PHASES.get(phase_n as usize).copied().unwrap_or("setup");
    if ["selectPrecoDeck", "selectDeck", "firstDayManaSelection", "gameSetup", "deckSetup", "firstDay"].contains(&state_name) {
        phase = "setup";
    }
    if php::truthy(&globals::get(g, "tieBreakerMode")?) {
        phase = "tiebreaker";
    }
    if state_name == "gameEnd" {
        phase = "ended";
    }

    // Cards (with the engine class name as ref)
    let mut tokens_on: std::collections::HashMap<i64, Vec<String>> = Default::default();
    let mut storm_tokens: std::collections::HashMap<(PId, String), i64> = Default::default();
    for m in meeples::get_all(g) {
        let mm = &g.meeples[&m];
        if let Some(c) = mm.location.strip_prefix("card-") {
            tokens_on.entry(php::intval(&json!(c))).or_default().push(mm.kind.clone());
        }
        if (mm.kind == "hero" || mm.kind == "companion") && mm.location.starts_with("storm-") {
            storm_tokens.insert((php::intval(&json!(mm.pid)), mm.kind.clone()), php::intval(&json!(&mm.location[6..])));
        }
    }
    let mut rows = ui_cards(g, you)?;
    // players' manaCards (the viewer's mana, already there)
    if let Some(p) = you {
        for c in crate::models::player::get_mana_cards(g, p, None)? {
            if !rows.contains(&c) {
                rows.push(c);
            }
        }
    }
    let mut positions = Default::default();
    let mut card_views = vec![];
    for id in &rows {
        card_views.push(card_view(g, *id, tokens_on.get(id).map(|v| v.as_slice()).unwrap_or(&[]), &mut positions)?);
    }

    // Players
    let biomes = players::get_biome_totals(g)?;
    let blocked = players::get_blocked_expeditions(g)?;
    let reserve_slots = players::get_reserve_slots(g)?;
    let landmark_slots = players::get_landmark_slots(g)?;
    let passed = if globals::is(g, "dayPhase")? { globals::get(g, "skippedPlayers")? } else { php::arr() };
    let mut player_views = vec![];
    for pid in players::get_all(g) {
        let hero = card_views.iter().filter(|c| c["zone"] == json!("hero") && c["owner"] == json!(pid)).map(|c| c["id"].clone()).last();
        let mut expeditions = Map::new();
        for (side, loc) in [("hero", "stormLeft"), ("companion", "stormRight")] {
            let b = php::get(php::get(&biomes, &pid.to_string()), loc);
            let stat = |k: &str| php::intval(php::coalesce(php::get(b, k), &json!(0)));
            expeditions.insert(
                side.into(),
                json!({
                    "position": storm_tokens.get(&(pid, side.to_string())).copied().unwrap_or(if side == "hero" { 0 } else { 7 }),
                    "stats": {"forest": stat("forest"), "mountain": stat("mountain"), "ocean": stat("ocean")},
                    "blocked": php::truthy(php::get(php::get(&blocked, &pid.to_string()), loc)),
                }),
            );
        }
        let faction = g.player(pid)?.faction.clone();
        player_views.push(json!({
            "id": pid,
            "name": g.player(pid)?.name,
            "bot": g.bots.contains(&pid),
            "faction": if faction.is_empty() { Value::Null } else { json!(faction) },
            "heroCardId": hero,
            "handCount": crate::models::player::get_hand(g, pid, &Value::Null)?.len(),
            "deckCount": crate::models::player::get_deck_count(g, pid)?,
            "discardCount": crate::models::player::get_discard_count(g, pid)?,
            "manaTotal": crate::models::player::get_total_mana(g, pid)?,
            "manaAvailable": crate::models::player::get_mana(g, pid)?,
            "passed": php::in_array(&json!(pid), &passed),
            "reserveSlots": php::intval(php::coalesce(php::get(&reserve_slots, &pid.to_string()), &json!(2))),
            "landmarkSlots": php::intval(php::coalesce(php::get(&landmark_slots, &pid.to_string()), &json!(2))),
            "expeditions": expeditions,
        }));
    }

    // Regions of the Tumult
    let storm = globals::get_storm(g, true);
    let mut regions = vec![];
    for i in 0..=7i64 {
        let card = php::get(&storm, &((i + 1) / 2).to_string());
        let biomes: Value = if i == 0 || i == 7 {
            json!(["forest", "mountain", "ocean"])
        } else if !card.is_null() && php::intval(php::get(card, "cardId")) != 6 {
            let mut sides = globals::storm_card(php::intval(php::get(card, "cardId")));
            if php::truthy(php::get(card, "rotated")) {
                sides.reverse();
            }
            json!(sides[((i - 1) % 2) as usize])
        } else {
            Value::Null
        };
        regions.push(json!({"index": i, "biomes": biomes, "kind": if i == 0 { "heroRegion" } else if i == 7 { "companionRegion" } else { "tumult" }}));
    }

    let mut winner = Value::Null;
    if phase == "ended" {
        for (pid, p) in &g.players {
            if p.score > 0 {
                winner = json!(pid);
            }
        }
    }

    let waiting: Vec<PId> = match state_type {
        "multipleactiveplayer" => framework::get_active_player_list(g),
        "activeplayer" => vec![g.active_player_id()],
        _ => vec![],
    };
    let first = globals::get_int(g, "firstPlayer")?;
    let decision_v = match (&raw, you) {
        (Some(r), Some(p)) if r.is_active => decision(g, r, p)?,
        _ => None,
    };
    let log: Vec<Value> = crate::arena::log_for(g, you)
        .into_iter()
        .map(|l| {
            let mut e = json!({"seq": l.seq, "text": l.text});
            if let Some(p) = l.player {
                e["player"] = json!(p);
            }
            e
        })
        .collect();
    Ok(json!({
        "gameId": game_id,
        "you": you,
        "seq": g.notif_seq,
        "day": globals::get_int(g, "day")?,
        "phase": phase,
        "firstPlayer": if first == 0 { Value::Null } else { json!(first) },
        "activePlayer": if state_type == "activeplayer" { json!(g.active_player_id()) } else { Value::Null },
        "winner": winner,
        "players": player_views,
        "cards": card_views,
        "regions": regions,
        "decision": decision_v,
        "waitingFor": waiting,
        "log": log,
    }))
}

// ---------------------------------------------------------------------------
// Log (V1::log on the notifications stored by Arena::storeNotifications)
// ---------------------------------------------------------------------------

/// The public message of a notification the translation sends without one
/// (Core/Notifications.php)
fn notification_msg(name: &str, args: &Value) -> &'static str {
    match name {
        "drawCards" => {
            if php::get(args, "to") == &json!("reserve") {
                "${player_name} places ${n} card(s) in reserve"
            } else {
                "${player_name} draws ${n} card(s) from its deck"
            }
        }
        "discardMana" => "${player_name} places ${n} card(s) as mana",
        "playCard" => {
            if php::get(args, "fromLocation") == &json!("reserve") {
                "${player_name} plays ${card_name} from Reserve for ${mana_cost}"
            } else {
                "${player_name} plays ${card_name} for ${mana_cost}"
            }
        }
        "passTurn" => "${player_name} passes and ends its afternoon",
        "moveStormToken" => "${player_name} advances in ${expedition} expedition",
        "supportEffect" => "${player_name} activates support effect of ${card_name} and discards it",
        "tapEffect" => "${player_name} taps ${card_name} to activate its effect",
        "tap" => {
            if !php::get(args, "card2").is_null() {
                "${player_name} exhausts ${card_name} (${card_name2}'s effect)"
            } else {
                ""
            }
        }
        "roll" => "${player_name} rolls ${roll_text}",
        "pay" => "${player_name} pays ${mana_cost}",
        "invokeToken" => "${player_name} invokes ${card_name} in ${displayLocation}",
        "targetCards" => "${player_name} targets ${card_names}",
        "publicDiscard" => "${player_name} discards ${card_names}",
        "discard" => "${player_name} discards ${card_names}",
        "addMeeples" => {
            if php::isset(args, "power") {
                "${card_name} gains ${n} ${power}"
            } else {
                ""
            }
        }
        "looseMeeples" => {
            if php::isset(args, "power") {
                "${card_name} loses ${n} ${power}"
            } else {
                ""
            }
        }
        "gainCounter" => "${card_name} gains a counter",
        "useCounter" | "spendCounter" => "${player_name} uses ${card_name}'s counter",
        "switchPlayer" | "newFirstPlayer" => "${player_name} becomes First player",
        "startDusk" => "Computing the progress in the Tumult",
        "winTieBreaker" => "${player_name} wins the tiebreaker with ${n} attributes",
        "startTiebreak" => "The tiebreaker is triggered as heroes reached their companions at the same time",
        "cleanupCards" => "All remaining cards of ${player_name} loose Anchored and Asleep",
        "nightCleanup" => "${player_name} cleans up its Reserve and Landmarks",
        "moveCard" => "${player_name} moves ${card_name} to opposite expedition (${card_name2}'s effect)",
        "defect" => "${card_name} defects to ${player_name}'s side",
        "afterYou" => "${player_name} triggers After You effect",
        "blockExpedition" => "${player_name} blocks ${player_name2} ${expedition}'s expedition until next Day",
        "blockAllExpeditions" => "${player_name} blocks all expeditions until next Day",
        "reveal" | "revealHand" => "${card_name} is revealed",
        "shuffleDeck" => "${player_name} exhausts its deck and shuffles the discard",
        "ready" => "${player_name} readies ${card_name}",
        "publicReadyMana" => "${player_name} readies a mana orb",
        "setTerrainMarker" => "${player_name} places a terrain marker on a region",
        "moveTerrainMarker" => "${player_name} moves a terrain marker on a region",
        "ascend" => "${player_name}'s expedition ascends",
        "newPhase" => "Day ${day}: ${phase}",
        _ => "",
    }
}

fn card_name(g: &Game, id: &Value) -> Option<String> {
    let id = if php::is_arr(id) { php::get(id, "id") } else { id };
    if !php::is_numeric(id) {
        return None;
    }
    g.card(php::intval(id)).ok().map(|c| php::strval(c.props.get("name").unwrap_or(&Value::Null)))
}

/// The log line of a notification (V1::log: `format($msg, $args + you)`, kept when not empty):
/// `Notifications::updateArgs` names (player_name, card_name, card_names...) are added from the
/// ids the translation sends
pub fn log_line(g: &Game, n: &Value) -> Option<LogLine> {
    let name = php::strval(php::get(n, "name"));
    let mut args = php::get(n, "args").clone();
    if !args.is_object() {
        args = Value::Object(Map::new());
    }
    let msg = if php::isset(n, "msg") {
        php::strval(php::get(n, "msg"))
    } else if name == "message" || name == "mediumMessage" {
        php::strval(php::coalesce(php::get(&args, "log"), php::get(&args, "msg")))
    } else {
        notification_msg(&name, &args).to_string()
    };
    if msg.trim().is_empty() {
        return None;
    }
    let target = php::get(n, "target");
    let target = if target.is_null() { None } else { Some(php::intval(target)) };
    // updateArgs
    let a = args.as_object_mut().unwrap();
    for (id_key, name_key) in [("player_id", "player_name"), ("player_id2", "player_name2")] {
        if let Some(p) = a.get(id_key).cloned() {
            if !a.contains_key(name_key) {
                if let Some(row) = g.players.get(&php::intval(&p)) {
                    a.insert(name_key.into(), json!(row.name));
                }
            }
        }
    }
    for (id_key, name_key) in [("cardId", "card_name"), ("card", "card_name"), ("card2", "card_name2"), ("sourceId", "card_name2"), ("source", "card_name2")] {
        if a.contains_key(name_key) {
            continue;
        }
        if let Some(c) = a.get(id_key).cloned() {
            if let Some(nm) = card_name(g, &c) {
                a.insert(name_key.into(), json!(nm));
            }
        }
    }
    if let Some(ids) = a.get("cardIds").cloned() {
        let names: Vec<String> = php::vals(&ids).into_iter().filter_map(|c| card_name(g, c).map(|n| format!("[[{}]]", n))).collect();
        a.entry("card_names").or_insert(json!(names.join(", ")));
        a.entry("n").or_insert(json!(php::count(&ids)));
    }
    if let Some(m) = a.get("meeples").cloned() {
        a.entry("n").or_insert(json!(php::count(&m)));
    }
    if let Some(r) = a.get("rolls").cloned() {
        let rolls: Vec<String> = php::vals(&r).into_iter().map(php::strval).collect();
        a.entry("roll_text").or_insert(json!(rolls.join(", ")));
    }
    if let Some(e) = a.get("expedition").cloned() {
        if e == json!("stormLeft") || e == json!("hero") {
            a.insert("expedition".into(), json!("Hero"));
        } else if e == json!("stormRight") || e == json!("companion") {
            a.insert("expedition".into(), json!("Companion"));
        }
    }
    if name == "moveStormToken" {
        let t = php::strval(a.get("token").unwrap_or(&Value::Null));
        let side = g.meeples.get(&php::intval(&json!(t))).map(|m| m.kind.clone()).unwrap_or_default();
        a.insert("expedition".into(), json!(if side == "companion" { "Companion" } else { "Hero" }));
    }
    if let Some(l) = a.get("displayLocation").cloned() {
        let s = php::strval(&l);
        let d = if s == "stormLeft" { "Hero's expedition" } else if s == "landmark" { "Landmark's zone" } else { "Companion's expedition" };
        a.insert("displayLocation".into(), json!(d));
    }
    if name == "newPhase" {
        let day = php::intval(g.globals.get("day").unwrap_or(&Value::Null));
        a.insert("day".into(), json!(day));
    }
    let player = a.get("player_id").map(php::intval);
    a.insert("you".into(), json!("You"));
    let text = format(&msg, &args, 0).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(LogLine { seq: php::intval(php::get(n, "seq")).max(0) as u64, text, player, target })
}
