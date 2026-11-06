//! The rules engine (`rules/`, the translation of the PHP engine) as the server keeps a game.
//!
//! The engine runs requests as PHP does: whole chains of states recursively (jumpToState is
//! re-entrant), so its calls run on worker threads with large stacks (`EnginePool`).

use altered_ai::legal;
use altered_ai::record::{self, Event, Record};
use altered_rules::arena::NewPlayer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// A seat of a game of the `rules` engine (the engine knows the players by id; the server keeps
/// names, bots and tokens)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeatInfo {
    pub id: u32,
    pub name: String,
    pub bot: bool,
    #[serde(default)]
    pub token: String,
    /// A bot seat the server plays with an AI (`ais`): the AI's id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<String>,
}

/// A seat of a new game
pub struct NewSeat {
    pub name: String,
    pub bot: bool,
    /// Starter deck id (protocol/data/decks.json), else chosen in game
    pub deck: Option<String>,
    /// An AI seat (a bot the server plays, not the engine): the AI's id and its decklist
    pub ai: Option<(String, Value)>,
    /// An Altered Reunion deck, as the publisher API answers it (`reunion::publisher_response`)
    pub reunion_deck: Option<Value>,
}

/// A game of the `rules` engine as the server keeps it (and saves it)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RulesGame {
    pub engine: String,
    pub id: String,
    pub seats: Vec<SeatInfo>,
    pub game: altered_rules::Game,
    /// The game as created and every change made to it since (`altered_ai::record`): any
    /// position of the game can be rebuilt
    #[serde(default)]
    pub record: Record,
    /// Added to the engine's `notif_seq` in the views: an undo rebuilds an earlier position (a
    /// lower `notif_seq`), and the seq clients poll with must still increase
    #[serde(default)]
    pub seq_base: u64,
}

/// The answer `undo` (the engine's undo) is refused: the arena offers only the server's undo
/// (`ServerGame::undo`), in games against bots
const ENGINE_UNDO_REFUSED: &str = "Undo is only possible against bots, with the Undo button";

impl RulesGame {
    /// Plays `e` (as `record::apply`) and records it once it is accepted
    fn play(&mut self, e: Event) -> Result<bool, String> {
        // The engine's undo (BGA's, `actRestart`) is not offered: only the server's, against bots
        if let Event::Answer { body, .. } | Event::Ai { body, .. } = &e {
            if body["type"] == json!("undo") {
                return Err(ENGINE_UNDO_REFUSED.into());
            }
        }
        let r = record::apply(&mut self.game, &e)?;
        self.record.events.push(e);
        Ok(r)
    }
}

/// A game in the server (boxed: the engine state is large)
#[derive(Clone)]
pub struct ServerGame(Box<RulesGame>);

/// An answer refused: HTTP status, message, and whether the player can retry
pub struct Refused {
    pub status: u16,
    pub msg: String,
    pub user: bool,
}

/// Player seat of the request (None: spectator)
pub type ViewSeat = Option<usize>;

/// Die results of a game of the `rules` engine: the arena takes them in order (cycling), the
/// BGA studio rule (every die a 5) applies only without a list
fn dice_from_seed(seed: u64) -> Vec<i64> {
    let mut mt = altered_rules::php_rand::PhpMt::new((seed ^ (seed >> 32)) as u32);
    (0..1000).map(|_| mt.range(1, 6)).collect()
}

impl ServerGame {
    /// A new game: players 2001 and 2002, decks chosen in the lobby (else in game)
    pub fn new(id: String, players: [NewSeat; 2], seed: u64) -> Result<ServerGame, String> {
        // An AI seat is a player of the engine with the AI's deck, the server answers its decisions
        let new: Vec<NewPlayer> = players
            .iter()
            .map(|p| match &p.ai {
                Some((_, deck)) => NewPlayer { name: p.name.clone(), bot: false, deck: None, api_deck: Some(deck.clone()), reunion_deck: None },
                None => NewPlayer { name: p.name.clone(), bot: p.bot, deck: p.deck.clone(), api_deck: None, reunion_deck: p.reunion_deck.clone() },
            })
            .collect();
        // The game id of the arena seeds the randomness of each act (Arena::seedRng)
        let game_id = (seed & 0x7FFF_FFFF) as i64;
        let game = altered_rules::Game::create(game_id, &new, &dice_from_seed(seed)).map_err(|e| e.0)?;
        let seats = players
            .iter()
            .enumerate()
            .map(|(i, p)| SeatInfo {
                id: 2001 + i as u32,
                // The engine keeps only letters, digits, spaces, _ and - of a name: an AI's label ("player — hero") stays the server's
                name: if p.ai.is_some() { p.name.clone() } else { game.players.get(&(2001 + i as i64)).map(|p| p.name.clone()).unwrap_or_default() },
                bot: p.bot || p.ai.is_some(),
                token: String::new(),
                ai: p.ai.as_ref().map(|(id, _)| id.clone()),
            })
            .collect();
        let record = Record::start(&game);
        Ok(ServerGame(Box::new(RulesGame { engine: "rules".into(), id, seats, game, record, seq_base: 0 })))
    }

    pub fn load(bytes: &[u8]) -> Option<ServerGame> {
        let g: RulesGame = serde_json::from_slice(bytes).ok()?;
        (g.engine == "rules").then(|| ServerGame(Box::new(g)))
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(&self.0).unwrap_or_default()
    }

    pub fn id(&self) -> &str {
        &self.0.id
    }

    /// (player id, name, bot, token) per seat
    pub fn seats(&self) -> Vec<(u32, String, bool, String)> {
        self.0.seats.iter().map(|s| (s.id, s.name.clone(), s.bot, s.token.clone())).collect()
    }

    /// Gives each human seat without a token a new one
    pub fn set_tokens(&mut self, mut new_token: impl FnMut() -> String) {
        for s in self.0.seats.iter_mut() {
            if !s.bot && s.token.is_empty() {
                s.token = new_token();
            }
        }
    }

    /// Increases with every change of the game
    pub fn seq(&self) -> u64 {
        self.0.game.notif_seq + self.0.seq_base
    }

    /// Changes at every undo: an AI answer decided before it is not played
    pub fn undos(&self) -> u64 {
        self.0.seq_base
    }

    /// The last answer of player `pid` that was a choice of theirs: not one the client sent for
    /// them (`"auto": true`: a decision without a real choice, answered again at once if the
    /// undo stopped there) nor the engine's undo (games saved before it was refused)
    fn last_real_answer(&self, pid: u32) -> Option<usize> {
        self.0.record.events.iter().rposition(|e| match e {
            Event::Answer { pid: p, body } => *p == pid && body["auto"] != json!(true) && body["type"] != json!("undo"),
            _ => false,
        })
    }

    /// The player of `seat` may take back their answers: they play only against bots or AIs
    /// and made at least one choice
    pub fn can_undo(&self, seat: usize) -> bool {
        let g = &self.0;
        let Some(me) = g.seats.get(seat) else { return false };
        !me.bot && g.record.initial.is_some() && g.seats.iter().enumerate().all(|(i, s)| i == seat || s.bot) && self.last_real_answer(me.id).is_some()
    }

    /// Takes back the last choice of the player of `seat`: the game is rebuilt from its record as
    /// it was just before that answer, and every event after it is dropped (the answers sent
    /// for the player after it too). The engine's randomness is seeded from the game and the
    /// number of acts, so the same answers lead again to the same draws.
    pub fn undo(&mut self, seat: usize) -> Result<(), Refused> {
        if !self.can_undo(seat) {
            let against_bots = self.0.seats.iter().enumerate().all(|(i, s)| i == seat || s.bot);
            let msg = if against_bots { "Nothing to undo" } else { "Undo is only possible against bots" };
            return Err(Refused { status: if against_bots { 409 } else { 403 }, msg: msg.into(), user: true });
        }
        let k = self.last_real_answer(self.0.seats[seat].id).unwrap_or(0);
        let g = &mut self.0;
        let game = g.record.replay(k).map_err(|e| Refused { status: 500, msg: e, user: false })?;
        let seq = g.game.notif_seq + g.seq_base;
        g.seq_base = (seq + 1).saturating_sub(game.notif_seq);
        g.game = game;
        g.record.events.truncate(k);
        Ok(())
    }

    /// Seed of an AI's randomness for its decision at this position: the same position gives the
    /// same answer, after an undo too
    pub fn ai_seed(&self, pid: u32) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (&self.0.id, self.0.game.notif_seq, self.0.record.events.len(), pid).hash(&mut h);
        h.finish()
    }

    pub fn ended(&self) -> bool {
        self.0.game.is_over()
    }

    pub fn run_bots(&mut self) {
        let _ = self.0.play(Event::Bots);
    }

    /// A bot of the engine (the simple bot) has a decision to take
    pub fn bot_pending(&self) -> bool {
        let bots = self.0.game.bots();
        self.0.game.waiting_for().iter().any(|p| bots.contains(&(*p as i64)))
    }

    /// One decision of a bot of the engine, so that the server can pace them (false: none taken)
    pub fn bot_step(&mut self) -> bool {
        self.0.play(Event::BotStep).unwrap_or(false)
    }

    /// The AI seats: (seat index, player id, AI id)
    pub fn ai_seats(&self) -> Vec<(usize, u32, String)> {
        self.0.seats.iter().enumerate().filter_map(|(i, s)| Some((i, s.id, s.ai.clone()?))).collect()
    }

    /// A decision an AI seat has to take: (player id, decision)
    pub fn ai_decision(&self) -> Option<(u32, Value)> {
        let waiting = self.0.game.waiting_for();
        let s = self.0.seats.iter().find(|s| s.ai.is_some() && waiting.contains(&s.id))?;
        Some((s.id, self.0.game.try_decision(s.id).ok()??))
    }

    pub fn game(&self) -> &altered_rules::Game {
        &self.0.game
    }

    /// The answer `a` of an AI seat to its decision `d`; refused, the first candidate the engine
    /// accepts (as headless games do, `altered_ai::play::play_game`). Then the bots play.
    pub fn ai_answer(&mut self, pid: u32, d: &Value, a: &Value) -> Result<(), String> {
        let g = &mut self.0;
        // A refused answer changes nothing (answers are validated before any change)
        let Err(e) = g.play(Event::Ai { pid, body: legal::with_id(d, a) }) else { return Ok(()) };
        eprintln!("game {}: AI answer refused ({}): {}", g.id, d["kind"], e);
        if legal::candidates(d).iter().any(|c| g.play(Event::Ai { pid, body: legal::with_id(d, c) }).is_ok()) {
            return Ok(());
        }
        Err(format!("no answer accepted ({}): {}", d["kind"], e))
    }

    /// The GameView of a seat (None: spectator), serialized
    pub fn view_json(&mut self, seat: ViewSeat) -> Result<Vec<u8>, String> {
        let g = &mut self.0;
        let pid = seat.and_then(|s| g.seats.get(s)).map(|s| s.id);
        let id = g.id.clone();
        let mut v = g.game.view(pid, &id).map_err(|e| e.0)?;
        v["seq"] = json!(g.game.notif_seq + g.seq_base);
        // The engine's undo (BGA's) is not offered, only the server's (`canUndo` of the view)
        if let Some(d) = v["decision"].as_object_mut() {
            d.remove("canUndo");
        }
        let can_undo = seat.is_some_and(|s| self.can_undo(s));
        let g = &mut self.0;
        v["canUndo"] = json!(can_undo);
        // Names, bots and the game id are the server's
        if let Some(ps) = v["players"].as_array_mut() {
            for p in ps {
                if let Some(s) = g.seats.iter().find(|s| json!(s.id) == p["id"]) {
                    p["name"] = json!(s.name);
                    p["bot"] = json!(s.bot);
                }
            }
        }
        Ok(serde_json::to_vec(&v).unwrap_or_default())
    }

    /// A player's answer (protocol v1 Answer)
    pub fn answer(&mut self, seat: usize, body: &Value) -> Result<(), Refused> {
        let g = &mut self.0;
        let Some(pid) = g.seats.get(seat).map(|s| s.id) else {
            return Err(Refused { status: 403, msg: "Unknown player".into(), user: false });
        };
        g.play(Event::Answer { pid, body: body.clone() }).map_err(|e| {
            // V1::answer: HttpError 409 for a stale decision, the engine's exceptions otherwise
            let stale = e == "This decision is no longer current" || e == "Nothing to decide";
            Refused { status: if stale { 409 } else { 400 }, msg: e, user: true }
        })?;
        Ok(())
    }

    /// Full hidden state (debug route)
    pub fn debug_state(&self) -> Value {
        json!({"oracle": self.0.game.oracle_state(), "tables": self.0.game.bga_tables()})
    }

    /// Debug: create cards of the given classes in the given engine locations for `pid`
    pub fn debug_add_cards(&mut self, pid: u32, cards: &[(String, String)]) -> Result<Vec<i64>, String> {
        if !self.0.seats.iter().any(|s| s.id == pid) {
            return Err("Unknown player".into());
        }
        if cards.is_empty() {
            return Err("No cards to add".into());
        }
        let g = &mut self.0.game;
        let mut ids = Vec::with_capacity(cards.len());
        let mut names = Vec::with_capacity(cards.len());
        for (class, location) in cards {
            let loc = debug_location(location)?;
            let id = altered_rules::managers::cards::create_from_class(g, class, loc, pid as i64).map_err(|e| e.0)?;
            ids.push(id);
            names.push(format!("{class} ({loc})"));
        }
        g.notify_to(None, "message", &format!("Debug: added {}", names.join(", ")), json!({}));
        altered_rules::arena::store_notifications(g);
        Ok(ids)
    }
}

/// Positions the debug dialog offers, mapped to the engine's location names
fn debug_location(s: &str) -> Result<&'static str, String> {
    let t: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
    Ok(match t.as_str() {
        "HAND" => "hand",
        "STORMLEFT" | "HEROEXPEDITION" => "stormLeft",
        "STORMRIGHT" | "COMPANIONEXPEDITION" => "stormRight",
        "RESERVE" => "reserve",
        "LANDMARK" => "landmark",
        "MANA" => "mana",
        _ => {
            return Err(format!("Unknown position '{s}' (use HAND, STORMLEFT, STORMRIGHT, RESERVE, LANDMARK, MANA)"));
        }
    })
}

// ---------------------------------------------------------------------------
// Worker threads with large stacks
// ---------------------------------------------------------------------------

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Worker threads for engine calls: a request of the `rules` engine recurses deeply (the golden
/// runner uses 256 MB stacks), more than a tokio worker's stack holds
pub struct EnginePool {
    tx: Mutex<mpsc::Sender<Job>>,
}

impl EnginePool {
    pub fn new(threads: usize, stack_mb: usize) -> EnginePool {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        for i in 0..threads.max(1) {
            let rx = rx.clone();
            std::thread::Builder::new()
                .name(format!("engine-{}", i))
                .stack_size(stack_mb << 20)
                .spawn(move || loop {
                    let job = match rx.lock() {
                        Ok(r) => r.recv(),
                        Err(_) => return,
                    };
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                })
                .expect("engine thread");
        }
        EnginePool { tx: Mutex::new(tx) }
    }

    /// Runs `f` on an engine thread; a panic of the engine becomes an error
    pub async fn run<T: Send + 'static>(&self, f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let job: Job = Box::new(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| {
                p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "engine panic".into())
            });
            let _ = tx.send(r);
        });
        self.tx.lock().map_err(|_| "engine pool poisoned".to_string())?.send(job).map_err(|_| "engine pool stopped".to_string())?;
        rx.await.map_err(|_| "engine thread lost".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Human against the simple bot, the human answering its last legal candidate (keeps cards,
    /// so that Night cleanups come up): once the winner is known, no cleanup is asked any more
    #[test]
    fn no_cleanup_once_the_game_is_decided() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let mut cleanups = 0;
                for seed in 1..=30 {
                    let seats = [
                        NewSeat { name: "Human".into(), bot: false, deck: Some("yz_65_moyo".into()), ai: None, reunion_deck: None },
                        NewSeat { name: "Bot".into(), bot: true, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                    ];
                    let mut g = ServerGame::new(format!("c{}", seed), seats, seed).unwrap();
                    g.run_bots();
                    let mut last = String::new();
                    for _ in 0..5000 {
                        if g.ended() {
                            break;
                        }
                        let Some(d) = g.game().try_decision(2001).unwrap() else {
                            g.run_bots();
                            continue;
                        };
                        last = d["kind"].as_str().unwrap_or("").to_string();
                        cleanups += (last == "cleanup") as usize;
                        let ok = legal::candidates(&d).iter().any(|c| g.answer(0, &legal::with_id(&d, c)).is_ok());
                        assert!(ok, "no answer accepted: {}", d);
                        g.run_bots();
                    }
                    assert!(g.ended(), "game {} did not end", seed);
                    assert_ne!(last, "cleanup", "game {}: a cleanup was asked after the winner was known", seed);
                    if seed <= 3 {
                        let rebuilt = g.0.record.replay(usize::MAX).unwrap();
                        assert_eq!(rebuilt.oracle_state(), g.game().oracle_state(), "game {}: the record does not rebuild the game", seed);
                    }
                }
                assert!(cleanups > 0, "no cleanup came up: the test checks nothing");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Undo against the simple bot: back to before each answer, down to the start of the game;
    /// the same answers then lead to the same game (same draws), and the seq keeps increasing
    #[test]
    fn undo_goes_back_and_the_same_answers_replay_the_same_game() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let seats = [
                    NewSeat { name: "Human".into(), bot: false, deck: Some("yz_65_moyo".into()), ai: None, reunion_deck: None },
                    NewSeat { name: "Bot".into(), bot: true, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                ];
                let mut g = ServerGame::new("u".into(), seats, 11).unwrap();
                g.run_bots();
                assert!(!g.can_undo(0), "nothing to undo before the first answer");
                // Positions before each answer of the human, and the answers
                let mut before = vec![];
                let mut answers = vec![];
                while answers.len() < 40 && !g.ended() {
                    let Some(d) = g.game().try_decision(2001).unwrap() else {
                        g.run_bots();
                        continue;
                    };
                    let state = g.game().oracle_state();
                    let a = legal::candidates(&d).into_iter().map(|c| legal::with_id(&d, &c)).find(|a| g.clone().answer(0, a).is_ok()).expect("an answer");
                    g.answer(0, &a).map_err(|r| r.msg).unwrap();
                    g.run_bots();
                    before.push(state);
                    answers.push(a);
                }
                let end = g.game().oracle_state();
                let mut seq = g.seq();
                // Undo every answer, one at a time
                for k in (0..answers.len()).rev() {
                    g.undo(0).map_err(|r| r.msg).unwrap();
                    assert_eq!(g.game().oracle_state(), before[k], "undo {} does not go back to before answer {}", answers.len() - k, k);
                    assert!(g.seq() > seq, "the seq must increase");
                    seq = g.seq();
                }
                assert!(!g.can_undo(0));
                assert_eq!(g.undo(0).err().map(|r| r.status), Some(409));
                // The same answers again: the same game
                for a in &answers {
                    g.answer(0, a).map_err(|r| r.msg).unwrap();
                    g.run_bots();
                }
                assert_eq!(g.game().oracle_state(), end, "the same answers must give the same game");
                // Saved and loaded, then undone: the same as before
                let mut loaded = ServerGame::load(&g.to_json()).unwrap();
                loaded.undo(0).map_err(|r| r.msg).unwrap();
                assert_eq!(loaded.game().oracle_state(), before[answers.len() - 1]);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// The engine's undo (BGA's) is neither offered by the views nor accepted as an answer; the
    /// server's undo still takes back the last answer
    #[test]
    fn the_engine_undo_is_not_offered_nor_accepted() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let mut engine_offers = 0;
                for seed in 1..=3 {
                    let seats = [
                        NewSeat { name: "Human".into(), bot: false, deck: Some("yz_65_moyo".into()), ai: None, reunion_deck: None },
                        NewSeat { name: "Bot".into(), bot: true, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                    ];
                    let mut g = ServerGame::new("e".into(), seats, seed).unwrap();
                    g.run_bots();
                    let mut answers = 0;
                    while answers < 60 && !g.ended() {
                        let Some(d) = g.game().try_decision(2001).unwrap() else {
                            g.run_bots();
                            continue;
                        };
                        let view: Value = serde_json::from_slice(&g.view_json(Some(0)).unwrap()).unwrap();
                        assert!(view["decision"].get("canUndo").is_none(), "seed {seed}: the view offers the engine's undo ({})", d["id"]);
                        if d["canUndo"] == json!(true) {
                            engine_offers += 1;
                            let state = g.game().oracle_state();
                            let r = g.answer(0, &legal::with_id(&d, &json!({"type": "undo"})));
                            assert_eq!(r.err().map(|r| r.status), Some(400));
                            assert_eq!(g.game().oracle_state(), state, "a refused undo changes nothing");
                        }
                        let a = legal::candidates(&d).into_iter().map(|c| legal::with_id(&d, &c)).find(|a| g.clone().answer(0, a).is_ok()).expect("an answer");
                        g.answer(0, &a).map_err(|r| r.msg).unwrap();
                        g.run_bots();
                        answers += 1;
                    }
                    assert!(view_can_undo(&mut g), "seed {seed}: the server's undo must stay offered");
                }
                assert!(engine_offers > 0, "the engine never offered its undo: the test checks nothing");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    fn view_can_undo(g: &mut ServerGame) -> bool {
        let v: Value = serde_json::from_slice(&g.view_json(Some(0)).unwrap()).unwrap();
        v["canUndo"] == json!(true)
    }

    /// The answers the client sent for the player (`"auto": true`) and the engine's undo of games
    /// saved before it was refused are taken back with the player's choice before them: an undo
    /// does not stop on a decision the client would answer again at once
    #[test]
    fn undo_goes_back_past_the_answers_sent_for_the_player() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let seats = [
                    NewSeat { name: "Human".into(), bot: false, deck: Some("yz_65_moyo".into()), ai: None, reunion_deck: None },
                    NewSeat { name: "Bot".into(), bot: true, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                ];
                let mut g = ServerGame::new("a".into(), seats, 5).unwrap();
                g.run_bots();
                let step = |g: &mut ServerGame, auto: bool| loop {
                    let Some(d) = g.game().try_decision(2001).unwrap() else {
                        g.run_bots();
                        continue;
                    };
                    let mut a = legal::candidates(&d).into_iter().map(|c| legal::with_id(&d, &c)).find(|a| g.clone().answer(0, a).is_ok()).expect("an answer");
                    if auto {
                        a["auto"] = json!(true);
                    }
                    g.answer(0, &a).map_err(|r| r.msg).unwrap();
                    g.run_bots();
                    return;
                };
                step(&mut g, false);
                let before_choice = g.game().oracle_state();
                step(&mut g, false);
                for _ in 0..4 {
                    step(&mut g, true);
                }
                // An engine undo recorded by an older server (it is refused now)
                g.0.record.events.push(Event::Answer { pid: 2001, body: json!({"type": "undo", "decisionId": "x"}) });
                g.undo(0).map_err(|r| r.msg).unwrap();
                assert_eq!(g.game().oracle_state(), before_choice, "back to before the last choice, past the answers sent for the player");
                g.undo(0).map_err(|r| r.msg).unwrap();
                assert!(!g.can_undo(0), "the first choice taken back: nothing left to undo");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// No undo against another human
    #[test]
    fn no_undo_against_a_human() {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let seats = [
                    NewSeat { name: "A".into(), bot: false, deck: Some("yz_65_moyo".into()), ai: None, reunion_deck: None },
                    NewSeat { name: "B".into(), bot: false, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                ];
                let mut g = ServerGame::new("h".into(), seats, 3).unwrap();
                g.run_bots();
                let d = g.game().try_decision(2001).unwrap().expect("a decision");
                let ok = legal::candidates(&d).iter().any(|c| g.answer(0, &legal::with_id(&d, c)).is_ok());
                assert!(ok);
                assert!(!g.can_undo(0));
                assert_eq!(g.undo(0).err().map(|r| r.status), Some(403));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// A full game against an AI seat, both seats answering their first legal candidate: the AI
    /// seat's decisions come from `ai_decision` and go through `ai_answer` (needs the tournament
    /// decklists, server/goldens/tournament_decks.json, which are not in git)
    #[test]
    fn game_with_an_ai_seat() {
        let Ok(text) = std::fs::read_to_string("../server/goldens/tournament_decks.json") else {
            eprintln!("skipped: no tournament decklists");
            return;
        };
        let decks: Value = serde_json::from_str(&text).unwrap();
        let deck = decks["D01"].clone();
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                let seats = [
                    NewSeat { name: "Human".into(), bot: false, deck: Some("ax_1_sierra".into()), ai: None, reunion_deck: None },
                    NewSeat { name: "Player — Sierra & Oddball".into(), bot: true, deck: None, ai: Some(("deck:D01".into(), deck)), reunion_deck: None },
                ];
                let mut g = ServerGame::new("t".into(), seats, 7).unwrap();
                g.run_bots();
                g.set_tokens(|| "tok".into());
                assert_eq!(g.ai_seats(), vec![(1, 2002, "deck:D01".to_string())]);
                let seats = g.seats();
                assert_eq!((seats[1].1.as_str(), seats[1].2, seats[1].3.as_str()), ("Player — Sierra & Oddball", true, ""));
                let mut ai_answers = 0;
                for _ in 0..3000 {
                    if g.ended() {
                        break;
                    }
                    if let Some((pid, d)) = g.ai_decision() {
                        let a = legal::candidates(&d).into_iter().next().unwrap_or(json!({"type": "pass"}));
                        g.ai_answer(pid, &d, &a).unwrap();
                        ai_answers += 1;
                        continue;
                    }
                    let d = g.game().try_decision(2001).unwrap().expect("a decision of the human seat");
                    let ok = legal::candidates(&d).iter().any(|c| g.answer(0, &legal::with_id(&d, c)).is_ok());
                    assert!(ok, "no answer accepted: {}", d);
                    g.run_bots();
                }
                assert!(g.ended(), "the game did not end");
                assert!(ai_answers > 10);
                // The record rebuilds the game
                let r = &g.0.record;
                let rebuilt = r.replay(usize::MAX).unwrap();
                assert_eq!(rebuilt.oracle_state(), g.game().oracle_state());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
