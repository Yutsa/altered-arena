//! Helpers/Log.php: the undo log. PHP logs every DB change and replays them backwards; here each
//! `checkpoint` / `step` / `engine` entry keeps a snapshot of the logged tables taken when the
//! entry is written, which is the state a revert to that entry gives back. Only the entries an
//! undo can reach keep their snapshot (those after the last checkpoint).

use crate::game::{Game, LogEntry, LogKind};
use crate::{PhpError, R};
use std::sync::Arc;

/// `Log::enable()`: the `logging` game state value (global 10)
pub fn enable(g: &mut Game) {
    g.fw_set(10, 1);
}

/// `Log::disable()`
pub fn disable(g: &mut Game) {
    g.fw_set(10, 0);
}

/// `Log::addEntry($entry)`
// PHP: Helpers/Log.php:44
fn add_entry(g: &mut Game, kind: LogKind) -> i64 {
    let id = g.next_log_id;
    g.next_log_id += 1;
    let move_id = g.fw_get(3).unwrap_or(0);
    if kind == LogKind::Checkpoint || kind == LogKind::Engine {
        // Older entries can't be reverted to by an undo of the turn any more
        for e in g.log.iter_mut() {
            e.snapshot = None;
        }
    }
    let snapshot = Some(Arc::new(g.snapshot_tables()));
    g.log.push(LogEntry { id, kind, move_id, snapshot });
    id
}

/// `Log::checkpoint()`
// PHP: Helpers/Log.php:57
pub fn checkpoint(g: &mut Game) -> R<i64> {
    // clearUndoableStepNotifications: UI only
    Ok(add_entry(g, LogKind::Checkpoint))
}

/// `Log::step()`
// PHP: Helpers/Log.php:64
pub fn step(g: &mut Game) -> R<i64> {
    Ok(add_entry(g, LogKind::Step))
}

/// `Log::startEngine()`
// PHP: Helpers/Log.php:70
pub fn start_engine(g: &mut Game) -> R<i64> {
    checkpoint(g)?;
    Ok(add_entry(g, LogKind::Engine))
}

/// `Log::getLastCheckpoint($includeEngineStarts)`: 1 when there is none
// PHP: Helpers/Log.php:77
pub fn get_last_checkpoint(g: &Game, include_engine: bool) -> i64 {
    g.log
        .iter()
        .rev()
        .find(|e| e.kind == LogKind::Checkpoint || (include_engine && e.kind == LogKind::Engine))
        .map(|e| e.id)
        .unwrap_or(1)
}

/// `Log::getUndoableSteps()`: ids of the steps after the last checkpoint, latest first
// PHP: Helpers/Log.php:95
pub fn get_undoable_steps(g: &Game) -> Vec<i64> {
    let cp = get_last_checkpoint(g, false);
    g.log.iter().rev().filter(|e| e.kind == LogKind::Step && e.id > cp).map(|e| e.id).collect()
}

/// `Log::undoTurn()`
// PHP: Helpers/Log.php:114
pub fn undo_turn(g: &mut Game) -> R<()> {
    let cp = get_last_checkpoint(g, true);
    revert_to_entry(g, cp, true)
}

/// `Log::undoToStep($stepId)`: back to the state just before the step
// PHP: Helpers/Log.php:123
pub fn undo_to_step(g: &mut Game, step_id: i64) -> R<()> {
    if !g.log.iter().any(|e| e.id == step_id) {
        return Err(PhpError::new("Cant undo here"));
    }
    revert_to_entry(g, step_id, false)
}

/// `Log::revertTo($id)`: the tables as they were when entry `id` was written; the entries after
/// it are deleted (and the entry itself when `keep` is false: revertTo($stepId - 1))
fn revert_to_entry(g: &mut Game, id: i64, keep: bool) -> R<()> {
    let Some(e) = g.log.iter().find(|e| e.id == id) else {
        return Err(PhpError::new("Undo: no log entry to revert to (log started after this point)"));
    };
    let Some(snap) = e.snapshot.clone() else {
        return Err(PhpError::new("Undo: log entry too old to revert to"));
    };
    g.restore_tables(&snap)?;
    g.log.retain(|e| if keep { e.id <= id } else { e.id < id });
    Ok(())
}
