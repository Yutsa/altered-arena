//! The rules engine (`rules/`, the translation of the PHP engine) as the server keeps a game.
//!
//! The engine runs requests as PHP does: whole chains of states recursively (jumpToState is
//! re-entrant), so its calls run on worker threads with large stacks (`EnginePool`).

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
    pub fn new(id: String, players: [(String, bool, Option<String>); 2], seed: u64) -> Result<ServerGame, String> {
        let new: Vec<NewPlayer> = players.iter().map(|(name, bot, deck)| NewPlayer { name: name.clone(), bot: *bot, deck: deck.clone() }).collect();
        // The game id of the arena seeds the randomness of each act (Arena::seedRng)
        let game_id = (seed & 0x7FFF_FFFF) as i64;
        let game = altered_rules::Game::create(game_id, &new, &dice_from_seed(seed)).map_err(|e| e.0)?;
        let seats = players
            .iter()
            .enumerate()
            .map(|(i, (_, bot, _))| SeatInfo { id: 2001 + i as u32, name: game.players.get(&(2001 + i as i64)).map(|p| p.name.clone()).unwrap_or_default(), bot: *bot, token: String::new() })
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
