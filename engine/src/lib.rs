//! Altered rules engine.
//!
//! Deterministic and free of I/O, so the same code runs in the game server and in a browser
//! (WebAssembly). Card data comes from the original engine (`data/`); effects are interpreted
//! from their flow trees (`flow`, `actions`, `conditions`).
//!
//! ```no_run
//! use altered_engine::{Game, PlayerConfig, Answer};
//! let mut g = Game::new("1", [
//!     PlayerConfig { id: 1, name: "Ana".into(), bot: false, deck: Some("ly_1_nevenka".into()) },
//!     PlayerConfig { id: 2, name: "Bot".into(), bot: true, deck: None },
//! ], 42);
//! g.run_bots();
//! let view = g.view(Some(0)); // protocol v1 GameView as JSON
//! ```

pub mod actions;
pub mod answer;
pub mod bot;
pub mod catalog;
pub mod conditions;
pub mod flow;
pub mod game;
pub mod model;
pub mod play;
pub mod rng;
pub mod snapshot;
pub mod special;
pub mod view;

pub use answer::Answer;
pub use game::{Game, PlayerConfig};
