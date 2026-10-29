//! Faithful translation of the AlteredBGA PHP rules engine (see docs/RUST_PORT.md)

use serde_json::Value;

/// A PHP exception: the request that raised it is rolled back
#[derive(Debug, Clone)]
pub struct PhpError(pub String);

impl std::fmt::Display for PhpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Game {}

impl Game {
    /// A game from the original engine's tables (rows of cards, meeples, global_variables, player,
    /// user_preferences, as MariaDB returns them) and the per-game dice list
    pub fn from_bga_tables(_tables: &Value, _dice: &[i64]) -> Result<Game, PhpError> {
        Err(PhpError("not translated yet".into()))
    }

    /// Players the game waits for (protocol v1 `waitingFor`)
    pub fn waiting_for(&self) -> Vec<u32> {
        vec![]
    }

    /// The protocol v1 decision of a player, with its id
    pub fn decision(&self, _player: u32) -> Option<Value> {
        None
    }

    /// A protocol v1 answer of a player
    pub fn answer(&mut self, _player: u32, _answer: &Value) -> Result<(), PhpError> {
        Err(PhpError("not translated yet".into()))
    }

    /// The state in the format of Arena::oracleState
    pub fn oracle_state(&self) -> Value {
        Value::Null
    }
}
