//! The rules engine (`rules/`, the translation of the PHP engine) as the server keeps a game.
//!
//! The engine runs requests as PHP does: whole chains of states recursively (jumpToState is
//! re-entrant), so its calls run on worker threads with large stacks (`EnginePool`).

use altered_ai::legal;
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
}

/// A game of the `rules` engine as the server keeps it (and saves it)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RulesGame {
    pub engine: String,
    pub id: String,
    pub seats: Vec<SeatInfo>,
    pub game: altered_rules::Game,
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
                Some((_, deck)) => NewPlayer { name: p.name.clone(), bot: false, deck: None, api_deck: Some(deck.clone()) },
                None => NewPlayer { name: p.name.clone(), bot: p.bot, deck: p.deck.clone(), api_deck: None },
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
        Ok(ServerGame(Box::new(RulesGame { engine: "rules".into(), id, seats, game })))
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
        self.0.game.notif_seq
    }

    pub fn ended(&self) -> bool {
        self.0.game.is_over()
    }

    pub fn run_bots(&mut self) {
        let g = &mut self.0;
        if let Err(e) = g.game.run_bots() {
            eprintln!("game {}: bots: {}", g.id, e);
        }
        // The game now waits for a player
        g.game.compact();
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
        if let Err(e) = g.game.answer(pid, &legal::with_id(d, a)) {
            eprintln!("game {}: AI answer refused ({}): {}", g.id, d["kind"], e.0);
            if !legal::candidates(d).iter().any(|c| g.game.answer(pid, &legal::with_id(d, c)).is_ok()) {
                return Err(format!("no answer accepted ({}): {}", d["kind"], e.0));
            }
        }
        self.run_bots();
        Ok(())
    }

    /// The GameView of a seat (None: spectator), serialized
    pub fn view_json(&mut self, seat: ViewSeat) -> Result<Vec<u8>, String> {
        let g = &mut self.0;
        let pid = seat.and_then(|s| g.seats.get(s)).map(|s| s.id);
        let id = g.id.clone();
        let mut v = g.game.view(pid, &id).map_err(|e| e.0)?;
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
        g.game.answer(pid, body).map_err(|e| {
            // V1::answer: HttpError 409 for a stale decision, the engine's exceptions otherwise
            let stale = e.0 == "This decision is no longer current" || e.0 == "Nothing to decide";
            Refused { status: if stale { 409 } else { 400 }, msg: e.0, user: true }
        })
    }

    /// Full hidden state (debug route)
    pub fn debug_state(&self) -> Value {
        json!({"oracle": self.0.game.oracle_state(), "tables": self.0.game.bga_tables()})
    }
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
                    NewSeat { name: "Human".into(), bot: false, deck: Some("ax_1_sierra".into()), ai: None },
                    NewSeat { name: "Player — Sierra & Oddball".into(), bot: true, deck: None, ai: Some(("deck:D01".into(), deck)) },
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
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
