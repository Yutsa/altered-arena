//! GameView of protocol v1 (protocol/protocol.ts): what one player may see, and their decision.

use crate::answer::expedition_key;
use crate::catalog::CardType;
use crate::game::*;
use crate::model::*;
use serde_json::{json, Value};

fn zone_name(z: Zone) -> &'static str {
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
        Zone::Limbo | Zone::Gone => "limbo",
    }
}

fn biomes(v: [i32; 3]) -> Value {
    json!({"forest": v[0], "mountain": v[1], "ocean": v[2]})
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Hero => "hero",
        Side::Companion => "companion",
    }
}

impl Game {
    fn card_view(&self, id: CardId, position: usize, visible: bool) -> Value {
        let c = self.card(id);
        let def = c.def();
        let mut v = json!({
            "id": id,
            "ref": if visible { Value::String(def.reference.clone()) } else { Value::Null },
            "owner": self.players[c.owner].id,
            "zone": zone_name(c.zone),
            "position": position,
            "exhausted": c.exhausted,
            "boosts": c.boosts,
            "counters": if c.counter > 0 { json!({"counter": c.counter}) } else { json!({}) },
        });
        let mut statuses: Vec<&str> = c.statuses.iter().map(|s| s.name()).collect();
        for kw in ["gigantic", "tough", "defender", "eternal", "seasoned"] {
            if def.flag(kw) {
                statuses.push(kw);
            }
        }
        v["statuses"] = json!(if visible { statuses } else { vec![] });
        if c.controller != c.owner {
            v["controller"] = json!(self.players[c.controller].id);
        }
        if visible {
            if def.is_token {
                v["token"] = json!(true);
            }
            if matches!(def.card_type, CardType::Character | CardType::Token) || def.is_token {
                v["stats"] = biomes(c.stats());
            }
            if def.card_type != CardType::Hero && !def.is_token {
                let seat = c.controller;
                let hand = if c.zone == Zone::Hand { self.play_cost(seat, id, false) } else { def.cost_hand };
                let reserve = if c.zone == Zone::Reserve { self.play_cost(seat, id, false) } else { def.cost_reserve };
                v["costHand"] = json!(hand);
                v["costReserve"] = json!(reserve);
            }
        }
        v
    }

    /// The table as seen by a player (None: spectator)
    pub fn view(&self, viewer: Option<Seat>) -> Value {
        let mut cards = vec![];
        for (seat, p) in self.players.iter().enumerate() {
            if let Some(h) = p.hero {
                cards.push(self.card_view(h, 0, true));
            }
            let mut zones: Vec<(&Vec<CardId>, bool)> = vec![
                (&p.reserve, true),
                (&p.landmarks, true),
                (&p.expeditions[0], true),
                (&p.expeditions[1], true),
                (&p.discard, true),
                (&p.limbo, true),
            ];
            if viewer == Some(seat) {
                zones.push((&p.hand, true));
                zones.push((&p.mana, true));
            }
            for (ids, visible) in zones {
                for (i, &id) in ids.iter().enumerate() {
                    cards.push(self.card_view(id, i, visible));
                }
            }
        }
        let players: Vec<Value> = self
            .players
            .iter()
            .enumerate()
            .map(|(seat, p)| {
                let hero = p.hero.map(|h| self.card(h).def());
                json!({
                    "id": p.id,
                    "name": p.name,
                    "bot": p.bot,
                    "faction": hero.map(|h| h.faction.clone()),
                    "heroCardId": p.hero,
                    "handCount": p.hand.len(),
                    "deckCount": p.deck.len(),
                    "discardCount": p.discard.len(),
                    "manaTotal": p.mana.len(),
                    "manaAvailable": self.mana_available(seat),
                    "passed": p.passed,
                    "reserveSlots": hero.map(|h| h.reserve_slots).unwrap_or(2),
                    "landmarkSlots": hero.map(|h| h.landmark_slots).unwrap_or(2),
                    "expeditions": {
                        "hero": {"position": p.position[0], "stats": biomes(self.expedition_stats(seat, Side::Hero)), "blocked": p.blocked[0]},
                        "companion": {"position": p.position[1], "stats": biomes(self.expedition_stats(seat, Side::Companion)), "blocked": p.blocked[1]},
                    },
                })
            })
            .collect();
        let regions: Vec<Value> = (0..8)
            .map(|i| {
                let tile = &self.storm[((i + 1) / 2) as usize];
                let visible = i == 0 || i == 7 || tile.revealed;
                let b: Option<Vec<Biome>> = if visible { Some(self.region_biomes(i)) } else { None };
                json!({
                    "index": i,
                    "biomes": b,
                    "kind": if i == 0 { "heroRegion" } else if i == 7 { "companionRegion" } else { "tumult" },
                })
            })
            .collect();
        let decision = viewer.and_then(|s| self.pending_for(s)).map(|p| self.decision_view(p));
        let waiting: Vec<u32> = self.pending.iter().map(|p| self.players[p.seat].id).collect();
        let log: Vec<Value> = self
            .log
            .iter()
            .rev()
            .take(100)
            .rev()
            .map(|l| {
                let mut v = json!({"seq": l.seq, "text": l.text});
                if let Some(s) = l.player {
                    v["player"] = json!(self.players[s].id);
                }
                v
            })
            .collect();
        let phase = if self.tiebreaker && self.phase != Phase::Ended { json!("tiebreaker") } else { json!(self.phase) };
        json!({
            "gameId": self.id,
            "you": viewer.map(|s| self.players[s].id),
            "seq": self.seq,
            "day": self.day,
            "phase": phase,
            "firstPlayer": self.players[self.first].id,
            "activePlayer": if self.phase == Phase::Afternoon { json!(self.players[self.active].id) } else { Value::Null },
            "winner": self.winner.map(|w| self.players[w].id),
            "players": players,
            "cards": cards,
            "regions": regions,
            "decision": decision,
            "waitingFor": waiting,
            "log": log,
        })
    }

    fn decision_view(&self, p: &Pending) -> Value {
        let mut v = json!({
            "id": format!("d{}", p.id),
            "prompt": p.prompt,
        });
        if let Some(s) = p.source {
            v["sourceCardId"] = json!(s);
        }
        if p.can_pass {
            v["canPass"] = json!(true);
        }
        if self.can_undo(p.seat) {
            v["canUndo"] = json!(true);
        }
        let extra = match &p.kind {
            PendingKind::Turn { play, support, tap, can_end, .. } => json!({
                "kind": "turn",
                "play": play.iter().map(|(c, d)| json!({"cardId": c, "destinations": d, "cost": self.play_cost(p.seat, *c, false)})).collect::<Vec<_>>(),
                "support": support,
                "tap": tap,
                "canEndTurn": can_end,
            }),
            PendingKind::Choice { options } | PendingKind::Order { options, .. } => json!({
                "kind": "choice",
                "choices": options.iter().enumerate().map(|(i, o)| {
                    let mut c = json!({"id": i, "label": o.label, "enabled": o.enabled});
                    if let Some(s) = o.source { c["sourceCardId"] = json!(s); }
                    c
                }).collect::<Vec<_>>(),
            }),
            PendingKind::Select { cards, min, max, costs, picker, .. } => {
                let costs: serde_json::Map<String, Value> = costs.iter().map(|(c, t)| (c.to_string(), json!(t))).collect();
                json!({"kind": "select", "cardIds": cards, "min": min, "max": max, "costs": costs, "showInPicker": picker})
            }
            PendingKind::Expedition { options, min, max, .. } => json!({
                "kind": "expedition",
                "expeditions": options.iter().map(|(s, side)| json!({"key": expedition_key(*s, *side), "player": self.players[*s].id, "side": side_name(*side)})).collect::<Vec<_>>(),
                "min": min,
                "max": max,
            }),
            PendingKind::Player { options, .. } => json!({"kind": "player", "players": options.iter().map(|s| self.players[*s].id).collect::<Vec<_>>()}),
            PendingKind::Number { values, .. } => json!({"kind": "number", "values": values}),
            PendingKind::Location { options, .. } => json!({
                "kind": "location",
                "locations": options.iter().map(|(k, l, s, z)| json!({"key": k, "label": l, "player": self.players[*s].id, "zone": zone_name(*z)})).collect::<Vec<_>>(),
            }),
            PendingKind::Cleanup { reserve, n_reserve, landmarks, n_landmarks } => json!({
                "kind": "cleanup",
                "reserve": {"cardIds": reserve, "n": n_reserve},
                "landmarks": {"cardIds": landmarks, "n": n_landmarks},
            }),
            PendingKind::Exchange { hand, reserve } => json!({"kind": "exchange", "hand": hand, "reserve": reserve}),
        };
        if let (Value::Object(a), Value::Object(b)) = (&mut v, extra) {
            a.extend(b);
        }
        v
    }

    /// Decision id as shown in views
    pub fn decision_id(&self, seat: Seat) -> Option<String> {
        self.pending_for(seat).map(|p| format!("d{}", p.id))
    }
}
