//! The tournament's games, replayed in the engine from their golden files (explore.php replay:
//! the PHP oracle rebuilt each request of the BGA replay as a protocol v1 answer). Gives, at each
//! decision of each player, the position and the answer the real player gave.

use altered_rules::game::ShuffleHook;
use altered_rules::Game;
use serde_json::Value;

pub struct Step {
    /// The game before the answer
    pub game: Game,
    pub player: u32,
    /// The decision (with its id)
    pub decision: Value,
    /// The real player's answer (with the decision id)
    pub answer: Value,
    pub next_ids: Value,
}

fn read(path: &str) -> Option<Value> {
    let b = std::fs::read(path).ok()?;
    let mut de = serde_json::Deserializer::from_slice(&b);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).ok()
}

/// Ids BGA consumed in requests an undo erased (as the golden runner sets them)
pub fn set_next_ids(g: &mut Game, next_ids: &Value) {
    if let Some(id) = next_ids["cards"].as_i64() {
        g.next_card_id = id;
    }
    if let Some(id) = next_ids["meeples"].as_i64() {
        g.next_meeple_id = id;
    }
}

/// Every step of a replay golden file, in order (stops at the first answer the engine refuses)
pub fn steps(path: &str) -> Result<Vec<Step>, String> {
    let data = read(path).ok_or("unreadable")?;
    let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
    let mut g = Game::from_bga_tables(&data["db"], &dice).map_err(|e| e.to_string())?;
    if let Some(orders) = data["injectShuffles"].as_array() {
        g.shuffle_hook = ShuffleHook::Injected(orders.iter().map(|o| o.as_array().into_iter().flatten().filter_map(Value::as_i64).collect()).collect());
    }
    let mut out = vec![];
    let mut node = &data["tree"];
    loop {
        let Some(p) = node["player"].as_u64().map(|p| p as u32) else { break };
        let Some(next) = node["next"].as_array().and_then(|n| n.first()) else { break };
        let Some(decision) = g.decision(p) else { break };
        let mut answer = next["answer"].clone();
        answer["decisionId"] = decision["id"].clone();
        let step = Step { game: g.clone(), player: p, decision, answer: answer.clone(), next_ids: node["nextIds"].clone() };
        set_next_ids(&mut g, &node["nextIds"]);
        if g.answer(p, &answer).is_err() {
            break;
        }
        out.push(step);
        node = &next["node"];
    }
    Ok(out)
}
