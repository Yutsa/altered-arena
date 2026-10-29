//! Full state in a neutral format, hidden information included. The original engine exports
//! the same format (oracle), so a game can be started from the exact same position in both
//! engines and their states compared after each decision.
//!
//! {players: [{id, name, bot, position: [hero, companion]}], first: playerId, day,
//!  storm: [{card, rotated, revealed}],
//!  cards: [{id, ref, owner, zone, order, exhausted, boosts, statuses, counter}]}
//! Card ids are kept: tokens created later get the same ids in both engines.

use crate::catalog::catalog;
use crate::game::*;
use crate::model::*;
use crate::rng::Rng;
use serde_json::{json, Value};

fn zone_from(s: &str) -> Option<Zone> {
    Some(match s {
        "hero" => Zone::Hero,
        "hand" => Zone::Hand,
        "deck" => Zone::Deck,
        "reserve" => Zone::Reserve,
        "landmark" => Zone::Landmark,
        "heroExpedition" => Zone::HeroExpedition,
        "companionExpedition" => Zone::CompanionExpedition,
        "discard" => Zone::Discard,
        "mana" => Zone::Mana,
        "limbo" => Zone::Limbo,
        _ => return None,
    })
}

impl Game {
    /// Starts a game from a snapshot taken during the first-day mana selection
    pub fn from_snapshot(id: String, s: &Value) -> Res<Game> {
        let players = s["players"].as_array().filter(|p| p.len() == 2).ok_or(EngineError("2 players expected".into()))?;
        let cfg = |p: &Value| PlayerConfig {
            id: p["id"].as_u64().unwrap_or(0) as u32,
            name: p["name"].as_str().unwrap_or("?").to_string(),
            bot: p["bot"].as_bool().unwrap_or(false),
            deck: None,
        };
        let mut g = Game::new_empty(id, [cfg(&players[0]), cfg(&players[1])], s["seed"].as_u64().unwrap_or(1));
        for (seat, p) in players.iter().enumerate() {
            if let Some(pos) = p["position"].as_array() {
                g.players[seat].position = [pos[0].as_i64().unwrap_or(0) as i32, pos[1].as_i64().unwrap_or(7) as i32];
            }
        }
        g.dice = s["dice"].as_array().map(|a| a.iter().filter_map(|d| d.as_i64()).map(|d| d as i32).collect()).unwrap_or_default();
        g.dice_used = s["diceUsed"].as_u64().unwrap_or(0) as usize;
        g.first = s["first"].as_u64().and_then(|f| g.seat_of(f as u32)).unwrap_or(0);
        g.active = g.first;
        g.day = s["day"].as_u64().unwrap_or(1) as u32;
        g.storm = s["storm"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|t| StormCard { card: t["card"].as_u64().unwrap_or(0) as u8, rotated: t["rotated"].as_bool().unwrap_or(false), revealed: t["revealed"].as_bool().unwrap_or(false) })
                    .collect()
            })
            .unwrap_or_default();
        let cards = s["cards"].as_array().ok_or(EngineError("cards expected".into()))?;
        let max_id = cards.iter().filter_map(|c| c["id"].as_u64()).max().unwrap_or(0) as usize;
        let max_id = max_id.max(s["nextCardId"].as_u64().unwrap_or(0).saturating_sub(1) as usize);
        // Placeholders keep ids aligned with the original engine
        for i in 0..=max_id {
            g.cards.push(Card {
                id: i as CardId,
                def: 0,
                owner: 0,
                controller: 0,
                zone: Zone::Gone,
                exhausted: false,
                boosts: 0,
                statuses: vec![],
                counter: 0,
                stat_mods: [0; 3],
                played_from: None,
                completed: false,
            });
        }
        let cat = catalog();
        let mut sorted: Vec<&Value> = cards.iter().collect();
        sorted.sort_by_key(|c| (c["order"].as_i64().unwrap_or(0), c["id"].as_u64().unwrap_or(0)));
        for c in sorted {
            let id = c["id"].as_u64().unwrap() as CardId;
            let def = cat.get(c["ref"].as_str().unwrap_or("")).ok_or_else(|| EngineError(format!("unknown card {}", c["ref"])))?;
            let seat = g.seat_of(c["owner"].as_u64().unwrap_or(0) as u32).ok_or(EngineError("unknown owner".into()))?;
            let zone = zone_from(c["zone"].as_str().unwrap_or("")).unwrap_or(Zone::Gone);
            let card = &mut g.cards[id as usize];
            card.def = def.index;
            card.owner = seat;
            card.controller = seat;
            card.zone = zone;
            card.exhausted = c["exhausted"].as_bool().unwrap_or(false);
            card.boosts = c["boosts"].as_i64().unwrap_or(0) as i32;
            card.counter = c["counter"].as_i64().unwrap_or(0) as i32;
            card.statuses = c["statuses"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().and_then(Status::from_engine)).collect()).unwrap_or_default();
            match zone {
                Zone::Hero => g.players[seat].hero = Some(id),
                Zone::Gone => {}
                z => g.players[seat].zone_mut(z).push(id),
            }
        }
        if s["state"].as_str() == Some("firstDayManaSelection") {
            g.stack.push(Task::Step(Step::SetupMana));
        } else {
            return err("Snapshots are only supported at the first-day mana selection");
        }
        g.run();
        Ok(g)
    }

    /// Full state in the snapshot format
    pub fn snapshot(&self) -> Value {
        let zone_name = |z: Zone| -> &'static str {
            match z {
                Zone::Hero => "hero",
                Zone::Hand => "hand",
                Zone::Deck => "deck",
                Zone::Reserve => "reserve",
                Zone::Landmark => "landmark",
                Zone::HeroExpedition => "heroExpedition",
                Zone::CompanionExpedition => "companionExpedition",
                Zone::Discard => "discard",
                Zone::Mana => "mana",
                Zone::Limbo => "limbo",
                Zone::Gone => "gone",
            }
        };
        let mut cards = vec![];
        for (seat, p) in self.players.iter().enumerate() {
            if let Some(h) = p.hero {
                cards.push((seat, Zone::Hero, 0usize, h));
            }
            for z in [Zone::Hand, Zone::Deck, Zone::Reserve, Zone::Landmark, Zone::HeroExpedition, Zone::CompanionExpedition, Zone::Discard, Zone::Mana, Zone::Limbo] {
                for (i, &id) in p.zone(z).iter().enumerate() {
                    cards.push((seat, z, i, id));
                }
            }
        }
        let cards: Vec<Value> = cards
            .into_iter()
            .map(|(seat, z, i, id)| {
                let c = self.card(id);
                json!({
                    "id": id, "ref": c.def().reference, "owner": self.players[c.owner].id, "controller": self.players[seat].id,
                    "zone": zone_name(z), "order": i, "exhausted": c.exhausted, "boosts": c.boosts,
                    "statuses": c.statuses.iter().map(|s| s.name()).collect::<Vec<_>>(), "counter": c.counter,
                })
            })
            .collect();
        json!({
            "players": self.players.iter().map(|p| json!({"id": p.id, "name": p.name, "bot": p.bot, "position": p.position})).collect::<Vec<_>>(),
            "first": self.players[self.first].id,
            "day": self.day,
            "phase": self.phase,
            "storm": self.storm.iter().map(|t| json!({"card": t.card, "rotated": t.rotated, "revealed": t.revealed})).collect::<Vec<_>>(),
            "cards": cards,
        })
    }

    /// A game with players and nothing else (cards, storm and steps are set by the caller)
    pub fn new_empty(id: String, players: [PlayerConfig; 2], seed: u64) -> Game {
        let mut g = Game::new(id, players, seed);
        g.cards.clear();
        for p in g.players.iter_mut() {
            p.hero = None;
            p.hand.clear();
            p.deck.clear();
            p.mana.clear();
            p.position = [0, 7];
        }
        g.stack.clear();
        g.pending.clear();
        g.reactions.clear();
        g.log.clear();
        g.seq = 0;
        g.rng = Rng::new(seed);
        g
    }
}
