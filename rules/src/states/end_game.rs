//! States/EndGameTrait.php

use crate::framework;
use crate::game::Game;
use crate::R;

/// `stPreEndOfGame($concede)`: the report to the publisher's API is BGA-only
// PHP: States/EndGameTrait.php:17
pub fn st_pre_end_of_game(g: &mut Game, concede: bool) -> R<()> {
    g.notify("message", serde_json::json!({"log": "The game has ended."}));
    if !concede {
        framework::next_state(g, "")?;
    }
    Ok(())
}
