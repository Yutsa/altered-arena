//! Faithful translation of the AlteredBGA PHP rules engine (see docs/RUST_PORT.md)

pub mod actions;
pub mod arena;
pub mod bot;
pub mod core;
pub mod db;
pub mod framework;
pub mod game;
pub mod game_methods;
pub mod helpers;
pub mod load;
pub mod managers;
pub mod models;
pub mod php;
pub mod php_rand;
pub mod states;
pub mod v1;

pub use game::Game;
use serde_json::Value;

/// A PHP exception: the request that raised it is rolled back
#[derive(Debug, Clone)]
pub struct PhpError(pub String);

impl PhpError {
    pub fn new(msg: impl Into<String>) -> PhpError {
        PhpError(msg.into())
    }

    /// A part of the PHP engine that is not translated yet
    pub fn nt(what: &str) -> PhpError {
        PhpError(format!("not translated: {}", what))
    }

    pub fn is_not_translated(&self) -> bool {
        self.0.starts_with("not translated: ")
    }
}

impl std::fmt::Display for PhpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PhpError {}

pub type R<T> = Result<T, PhpError>;

impl Game {
    /// A game from the original engine's tables (rows of cards, meeples, global_variables, player,
    /// user_preferences, as MariaDB returns them) and the per-game dice list
    pub fn from_bga_tables(tables: &Value, dice: &[i64]) -> Result<Game, PhpError> {
        load::from_bga_tables(tables, dice)
    }

    /// Players the game waits for (protocol v1 `waitingFor`)
    pub fn waiting_for(&self) -> Vec<u32> {
        v1::waiting_for(self).into_iter().map(|p| p as u32).collect()
    }

    /// The protocol v1 decision of a player, with its id
    pub fn decision(&self, player: u32) -> Option<Value> {
        self.try_decision(player).ok().flatten()
    }

    /// The protocol v1 decision of a player, or the error the view raised
    pub fn try_decision(&self, player: u32) -> Result<Option<Value>, PhpError> {
        let mut g = self.clone();
        let raw = v1::view_state(&mut g, player as i64)?;
        v1::decision(&mut g, &raw, player as i64)
    }

    /// A protocol v1 answer of a player
    pub fn answer(&mut self, player: u32, answer: &Value) -> Result<(), PhpError> {
        v1::answer(self, player as i64, answer)
    }

    /// The state in the format of Arena::oracleState
    pub fn oracle_state(&self) -> Value {
        let mut g = self.clone();
        v1::oracle_state(&mut g).unwrap_or(Value::Null)
    }

    /// A new game as the arena creates it (`V1::createGame`): players 2001 and 2002, the
    /// randomness seeded from `game_id` (Arena::seedRng), dice from `dice` (in order, cycling;
    /// empty: the BGA studio rule, every die is a 5), the decks of human players selected right
    /// away, then the bots play their decisions
    pub fn create(game_id: i64, players: &[arena::NewPlayer], dice: &[i64]) -> Result<Game, PhpError> {
        arena::create_v1_game(game_id, players, dice)
    }

    /// The protocol v1 GameView of a player (None: a spectator). As in PHP, a view may create the
    /// missing user preferences and global rows (with their default values).
    pub fn view(&mut self, player: Option<u32>, game_id: &str) -> Result<Value, PhpError> {
        v1::build(self, player.map(|p| p as PId), game_id)
    }

    /// Bot players take every decision that is theirs (Arena::runBots)
    pub fn run_bots(&mut self) -> Result<(), PhpError> {
        arena::run_bots(self)
    }

    /// The game is over (state `gameEnd`)
    pub fn is_over(&self) -> bool {
        framework::state(self).is_some_and(|s| s.name == "gameEnd")
    }

    /// The players the bot plays
    pub fn bots(&self) -> &[PId] {
        &self.bots
    }

    /// The tables in the format of `from_bga_tables` (explore.php `dumpTables`)
    pub fn bga_tables(&self) -> Value {
        load::to_bga_tables(self)
    }
}

use game::PId;
