//! The local BGA framework (server/framework/module/table/table.game.php): the state machine of
//! states.inc.php, active / multiactive players, player order.

use crate::game::{Game, PId};
use crate::game_methods;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct StateDef {
    pub id: i64,
    pub name: &'static str,
    pub kind: &'static str,
    pub action: Option<&'static str>,
    pub args: Option<&'static str>,
    pub possible: Option<&'static [&'static str]>,
    pub transitions: &'static [(&'static str, i64)],
    pub progression: bool,
}

// constants.inc.php
pub const ST_GAME_SETUP: i64 = 1;
pub const ST_PRECO_DECK_SELECTION: i64 = 2;
pub const ST_DECK_SELECTION: i64 = 3;
pub const ST_SETUP: i64 = 5;
pub const ST_FIRST_DAY: i64 = 10;
pub const ST_FIRST_DAY_MULTI: i64 = 11;
pub const ST_NEW_DAY: i64 = 12;
pub const ST_BEFORE_ASSIGNMENT: i64 = 18;
pub const ST_ASSIGNMENT: i64 = 19;
pub const ST_CHOOSE_ASSIGNMENT: i64 = 20;
pub const ST_DISCARD: i64 = 21;
pub const ST_GAIN: i64 = 22;
pub const ST_TARGET: i64 = 23;
pub const ST_LOOSE: i64 = 24;
pub const ST_SPELL_CLEANUP: i64 = 25;
pub const ST_INVOKE_TOKEN: i64 = 26;
pub const ST_ACTIVATE_CARD: i64 = 27;
pub const ST_DRAW: i64 = 28;
pub const ST_SPECIAL_EFFECT: i64 = 29;
pub const ST_CHECK_CONDITION: i64 = 30;
pub const ST_AFTER_YOU: i64 = 31;
pub const ST_ROLL_DIE: i64 = 32;
pub const ST_RESUPPLY: i64 = 33;
pub const ST_MOVE_EXPEDITION: i64 = 34;
pub const ST_USE_COUNTER: i64 = 35;
pub const ST_ACTIVATE_EFFECT: i64 = 36;
pub const ST_TAP: i64 = 37;
pub const ST_PLAY_CARD: i64 = 38;
pub const ST_MOVE_CARD: i64 = 39;
pub const ST_PAY: i64 = 40;
pub const ST_DRAW_MANA: i64 = 41;
pub const ST_BLOCK_EXPEDITION: i64 = 42;
pub const ST_TARGET_PLAYER: i64 = 43;
pub const ST_DISCARD_DO: i64 = 44;
pub const ST_TARGET_EXPEDITION: i64 = 45;
pub const ST_EXHAUST: i64 = 46;
pub const ST_READY: i64 = 47;
pub const ST_EXCHANGE: i64 = 48;
pub const ST_SPEND: i64 = 49;
pub const ST_BOOST_EXCHANGE: i64 = 50;
pub const ST_END_AFTERNOON: i64 = 51;
pub const ST_MARK_REGION: i64 = 52;
pub const ST_MOVE_REGION_MARKER: i64 = 53;
pub const ST_INTERRUPT_REVEAL: i64 = 54;
pub const ST_SHUFFLE: i64 = 55;
pub const ST_COMPLETE_FEAT: i64 = 56;
pub const ST_DISCARD_FROM_DECK: i64 = 57;
pub const ST_PRE_DUSK_PHASE: i64 = 83;
pub const ST_DUSK: i64 = 84;
pub const ST_BEFORE_NIGHT: i64 = 85;
pub const ST_AFTER_DUSK: i64 = 87;
pub const ST_NIGHT_CLEANUP: i64 = 88;
pub const ST_END_OF_NIGHT: i64 = 89;
pub const ST_RESOLVE_STACK: i64 = 90;
pub const ST_RESOLVE_CHOICE: i64 = 91;
pub const ST_IMPOSSIBLE_MANDATORY_ACTION: i64 = 92;
pub const ST_CONFIRM_TURN: i64 = 93;
pub const ST_CONFIRM_PARTIAL_TURN: i64 = 94;
pub const ST_GENERIC_NEXT_PLAYER: i64 = 97;
pub const ST_PRE_END_OF_GAME: i64 = 98;
pub const ST_END_GAME: i64 = 99;

const fn st(id: i64, name: &'static str, kind: &'static str, action: Option<&'static str>, args: Option<&'static str>, possible: Option<&'static [&'static str]>, transitions: &'static [(&'static str, i64)]) -> StateDef {
    StateDef { id, name, kind, action, args, possible, transitions, progression: false }
}

const ATOMIC: Option<&str> = Some("stAtomicAction");
const ARGS_ATOMIC: Option<&str> = Some("argsAtomicAction");
const PASS_ONLY: Option<&[&str]> = Some(&["actPassOptionalAction"]);

/// states.inc.php `$machinestates` (state 50 is boostExchange: the later key wins)
pub static STATES: &[StateDef] = &[
    st(ST_GAME_SETUP, "gameSetup", "manager", Some("stGameSetup"), None, None, &[("", ST_PRECO_DECK_SELECTION)]),
    st(ST_GENERIC_NEXT_PLAYER, "genericNextPlayer", "game", None, None, None, &[]),
    st(ST_PRECO_DECK_SELECTION, "selectPrecoDeck", "multipleactiveplayer", None, Some("argsPrecoDeckSelection"), Some(&["actSelectPrecoDeck", "actSelectRandomDeck", "actConfirmAPIDeck", "actLoadAPIDecks", "actCancelPrecoDeckSelection"]), &[("done", ST_SETUP), ("zombiePass", ST_SETUP)]),
    st(ST_DECK_SELECTION, "selectDeck", "multipleactiveplayer", None, None, Some(&["actSelectDeck", "actCancelDeckSelection"]), &[("done", ST_SETUP), ("zombiePass", ST_SETUP)]),
    st(ST_SETUP, "deckSetup", "game", Some("stDeckSetup"), None, None, &[("", ST_FIRST_DAY)]),
    st(ST_FIRST_DAY, "firstDay", "game", Some("stFirstDay"), None, None, &[("done", ST_FIRST_DAY_MULTI)]),
    st(ST_FIRST_DAY_MULTI, "firstDayManaSelection", "multipleactiveplayer", None, Some("argsFirstDayManaSelection"), Some(&["actFirstDayManaSelection", "actCancelFirstDayManaSelection"]), &[("done", ST_BEFORE_ASSIGNMENT), ("zombiePass", ST_BEFORE_ASSIGNMENT)]),
    st(ST_NEW_DAY, "newDay", "game", Some("stNewDay"), None, None, &[]),
    StateDef { id: ST_BEFORE_ASSIGNMENT, name: "beforeAssignment", kind: "game", action: Some("stBeforeAssignment"), args: None, possible: None, transitions: &[], progression: true },
    st(ST_ASSIGNMENT, "assigment", "game", Some("stAssignment"), None, None, &[("done", ST_PRE_DUSK_PHASE)]),
    st(ST_PRE_DUSK_PHASE, "beforeDusk", "game", Some("stBeforeDusk"), None, None, &[("done", ST_DUSK)]),
    st(ST_DUSK, "dusk", "game", Some("stDusk"), None, None, &[("done", ST_AFTER_DUSK)]),
    st(ST_AFTER_DUSK, "afterDusk", "game", Some("stAfterDusk"), None, None, &[("done", ST_BEFORE_NIGHT)]),
    StateDef { id: ST_BEFORE_NIGHT, name: "beforeNight", kind: "game", action: Some("stBeforeNight"), args: None, possible: None, transitions: &[("done", ST_NIGHT_CLEANUP), ("newDay", ST_BEFORE_ASSIGNMENT)], progression: true },
    st(ST_END_OF_NIGHT, "endOfNight", "game", Some("stEndOfNight"), None, None, &[("newDay", ST_BEFORE_ASSIGNMENT)]),
    st(ST_CHOOSE_ASSIGNMENT, "chooseAssignment", "activeplayer", None, ARGS_ATOMIC, Some(&["actPlay", "actSupport", "actTap", "actPass", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_GAIN, "gain", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_LOOSE, "loose", "game", ATOMIC, None, None, &[]),
    st(ST_DRAW, "draw", "game", ATOMIC, None, None, &[]),
    st(ST_SPECIAL_EFFECT, "specialEffect", "game", ATOMIC, None, None, &[]),
    st(ST_CHECK_CONDITION, "checkCondition", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_AFTER_YOU, "afterYou", "game", ATOMIC, None, None, &[]),
    st(ST_DRAW_MANA, "drawMana", "game", ATOMIC, None, None, &[]),
    st(ST_USE_COUNTER, "useCounter", "game", ATOMIC, None, None, &[]),
    st(ST_ROLL_DIE, "rollDie", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actRollDie", "actDiscardAdd", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_RESUPPLY, "resupply", "game", ATOMIC, None, None, &[]),
    st(ST_MOVE_EXPEDITION, "moveExpedition", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actMoveExpedition", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_DISCARD, "discard", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actDiscard", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_NIGHT_CLEANUP, "nightCleanup", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actNightCleanup"]), &[]),
    st(ST_TARGET, "target", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actTarget", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_DISCARD_DO, "discardDo", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actTarget", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_TARGET_PLAYER, "targetPlayer", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actTargetPlayer", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_SPELL_CLEANUP, "spellCleanup", "game", ATOMIC, None, None, &[]),
    st(ST_INVOKE_TOKEN, "invokeToken", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actInvokeToken", "actInvokeTokenPass", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_BLOCK_EXPEDITION, "blockExpedition", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actBlockExpedition", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_TARGET_EXPEDITION, "targetExpedition", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actTargetExpedition", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_PLAY_CARD, "playCard", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actPlayCard", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_ACTIVATE_CARD, "activateCard", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_MOVE_CARD, "moveCard", "game", ATOMIC, None, None, &[]),
    st(ST_ACTIVATE_EFFECT, "activateEffect", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_TAP, "tap", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_PAY, "pay", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actPay", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_EXHAUST, "exhaust", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_READY, "ready", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_EXCHANGE, "exchange", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actExchange", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_SPEND, "spend", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actSpend", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_BOOST_EXCHANGE, "boostExchange", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actBoostExchange", "actConfirmTurn", "actRestart", "actPassOptionalAction"]), &[]),
    st(ST_END_AFTERNOON, "endAfternoon", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_MARK_REGION, "markRegion", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actMarkRegion", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_MOVE_REGION_MARKER, "moveRegionMarker", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actMoveRegionMarker", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_INTERRUPT_REVEAL, "interuptReveal", "activeplayer", ATOMIC, ARGS_ATOMIC, Some(&["actInteruptReveal", "actPassOptionalAction", "actConfirmTurn", "actRestart"]), &[]),
    st(ST_SHUFFLE, "shuffle", "game", ATOMIC, None, None, &[]),
    st(ST_COMPLETE_FEAT, "completeFeat", "game", ATOMIC, None, PASS_ONLY, &[]),
    st(ST_DISCARD_FROM_DECK, "discardFromDeck", "game", ATOMIC, None, None, &[]),
    st(ST_RESOLVE_STACK, "resolveStack", "game", Some("stResolveStack"), None, None, &[]),
    st(ST_CONFIRM_TURN, "confirmTurn", "activeplayer", Some("stConfirmTurn"), Some("argsConfirmTurn"), Some(&["actConfirmTurn", "actRestart"]), &[("endOfDay", ST_CONFIRM_TURN)]),
    st(ST_CONFIRM_PARTIAL_TURN, "confirmPartialTurn", "activeplayer", None, Some("argsConfirmTurn"), Some(&["actConfirmPartialTurn", "actRestart"]), &[]),
    st(ST_RESOLVE_CHOICE, "resolveChoice", "activeplayer", Some("stResolveChoice"), Some("argsResolveChoice"), Some(&["actChooseAction", "actRestart"]), &[]),
    st(ST_IMPOSSIBLE_MANDATORY_ACTION, "impossibleAction", "activeplayer", None, Some("argsImpossibleAction"), Some(&["actRestart"]), &[]),
    StateDef { id: ST_PRE_END_OF_GAME, name: "preEndOfGame", kind: "game", action: Some("stPreEndOfGame"), args: None, possible: None, transitions: &[("", ST_END_GAME)], progression: true },
    st(ST_END_GAME, "gameEnd", "manager", Some("stGameEnd"), Some("argGameEnd"), None, &[]),
];

pub fn state_def(id: i64) -> Option<&'static StateDef> {
    STATES.iter().find(|s| s.id == id)
}

/// `GameState::state_id()` (global 1)
pub fn state_id(g: &Game) -> i64 {
    g.fw_get(1).unwrap_or(0)
}

/// The current state (`$this->states[$id] ?? []`)
pub fn state(g: &Game) -> Option<&'static StateDef> {
    state_def(state_id(g))
}

fn is_player_state(s: &StateDef) -> bool {
    s.kind == "activeplayer" || s.kind == "multipleactiveplayer"
}

/// `GameState::state()`: the current state with its args when a player has to act
pub fn state_with_args(g: &mut Game) -> R<(Option<&'static StateDef>, Value)> {
    let s = state(g);
    let args = match s {
        Some(s) if s.args.is_some() && is_player_state(s) => compute_args(g, s)?,
        _ => Value::Null,
    };
    Ok((s, args))
}

fn compute_args(g: &mut Game, s: &StateDef) -> R<Value> {
    game_methods::call_args(g, s.args.unwrap())
}

/// `GameState::jumpToState($stateId)`: runs the state's action synchronously, then its args if
/// the state did not change in the meantime
// PHP: server/framework/module/table/table.game.php:286
pub fn jump_to_state(g: &mut Game, id: &Value) -> R<()> {
    let sid = match id {
        Value::Number(_) => php::intval(id),
        Value::String(s) if php::int_key(s).is_some() => php::intval(id),
        _ => return Err(PhpError::new(format!("jumpToState: unknown state {}", php::strval(id)))),
    };
    let Some(s) = state_def(sid) else {
        return Err(PhpError::new(format!("jumpToState: unknown state {}", sid)));
    };
    g.fw_set(1, sid);
    let active = g.active_player_id();
    g.notify("gameStateChange", json!({"id": sid, "active_player": active}));
    if s.progression {
        // getGameProgression() only reads the game (progress bar)
    }
    if let Some(action) = s.action {
        game_methods::call_method(g, action, &Value::Null)?;
    }
    if state_id(g) == sid && is_player_state(s) && s.args.is_some() {
        let a = compute_args(g, s)?;
        g.last_args = Some(a);
    }
    Ok(())
}

pub fn jump(g: &mut Game, id: i64) -> R<()> {
    jump_to_state(g, &json!(id))
}

/// `GameState::nextState($transition)`
// PHP: server/framework/module/table/table.game.php:308
pub fn next_state(g: &mut Game, transition: &str) -> R<()> {
    let s = state(g).ok_or_else(|| PhpError::new("Undefined array key (nextState)"))?;
    let next = if let Some((_, n)) = s.transitions.iter().find(|(t, _)| *t == transition) {
        *n
    } else if s.transitions.len() == 1 {
        s.transitions[0].1
    } else {
        return Err(PhpError::new(format!("nextState: no transition '{}' from state {}", transition, s.name)));
    };
    jump(g, next)
}

/// `GameState::checkPossibleAction($action)`
pub fn check_possible_action(g: &Game, action: &str) -> R<()> {
    if let Some(s) = state(g) {
        if let Some(p) = s.possible {
            if !p.contains(&action) {
                return Err(PhpError::new(format!("Impossible action {} in state {}", action, s.name)));
            }
        }
    }
    Ok(())
}

/// `Table::checkAction($action, $bThrowException)`
pub fn check_action(g: &Game, action: &str, throw: bool) -> R<bool> {
    match check_possible_action(g, action) {
        Ok(()) => Ok(true),
        Err(e) => {
            if throw {
                Err(e)
            } else {
                Ok(false)
            }
        }
    }
}

/// `GameState::changeActivePlayer($pId)`
pub fn change_active_player(g: &mut Game, pid: &Value) {
    g.fw_set(2, php::intval(pid));
}

/// `GameState::setAllPlayersMultiactive()`
pub fn set_all_players_multiactive(g: &mut Game) {
    for p in g.players.values_mut() {
        if php::intval(&json!(p.zombie)) == 0 {
            p.is_multiactive = true;
        }
    }
}

/// `GameState::setPlayersMultiactive($players, $nextState, $bExclusive)`
pub fn set_players_multiactive(g: &mut Game, players: &[PId], next: &str, exclusive: bool) -> R<bool> {
    if exclusive {
        for p in g.players.values_mut() {
            p.is_multiactive = false;
        }
    }
    for id in players {
        if let Some(p) = g.players.get_mut(id) {
            p.is_multiactive = true;
        }
    }
    if get_active_player_list(g).is_empty() {
        next_state(g, next)?;
        return Ok(true);
    }
    Ok(false)
}

/// `GameState::setPlayerNonMultiactive($pId, $nextState)`
pub fn set_player_non_multiactive(g: &mut Game, pid: PId, next: &str) -> R<bool> {
    if let Some(p) = g.players.get_mut(&pid) {
        p.is_multiactive = false;
    }
    if get_active_player_list(g).is_empty() {
        next_state(g, next)?;
        return Ok(true);
    }
    Ok(false)
}

/// `GameState::getActivePlayerList()`
pub fn get_active_player_list(g: &Game) -> Vec<PId> {
    g.players.values().filter(|p| p.is_multiactive).map(|p| p.id).collect()
}

/// `GameState::isPlayerActive($pId)`
pub fn is_player_active(g: &Game, pid: PId) -> bool {
    let Some(s) = state(g) else { return false };
    if s.kind == "multipleactiveplayer" {
        return get_active_player_list(g).contains(&pid);
    }
    s.kind == "activeplayer" && g.active_player_id() == pid
}

/// `Table::loadPlayersBasicInfos()` order: seat order, else `player_no, player_id`
pub fn players_in_order(g: &Game) -> Vec<PId> {
    if !g.seat_order.is_empty() {
        let mut ids: Vec<PId> = g.players.keys().copied().collect();
        // FIELD(player_id, ...): ids not listed come first
        ids.sort_by_key(|id| g.seat_order.iter().position(|s| s == id).map(|p| p as i64 + 1).unwrap_or(0));
        return ids;
    }
    let mut ps: Vec<(i64, PId)> = g.players.values().map(|p| (p.no, p.id)).collect();
    ps.sort();
    ps.into_iter().map(|(_, id)| id).collect()
}

/// `Table::getNextPlayerTable()`: `[0 => firstSeat, pId => nextPId]`
pub fn get_next_player_table(g: &Game) -> Value {
    let ids = players_in_order(g);
    let mut t = php::arr();
    if ids.is_empty() {
        return t;
    }
    php::seti(&mut t, 0, json!(ids[0]));
    for (i, id) in ids.iter().enumerate() {
        php::seti(&mut t, *id, json!(ids[(i + 1) % ids.len()]));
    }
    t
}

/// `Table::getPrevPlayerTable()`
pub fn get_prev_player_table(g: &Game) -> Value {
    let ids = players_in_order(g);
    let mut t = php::arr();
    let n = ids.len();
    for (i, id) in ids.iter().enumerate() {
        php::seti(&mut t, *id, json!(ids[(i + n - 1) % n]));
    }
    t
}

/// `Table::activeNextPlayer()`
pub fn active_next_player(g: &mut Game) -> Value {
    let t = get_next_player_table(g);
    let cur = g.active_player_id();
    let next = php::get(&t, &cur.to_string()).clone();
    let next = if next.is_null() { php::geti(&t, 0).clone() } else { next };
    change_active_player(g, &next);
    next
}
