//! Imitation AIs: for each real player, an AI that makes the choices that player would make.
//! A decision dataset from native replays (`dataset`), generic candidate features (`features`),
//! a shared + per-player softmax model (`model`), the cross-validated score (`cv`) and the agent
//! that plays full games (`agent`). See `docs/ai/IMITATION.md`.

pub mod agent;
pub mod cv;
pub mod dataset;
pub mod features;
pub mod model;
