//! The two rules engines behind the same routes: `legacy` (the first Rust engine, `engine/`)
//! and `rules` (the translation of the PHP engine, `rules/`), chosen by ARENA_ENGINE.
//!
//! The `rules` engine runs requests as PHP does: whole chains of states recursively (jumpToState
//! is re-entrant), so its calls run on worker threads with large stacks (`EnginePool`).

use altered_engine::model::Seat;
use altered_engine::{Answer, PlayerConfig};
use altered_rules::arena::NewPlayer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineKind {
    Legacy,
    Rules,
}

impl EngineKind {
    pub fn from_env() -> EngineKind {
        match std::env::var("ARENA_ENGINE").as_deref() {
            Ok("rules") => EngineKind::Rules,
            Ok("legacy") | Err(_) => EngineKind::Legacy,
            Ok(other) => panic!("ARENA_ENGINE must be rules or legacy, not {}", other),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EngineKind::Legacy => "legacy",
            EngineKind::Rules => "rules",
        }
    }
}

/// A seat of a game of the `rules` engine (the engine knows the players by id; the server keeps
/// names, bots and tokens)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SeatInfo {
    pub id: u32,
    pub name: String,
    pub bot: bool,
    #[serde(default)]
    pub token: String,
}

/// A game of the `rules` engine as the server keeps it (and saves it)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RulesGame {
    pub engine: String,
    pub id: String,
    pub seats: Vec<SeatInfo>,
    pub game: altered_rules::Game,
}

#[derive(Clone)]
pub enum AnyGame {
    Legacy(altered_engine::Game),
    Rules(Box<RulesGame>),
}

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

impl AnyGame {
    /// A new game: players 2001 and 2002, decks chosen in the lobby (else in game)
    pub fn new(kind: EngineKind, id: String, players: [(String, bool, Option<String>); 2], seed: u64) -> Result<AnyGame, String> {
        match kind {
            EngineKind::Legacy => {
                let cfg = |i: usize| {
                    let (name, bot, deck) = players[i].clone();
                    PlayerConfig { id: 2001 + i as u32, name, bot, deck }
                };
                Ok(AnyGame::Legacy(altered_engine::Game::new(id, [cfg(0), cfg(1)], seed)))
            }
            EngineKind::Rules => {
                let new: Vec<NewPlayer> = players.iter().map(|(name, bot, deck)| NewPlayer { name: name.clone(), bot: *bot, deck: deck.clone() }).collect();
                // The game id of the arena seeds the randomness of each act (Arena::seedRng)
                let game_id = (seed & 0x7FFF_FFFF) as i64;
                let game = altered_rules::Game::create(game_id, &new, &dice_from_seed(seed)).map_err(|e| e.0)?;
                let seats = players
                    .iter()
                    .enumerate()
                    .map(|(i, (_, bot, _))| SeatInfo { id: 2001 + i as u32, name: game.players.get(&(2001 + i as i64)).map(|p| p.name.clone()).unwrap_or_default(), bot: *bot, token: String::new() })
                    .collect();
                Ok(AnyGame::Rules(Box::new(RulesGame { engine: "rules".into(), id, seats, game })))
            }
        }
    }

    pub fn load(kind: EngineKind, bytes: &[u8]) -> Option<AnyGame> {
        match kind {
            EngineKind::Legacy => serde_json::from_slice(bytes).ok().map(AnyGame::Legacy),
            EngineKind::Rules => {
                let g: RulesGame = serde_json::from_slice(bytes).ok()?;
                (g.engine == "rules").then(|| AnyGame::Rules(Box::new(g)))
            }
        }
    }

    pub fn to_json(&self) -> Vec<u8> {
        match self {
            AnyGame::Legacy(g) => serde_json::to_vec(g).unwrap_or_default(),
            AnyGame::Rules(g) => serde_json::to_vec(g).unwrap_or_default(),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            AnyGame::Legacy(g) => &g.id,
            AnyGame::Rules(g) => &g.id,
        }
    }

    /// (player id, name, bot, token) per seat
    pub fn seats(&self) -> Vec<(u32, String, bool, String)> {
        match self {
            AnyGame::Legacy(g) => g.players.iter().map(|p| (p.id, p.name.clone(), p.bot, p.token.clone())).collect(),
            AnyGame::Rules(g) => g.seats.iter().map(|s| (s.id, s.name.clone(), s.bot, s.token.clone())).collect(),
        }
    }

    /// Gives each human seat without a token a new one
    pub fn set_tokens(&mut self, mut new_token: impl FnMut() -> String) {
        match self {
            AnyGame::Legacy(g) => {
                for p in g.players.iter_mut() {
                    if !p.bot && p.token.is_empty() {
                        p.token = new_token();
                    }
                }
            }
            AnyGame::Rules(g) => {
                for s in g.seats.iter_mut() {
                    if !s.bot && s.token.is_empty() {
                        s.token = new_token();
                    }
                }
            }
        }
    }

    /// Increases with every change of the game
    pub fn seq(&self) -> u64 {
        match self {
            AnyGame::Legacy(g) => g.seq,
            AnyGame::Rules(g) => g.game.notif_seq,
        }
    }

    pub fn ended(&self) -> bool {
        match self {
            AnyGame::Legacy(g) => g.winner.is_some(),
            AnyGame::Rules(g) => g.game.is_over(),
        }
    }

    pub fn run_bots(&mut self) {
        match self {
            AnyGame::Legacy(g) => {
                g.run_bots();
            }
            AnyGame::Rules(g) => {
                if let Err(e) = g.game.run_bots() {
                    eprintln!("game {}: bots: {}", g.id, e);
                }
                // The game now waits for a player
                g.game.compact();
            }
        }
    }

    /// The GameView of a seat (None: spectator), serialized
    pub fn view_json(&mut self, seat: ViewSeat) -> Result<Vec<u8>, String> {
        match self {
            AnyGame::Legacy(g) => Ok(g.view_json(seat)),
            AnyGame::Rules(g) => {
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
        }
    }

    /// A player's answer (protocol v1 Answer)
    pub fn answer(&mut self, seat: usize, body: &Value) -> Result<(), Refused> {
        match self {
            AnyGame::Legacy(g) => {
                let decision_id = body.get("decisionId").and_then(Value::as_str).unwrap_or("");
                let is_undo = body.get("type").and_then(Value::as_str) == Some("undo");
                if !is_undo && g.decision_id(seat as Seat).as_deref() != Some(decision_id) {
                    return Err(Refused { status: 409, msg: "This decision is no longer current".into(), user: true });
                }
                let answer: Answer = serde_json::from_value(body.clone()).map_err(|e| Refused { status: 400, msg: format!("Bad answer: {}", e), user: true })?;
                g.answer(seat as Seat, answer).map_err(|e| Refused { status: 400, msg: e.0, user: true })
            }
            AnyGame::Rules(g) => {
                let Some(pid) = g.seats.get(seat).map(|s| s.id) else {
                    return Err(Refused { status: 403, msg: "Unknown player".into(), user: false });
                };
                g.game.answer(pid, body).map_err(|e| {
                    // V1::answer: HttpError 409 for a stale decision, the engine's exceptions otherwise
                    let stale = e.0 == "This decision is no longer current" || e.0 == "Nothing to decide";
                    Refused { status: if stale { 409 } else { 400 }, msg: e.0, user: true }
                })
            }
        }
    }

    /// Full hidden state (debug routes)
    pub fn debug_state(&self) -> Value {
        match self {
            AnyGame::Legacy(g) => g.snapshot(),
            AnyGame::Rules(g) => json!({"oracle": g.game.oracle_state(), "tables": g.game.bga_tables()}),
        }
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
