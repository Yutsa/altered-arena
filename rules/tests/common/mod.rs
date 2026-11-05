//! The recorded positions of the golden files. They are not in git: `GOLDENS_DIR` (or
//! `ALTERED_GOLDENS`) points to them, else `server/goldens` of this checkout. CI downloads them
//! (goldens/ARCHIVE).

// Each test file uses part of these helpers
#![allow(dead_code)]

use altered_rules::Game;
use serde_json::Value;

/// The dice the recorded positions are loaded with
pub const DICE: [i64; 6] = [3, 1, 6, 4, 2, 5];

pub fn goldens_dir() -> String {
    std::env::var("GOLDENS_DIR")
        .or_else(|_| std::env::var("ALTERED_GOLDENS"))
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../server/goldens").to_string())
}

/// A golden file (path relative to the golden directory). Without the goldens the test is
/// skipped, except under CI (`CI` set), where a missing file fails the test.
pub fn golden(rel: &str) -> Option<Value> {
    let path = format!("{}/{}", goldens_dir(), rel);
    match std::fs::read(&path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {}", path, e))),
        Err(e) if std::env::var_os("CI").is_some() => panic!("{}: {} (CI runs with the goldens)", path, e),
        Err(_) => {
            eprintln!("skipped: {} is missing", path);
            None
        }
    }
}

/// The rich board of a card: the game and the card's id
pub fn rich(class: &str) -> Option<(Game, i64)> {
    let data = golden(&format!("rich/{}.json", class))?;
    let g = Game::from_bga_tables(&data["db"], &DICE).expect("load");
    Some((g, data["cardId"].as_i64().expect("cardId")))
}

/// The rich board of AX_Common_TikTok (player 2001 to play)
pub fn tiktok() -> Option<Game> {
    rich("AX_Common_TikTok").map(|(g, _)| g)
}
