//! Evaluation of a position from one player's point of view, on their GameView.
//!
//! Altered is a race: at Dusk, each Hero expedition is compared with the opposing Hero
//! expedition and each Companion expedition with the opposing Companion one, biome by biome
//! (TurnTrait::stAdvanceDusk, Players::computeStorm): an expedition moves forward when it is
//! strictly stronger in a biome of the region it stands in. A player wins when their two
//! expeditions meet. The evaluation counts the progress made, the Dusk outlook of each
//! expedition and the resources (hand, Reserve, Mana, cards on the board).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Weights of the evaluation (per deck, `ai/params/DXX.json`)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Weights {
    /// One region of progress (my expeditions' distance covered minus the opponent's)
    pub progress: f64,
    /// An expedition that would move at Dusk as things stand (mine minus the opponent's)
    pub dusk_win: f64,
    /// Margin in the best biome of an expedition's region (capped at +-cap)
    pub margin: f64,
    pub margin_cap: f64,
    /// Total statistics of the characters in my expeditions, minus the opponent's
    pub board: f64,
    pub hand: f64,
    pub reserve: f64,
    /// Each Mana Orb up to `mana_soft`, then `mana_late` each
    pub mana: f64,
    pub mana_soft: f64,
    pub mana_late: f64,
    /// Ready (unexhausted) Mana Orbs during the Afternoon
    pub mana_ready: f64,
    pub landmark: f64,
    /// An expedition with no character (the opponent's counts against them)
    pub empty_expedition: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            progress: 12.0,
            dusk_win: 5.0,
            margin: 0.6,
            margin_cap: 3.0,
            board: 0.25,
            hand: 0.6,
            reserve: 0.5,
            mana: 1.6,
            mana_soft: 7.0,
            mana_late: 0.3,
            mana_ready: 0.15,
            landmark: 0.6,
            empty_expedition: 1.0,
        }
    }
}

pub const WIN: f64 = 1000.0;
const BIOMES: [&str; 3] = ["forest", "mountain", "ocean"];

fn i(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// Biomes of a region (all three while its Tumult card is face down)
fn region_biomes<'a>(view: &'a Value, pos: i64) -> Vec<&'a str> {
    let r = view["regions"].as_array().and_then(|rs| rs.iter().find(|r| r["index"].as_i64() == Some(pos)));
    match r.and_then(|r| r["biomes"].as_array()) {
        Some(b) => b.iter().filter_map(|x| x.as_str()).collect(),
        None => BIOMES.to_vec(),
    }
}

/// The value of `view` for its viewer (`view.you`)
pub fn evaluate(view: &Value, w: &Weights) -> f64 {
    let me = view["you"].as_i64().unwrap_or(0);
    if let Some(winner) = view["winner"].as_i64() {
        return if winner == me { WIN } else { -WIN };
    }
    let players = match view["players"].as_array() {
        Some(p) if p.len() == 2 => p,
        _ => return 0.0,
    };
    let (mine, theirs) = if players[0]["id"].as_i64() == Some(me) { (&players[0], &players[1]) } else { (&players[1], &players[0]) };
    let progress = |p: &Value| i(&p["expeditions"]["hero"]["position"]) + (7.0 - i(&p["expeditions"]["companion"]["position"]));
    let mut v = w.progress * (progress(mine) - progress(theirs));

    // Dusk outlook, side by side
    for side in ["hero", "companion"] {
        for (a, b, sign) in [(mine, theirs, 1.0), (theirs, mine, -1.0)] {
            let e = &a["expeditions"][side];
            let o = &b["expeditions"][side];
            if e["blocked"].as_bool().unwrap_or(false) {
                continue;
            }
            let pos = e["position"].as_i64().unwrap_or(0);
            let mut best = f64::MIN;
            let mut wins = false;
            for bio in region_biomes(view, pos) {
                let (x, y) = (i(&e["stats"][bio]), i(&o["stats"][bio]));
                if x > y && x > 0.0 {
                    wins = true;
                }
                best = best.max(x - y);
            }
            if wins {
                v += sign * w.dusk_win;
            }
            if best > f64::MIN {
                v += sign * w.margin * best.clamp(-w.margin_cap, w.margin_cap);
            }
            let total: f64 = BIOMES.iter().map(|b| i(&e["stats"][*b])).sum();
            v += sign * w.board * total;
            if total == 0.0 {
                v -= sign * w.empty_expedition;
            }
        }
    }

    // Resources
    let count = |owner: i64, zone: &str| view["cards"].as_array().map(|c| c.iter().filter(|c| c["owner"].as_i64() == Some(owner) && c["zone"] == zone).count()).unwrap_or(0) as f64;
    let opp = theirs["id"].as_i64().unwrap_or(0);
    v += w.hand * (i(&mine["handCount"]) - i(&theirs["handCount"]));
    v += w.reserve * (count(me, "reserve") - count(opp, "reserve"));
    v += w.landmark * (count(me, "landmark") - count(opp, "landmark"));
    let mana = |m: f64| w.mana * m.min(w.mana_soft) + w.mana_late * (m - w.mana_soft).max(0.0);
    v += mana(i(&mine["manaTotal"])) - mana(i(&theirs["manaTotal"]));
    if view["phase"] == "afternoon" {
        v += w.mana_ready * (i(&mine["manaAvailable"]) - i(&theirs["manaAvailable"]));
    }
    v
}
