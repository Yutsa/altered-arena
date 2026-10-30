//! server/src/Arena.php (game creation, requests, notifications, bots) and V1::createGame: the
//! arena that runs the engine for the game server

use crate::bot;
use crate::framework;
use crate::game::{Game, LogLine, PId};
use crate::php;
use crate::v1;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `Arena::PLAYER_IDS`
pub const PLAYER_IDS: [PId; 2] = [2001, 2002];

/// A seat of a new game (protocol v1 NewGame player)
#[derive(Clone, Debug, Default)]
pub struct NewPlayer {
    pub name: String,
    pub bot: bool,
    /// Starter deck id (protocol/data/decks.json), selected right away for a human player
    pub deck: Option<String>,
    /// Custom deck content (`apiDeck`: what `actConfirmAPIDeck` receives), confirmed right away
    /// for a human player
    pub api_deck: Option<Value>,
}

/// `substr(preg_replace('/[^\p{L}\p{N} _-]/u', '', $name) ?: 'Player n', 0, 32)`
pub fn clean_name(name: &str, i: usize) -> String {
    let n: String = name.chars().filter(|c| c.is_alphabetic() || c.is_numeric() || " _-".contains(*c)).collect();
    let n = if n.is_empty() { format!("Player {}", i + 1) } else { n };
    let mut end = n.len().min(32);
    while !n.is_char_boundary(end) {
        end -= 1;
    }
    n[..end].to_string()
}

/// Options of a game created by the arena: `[151 => 1]` (undo allowed), the others take their
/// default from gameoptions.json (150: 0, 155: 0)
pub fn default_options() -> Value {
    json!({"150": 0, "151": 1, "155": 0})
}

/// `Table::harnessCreate($players, $options)`: setupNewGame, seat numbers, then the state
/// machine starts at state 1
// PHP: server/framework/module/table/table.game.php:626
pub fn harness_create(g: &mut Game, players: &[(PId, String)], options: &Value) -> R<()> {
    g.seat_order = players.iter().map(|(id, _)| *id).collect();
    // new static(): Engine::boot()
    crate::core::engine::boot(g)?;
    crate::states::setup::setup_new_game(g, players, options)?;
    let order = framework::players_in_order(g);
    for (no, pid) in order.iter().enumerate() {
        if let Some(p) = g.players.get_mut(pid) {
            p.no = no as i64 + 1;
        }
    }
    framework::jump(g, framework::ST_GAME_SETUP)
}

/// `Arena::createGame($config)`: the game tables, then the bots play (deck choice...)
// PHP: server/src/Arena.php:87
pub fn create_game(game_id: i64, players: &[NewPlayer], dice: &[i64]) -> R<Game> {
    if players.len() != 2 {
        return Err(PhpError::new("A game needs 2 players"));
    }
    let mut g = Game::empty();
    let seats: Vec<(PId, String)> = players.iter().enumerate().map(|(i, p)| (PLAYER_IDS[i], clean_name(&p.name, i))).collect();
    g.bots = players.iter().enumerate().filter(|(_, p)| p.bot).map(|(i, _)| PLAYER_IDS[i]).collect();
    g.dice = std::sync::Arc::new(dice.iter().map(|d| (*d).clamp(1, 6)).collect());
    g.environment = if g.dice.is_empty() { "studio".into() } else { "prod".into() };
    // arena_rng: dice cursor, game id and number of acts (seedRng)
    g.dice_used = 0;
    g.rng_game = game_id;
    g.rng_acts = 0;
    // server/schema.sql: INSERT INTO `global` VALUES (1, 1), (2, 0), (3, 0), (10, 0)
    for (k, v) in [(1, 1), (2, 0), (3, 0), (10, 0)] {
        g.fw_set(k, v);
    }
    // request(null, harnessCreate, act: true)
    g.seed_rng();
    g.fresh_state(None)?;
    harness_create(&mut g, &seats, &default_options())?;
    store_notifications(&mut g);
    run_bots(&mut g)?;
    Ok(g)
}

/// `V1::createGame($body)`: the game, then the decks chosen in the lobby are selected right away
/// (each selection is an act of the player, after a view of the game); a custom deck (`apiDeck`)
/// is confirmed with `actConfirmAPIDeck`
// PHP: server/src/V1.php:40
pub fn create_v1_game(game_id: i64, players: &[NewPlayer], dice: &[i64]) -> R<Game> {
    let mut g = create_game(game_id, players, dice)?;
    for (i, p) in players.iter().enumerate() {
        if p.bot {
            continue;
        }
        let pid = PLAYER_IDS[i];
        if let Some(api) = p.api_deck.as_ref().filter(|d| !php::empty(d)) {
            v1::build(&mut g, Some(pid), "")?;
            v1::act(&mut g, pid, "actConfirmAPIDeck", &json!([api]))?;
            run_bots(&mut g)?;
            continue;
        }
        let Some(deck) = p.deck.as_deref().filter(|d| !d.is_empty()) else { continue };
        // Arena::view (getAllDatas creates the missing preferences and globals)
        v1::build(&mut g, Some(pid), "")?;
        let mut view = g.clone();
        let raw = v1::view_state(&mut view, pid)?;
        let decks = php::get(php::get(&raw.args, "_private"), "decks").clone();
        for d in php::vals(&decks) {
            if php::loose_eq(php::get(d, "deckId"), &json!(deck)) {
                v1::act(&mut g, pid, "actSelectPrecoDeck", &json!([php::get(d, "deckNumber")]))?;
                run_bots(&mut g)?;
            }
        }
    }
    Ok(g)
}

/// `Arena::storeNotifications()`: the notifications of the request that have a text go to the
/// game log (the latest 200 are kept)
// PHP: server/src/Arena.php:239
pub fn store_notifications(g: &mut Game) {
    let notifs = std::mem::take(&mut g.notifications);
    for n in &notifs {
        if let Some(line) = v1::log_line(g, n) {
            g.history.push(std::sync::Arc::new(line));
        }
    }
    g.notifications = notifs;
    let n = g.history.len();
    if n > 200 {
        g.history.drain(..n - 200);
    }
}

/// One request of a bot (`Arena::request($botId, ..., true)`): rolled back on error
fn bot_act(g: &mut Game, bot: PId, action: &str, args: &[Value]) -> R<()> {
    let backup = g.clone();
    let res = (|| {
        g.seed_rng();
        g.begin_request(Some(bot))?;
        v1::call_action(g, action, args)
    })();
    match res {
        Ok(()) => {
            store_notifications(g);
            Ok(())
        }
        Err(e) => {
            *g = backup;
            Err(e)
        }
    }
}

/// `Arena::runBots($gameId)`: bot players take every decision that is theirs, one request per
/// decision (at most 200)
// PHP: server/src/Arena.php:362
pub fn run_bots(g: &mut Game) -> R<()> {
    for _ in 0..200 {
        if !bot_step(g)? {
            return Ok(());
        }
    }
    Ok(())
}

/// One decision of a bot player, as `run_bots` takes it (the server paces them). False: no bot
/// has a decision, or none of its answers was accepted (the game waits for the bot).
pub fn bot_step(g: &mut Game) -> R<bool> {
    if g.bots.is_empty() {
        return Ok(false);
    }
    let bots = g.bots.clone();
    // request(null): which bot decides what (the draws of the bot continue the generator)
    g.begin_request(None)?;
    let mut decision = None;
    for b in &bots {
        if framework::is_player_active(g, *b) {
            decision = Some((*b, bot::decide(g, *b)?));
            break;
        }
    }
    let Some((bot_id, call)) = decision else { return Ok(false) };
    let Some((action, args)) = call else {
        // Bot.php has nothing to answer: PHP stops here (the game waits for the bot)
        return last_resort(g, bot_id);
    };
    if let Err(e) = bot_act(g, bot_id, &action, &args) {
        // A bad bot choice must not block the game: try the fallback once, else stop
        eprintln!("bot {} {} {}: {}", bot_id, action, Value::Array(args.clone()), e);
        g.begin_request(None)?;
        let fallback = bot::fallback(g, bot_id)?;
        let ok = match fallback {
            Some((action, args)) => match bot_act(g, bot_id, &action, &args) {
                Ok(()) => true,
                Err(e2) => {
                    eprintln!("bot {} fallback failed: {}", bot_id, e2);
                    false
                }
            },
            None => false,
        };
        if !ok {
            return last_resort(g, bot_id);
        }
    }
    Ok(true)
}

/// Not in Arena.php (whose runBots stops, leaving the game waiting for the bot): the answers of
/// `bot::last_resort` until one is accepted. False when none is.
fn last_resort(g: &mut Game, bot_id: PId) -> R<bool> {
    for (action, args) in bot::last_resort(g, bot_id)? {
        if bot_act(g, bot_id, &action, &args).is_ok() {
            return Ok(true);
        }
    }
    eprintln!("bot {}: no answer accepted", bot_id);
    Ok(false)
}

/// The log as `V1::log` reads it: the latest 100 lines that `you` may see
pub fn log_for(g: &Game, you: Option<PId>) -> Vec<&LogLine> {
    let visible: Vec<&LogLine> = g.history.iter().map(|l| &**l).filter(|l| l.target.is_none() || l.target == you).collect();
    let from = visible.len().saturating_sub(100);
    visible[from..].to_vec()
}
