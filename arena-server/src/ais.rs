//! The AIs a bot seat can play: the simple bot (the engine's Bot.php), the imitation AIs of the
//! players of BGA tournament 617607 (`altered-imitate`, one per player, on that player's deck)
//! and the deck AIs of the tournament decks (`altered-ai`). Their files are read on demand, so a
//! refit is picked up without restarting the server.
//!
//! Environment: IMITATION_DIR (private/tournament-617607/imitation: `shared.json` and
//! `players/*.json`), AI_PARAMS_DIR (ai/params: `DXX.json`), TOURNAMENT_DECKS
//! (server/goldens/tournament_decks.json: the decklists). Without the decklists, only the simple
//! bot is offered.

use altered_ai::play::Agent;
use altered_ai::search::{Ai, Params};
use altered_imitate::agent::{Imitator, PlayerFile, Shared};
use serde_json::{json, Value};
use std::path::PathBuf;

/// The simple bot: the engine plays it (`Game::run_bots`)
pub const SIMPLE: &str = "simple";

pub struct AiDirs {
    imitation: PathBuf,
    params: PathBuf,
    decks: PathBuf,
}

impl AiDirs {
    pub fn from_env() -> AiDirs {
        let env = |k: &str, d: &str| PathBuf::from(std::env::var(k).unwrap_or_else(|_| d.to_string()));
        AiDirs {
            imitation: env("IMITATION_DIR", "private/tournament-617607/imitation"),
            params: env("AI_PARAMS_DIR", "ai/params"),
            decks: env("TOURNAMENT_DECKS", "server/goldens/tournament_decks.json"),
        }
    }

    fn decks(&self) -> Option<Value> {
        serde_json::from_slice(&std::fs::read(&self.decks).ok()?).ok()
    }

    /// The player files, with their file name (without `.json`), sorted by label
    fn players(&self) -> Vec<(String, PlayerFile)> {
        let mut out: Vec<(String, PlayerFile)> = std::fs::read_dir(self.imitation.join("players"))
            .map(|d| {
                d.flatten()
                    .filter_map(|f| {
                        let stem = f.path().file_stem()?.to_str()?.to_string();
                        let pf: PlayerFile = serde_json::from_slice(&std::fs::read(f.path()).ok()?).ok()?;
                        Some((stem, pf))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|(_, p)| p.label.to_lowercase());
        out
    }

    /// The player file `players/<stem>.json` (the name must be one the directory lists)
    fn player(&self, stem: &str) -> Option<PlayerFile> {
        let f = std::fs::read_dir(self.imitation.join("players")).ok()?.flatten().find(|f| f.path().file_stem().and_then(|s| s.to_str()) == Some(stem))?;
        serde_json::from_slice(&std::fs::read(f.path()).ok()?).ok()
    }

    fn params(&self, deck: &str) -> Option<Params> {
        serde_json::from_slice(&std::fs::read(self.params.join(format!("{}.json", deck))).ok()?).ok()
    }

    /// Protocol AiSummary list: the simple bot, the imitation AIs, the deck AIs
    pub fn list(&self) -> Vec<Value> {
        let mut out = vec![json!({"id": SIMPLE, "label": "Simple bot", "kind": "simple"})];
        let Some(decks) = self.decks() else { return out };
        let shared = self.imitation.join("shared.json").is_file();
        for (stem, p) in self.players() {
            if shared && decks.get(&p.deck).is_some() {
                out.push(json!({"id": format!("imitation:{}", stem), "label": p.label, "kind": "imitation", "deck": p.deck, "hero": p.hero}));
            }
        }
        for (id, deck) in decks.as_object().into_iter().flatten() {
            if self.params.join(format!("{}.json", id)).is_file() {
                let hero = hero_name(deck);
                out.push(json!({"id": format!("deck:{}", id), "label": format!("Deck AI {} — {}", id, hero), "kind": "deck", "deck": id, "hero": hero}));
            }
        }
        out
    }

    /// The display name and the decklist of an AI (None: unknown AI, or the simple bot)
    pub fn describe(&self, id: &str) -> Option<(String, Value)> {
        let decks = self.decks()?;
        if let Some(stem) = id.strip_prefix("imitation:") {
            let p = self.player(stem)?;
            return Some((p.label, decks.get(&p.deck)?.clone()));
        }
        let deck = id.strip_prefix("deck:")?;
        let list = decks.get(deck)?;
        self.params(deck)?;
        Some((format!("Deck AI {} — {}", deck, hero_name(list)), list.clone()))
    }

    /// A new agent for an AI of `describe`
    pub fn agent(&self, id: &str) -> Result<Box<dyn Agent>, String> {
        if let Some(stem) = id.strip_prefix("imitation:") {
            let p = self.player(stem).ok_or_else(|| format!("no player file {}", stem))?;
            let shared: Shared = serde_json::from_slice(&std::fs::read(self.imitation.join("shared.json")).map_err(|e| format!("shared.json: {}", e))?)
                .map_err(|e| format!("shared.json: {}", e))?;
            if p.shared != shared.version {
                eprintln!("AI {}: offsets for shared weights {}, shared.json is {}", p.label, p.shared, shared.version);
            }
            let params = self.params(&p.deck);
            return Ok(Box::new(Imitator::new(&p.label, &shared, Some(&p), params)));
        }
        if let Some(deck) = id.strip_prefix("deck:") {
            let params = self.params(deck).ok_or_else(|| format!("no AI parameters for {}", deck))?;
            return Ok(Box::new(Ai::new(id, params)));
        }
        Err(format!("unknown AI {}", id))
    }
}

/// The hero's name in a decklist (`cards` holds the hero, `hero` is its uid)
fn hero_name(deck: &Value) -> String {
    deck["cards"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(_, c)| &c["card"]["properties"])
        .find(|p| p["uid"] == deck["hero"])
        .and_then(|p| p["name"].as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_files_only_the_simple_bot() {
        let dirs = AiDirs { imitation: "/nonexistent".into(), params: "/nonexistent".into(), decks: "/nonexistent/decks.json".into() };
        let list = dirs.list();
        assert_eq!(list, vec![json!({"id": SIMPLE, "label": "Simple bot", "kind": "simple"})]);
        assert!(dirs.describe("deck:D01").is_none());
        assert!(dirs.describe("imitation:../x").is_none());
        assert!(dirs.agent("other").is_err());
    }

    #[test]
    fn hero_of_a_decklist() {
        let deck = json!({"hero": "H1", "cards": {"0": {"card": {"properties": {"uid": "C1", "name": "A card"}}}, "1": {"card": {"properties": {"uid": "H1", "name": "Sierra & Oddball"}}}}});
        assert_eq!(hero_name(&deck), "Sierra & Oddball");
    }
}
