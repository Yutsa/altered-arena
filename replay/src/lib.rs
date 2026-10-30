//! Replays of Board Game Arena games on the translated engine. A replay holds what the players
//! saw (notifications), not what they did: each request is rebuilt by trying the answers the
//! position allows and keeping the one whose effects match what BGA recorded.

pub mod bga;
pub mod setup;
pub mod sig;

use altered_rules::{v1, Game, PhpError};
use serde_json::Value;

/// `Game::answer` (V1::answer), returning the notifications of the request it makes. The answer
/// needs no decision id. On error the game is left as it was.
pub fn play(g: &mut Game, pid: u32, a: &Value) -> Result<Vec<Value>, PhpError> {
    let pid = pid as i64;
    let backup = g.clone();
    let res = (|| {
        if a["type"] == "act" {
            // A request the protocol has no answer for (BGA's client sends it, e.g. taking back a
            // first-day Mana choice): the action itself, as Arena::request runs it
            g.seed_rng();
            g.begin_request(Some(pid))?;
            let args: Vec<Value> = a["args"].as_array().cloned().unwrap_or_default();
            v1::call_action(g, a["action"].as_str().unwrap_or(""), &args)?;
            altered_rules::arena::store_notifications(g);
            return Ok(std::mem::take(&mut g.notifications));
        }
        let raw = v1::view_state(g, pid)?;
        if v1::decision(g, &raw, pid)?.is_none() {
            return Err(PhpError::new("Nothing to decide"));
        }
        let (action, args) = v1::to_action(&raw, a)?;
        v1::act(g, pid, &action, &args)?;
        let notifs = std::mem::take(&mut g.notifications);
        // The view after the action (its errors make the answer fail)
        let raw = v1::view_state(g, pid)?;
        v1::decision(g, &raw, pid)?;
        Ok(notifs)
    })();
    if res.is_err() {
        *g = backup;
    }
    res
}
pub mod runner;
pub mod native;
