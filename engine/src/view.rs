//! GameView of protocol v1 (protocol/protocol.ts): what one player may see, and their decision.

use crate::answer::expedition_key;
use crate::catalog::CardType;
use crate::game::*;
use crate::model::*;
use serde::Serialize;
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

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Hero => "hero",
        Side::Companion => "companion",
    }
}

#[derive(Serialize)]
struct Biomes {
    forest: i32,
    mountain: i32,
    ocean: i32,
}

fn biomes3(v: [i32; 3]) -> Biomes {
    Biomes { forest: v[0], mountain: v[1], ocean: v[2] }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardView<'a> {
    id: CardId,
    #[serde(rename = "ref")]
    reference: Option<&'a str>,
    owner: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    controller: Option<u32>,
    zone: &'static str,
    position: usize,
    exhausted: bool,
    boosts: i32,
    counters: Counters,
    statuses: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<Biomes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_hand: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_reserve: Option<i32>,
}

#[derive(Serialize)]
struct Counters {
    #[serde(skip_serializing_if = "Option::is_none")]
    counter: Option<i32>,
}

#[derive(Serialize)]
struct ExpeditionView {
    position: i32,
    stats: Biomes,
    blocked: bool,
}

#[derive(Serialize)]
struct Expeditions {
    hero: ExpeditionView,
    companion: ExpeditionView,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerView<'a> {
    id: u32,
    name: &'a str,
    bot: bool,
    faction: Option<&'a str>,
    hero_card_id: Option<CardId>,
    hand_count: usize,
    deck_count: usize,
    discard_count: usize,
    mana_total: usize,
    mana_available: i32,
    passed: bool,
    reserve_slots: i32,
    landmark_slots: i32,
    expeditions: Expeditions,
}

#[derive(Serialize)]
struct RegionView {
    index: i32,
    biomes: Option<Vec<Biome>>,
    kind: &'static str,
}

#[derive(Serialize)]
struct LogView<'a> {
    seq: u64,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    player: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameView<'a> {
    game_id: &'a str,
    you: Option<u32>,
    seq: u64,
    day: u32,
    phase: &'static str,
    first_player: u32,
    active_player: Option<u32>,
    winner: Option<u32>,
    players: Vec<PlayerView<'a>>,
    cards: Vec<CardView<'a>>,
    regions: Vec<RegionView>,
    decision: Option<Value>,
    waiting_for: Vec<u32>,
    log: Vec<LogView<'a>>,
}

impl Game {
    fn card_view(&self, id: CardId, position: usize, visible: bool) -> CardView<'_> {
        let c = self.card(id);
        let def = c.def();
        let mut v = CardView {
            id,
            reference: if visible { Some(def.reference.as_str()) } else { None },
            owner: self.players[c.owner].id,
            controller: if c.controller != c.owner { Some(self.players[c.controller].id) } else { None },
            zone: zone_name(c.zone),
            position,
            exhausted: c.exhausted,
            boosts: c.boosts,
            counters: Counters { counter: if c.counter > 0 { Some(c.counter) } else { None } },
            statuses: vec![],
            token: None,
            stats: None,
            cost_hand: None,
            cost_reserve: None,
        };
        if visible {
            v.statuses = c.statuses.iter().map(|s| s.name()).collect();
            for kw in ["gigantic", "tough", "defender", "eternal", "seasoned"] {
                if def.flag(kw) {
                    v.statuses.push(kw);
                }
            }
            if def.is_token {
                v.token = Some(true);
            }
            if matches!(def.card_type, CardType::Character | CardType::Token) || def.is_token {
                v.stats = Some(biomes3(c.stats()));
            }
            if def.card_type != CardType::Hero && !def.is_token {
                let seat = c.controller;
                v.cost_hand = Some(if c.zone == Zone::Hand { self.play_cost(seat, id, false) } else { def.cost_hand });
                v.cost_reserve = Some(if c.zone == Zone::Reserve { self.play_cost(seat, id, false) } else { def.cost_reserve });
            }
        }
        v
    }

    fn game_view(&self, viewer: Option<Seat>) -> GameView<'_> {
        let mut cards = Vec::with_capacity(96);
        for (seat, p) in self.players.iter().enumerate() {
            if let Some(h) = p.hero {
                cards.push(self.card_view(h, 0, true));
            }
            let mut zones: [Option<&Vec<CardId>>; 8] =
                [Some(&p.reserve), Some(&p.landmarks), Some(&p.expeditions[0]), Some(&p.expeditions[1]), Some(&p.discard), Some(&p.limbo), None, None];
            if viewer == Some(seat) {
                zones[6] = Some(&p.hand);
                zones[7] = Some(&p.mana);
            }
            for ids in zones.into_iter().flatten() {
                for (i, &id) in ids.iter().enumerate() {
                    cards.push(self.card_view(id, i, true));
                }
            }
        }
        let players = self
            .players
            .iter()
            .enumerate()
            .map(|(seat, p)| {
                let hero = p.hero.map(|h| self.card(h).def());
                PlayerView {
                    id: p.id,
                    name: &p.name,
                    bot: p.bot,
                    faction: hero.map(|h| h.faction.as_str()),
                    hero_card_id: p.hero,
                    hand_count: p.hand.len(),
                    deck_count: p.deck.len(),
                    discard_count: p.discard.len(),
                    mana_total: p.mana.len(),
                    mana_available: self.mana_available(seat),
                    passed: p.passed,
                    reserve_slots: hero.map(|h| h.reserve_slots).unwrap_or(2),
                    landmark_slots: hero.map(|h| h.landmark_slots).unwrap_or(2),
                    expeditions: Expeditions {
                        hero: ExpeditionView { position: p.position[0], stats: biomes3(self.expedition_stats(seat, Side::Hero)), blocked: p.blocked[0] },
                        companion: ExpeditionView {
                            position: p.position[1],
                            stats: biomes3(self.expedition_stats(seat, Side::Companion)),
                            blocked: p.blocked[1],
                        },
                    },
                }
            })
            .collect();
        let regions = (0..8)
            .map(|i| {
                let tile = &self.storm[((i + 1) / 2) as usize];
                let visible = i == 0 || i == 7 || tile.revealed;
                RegionView {
                    index: i,
                    biomes: if visible { Some(self.region_biomes(i)) } else { None },
                    kind: if i == 0 { "heroRegion" } else if i == 7 { "companionRegion" } else { "tumult" },
                }
            })
            .collect();
        // Recent lines only: clients keep the history they already received
        let from = self.log.len().saturating_sub(40);
        let log = self.log[from..].iter().map(|l| LogView { seq: l.seq, text: &l.text, player: l.player.map(|s| self.players[s].id) }).collect();
        let phase = if self.tiebreaker && self.phase != Phase::Ended {
            "tiebreaker"
        } else {
            match self.phase {
                Phase::Setup => "setup",
                Phase::Morning => "morning",
                Phase::Noon => "noon",
                Phase::Afternoon => "afternoon",
                Phase::Dusk => "dusk",
                Phase::Night => "night",
                Phase::Ended => "ended",
            }
        };
        GameView {
            game_id: &self.id,
            you: viewer.map(|s| self.players[s].id),
            seq: self.seq,
            day: self.day,
            phase,
            first_player: self.players[self.first].id,
            active_player: if self.phase == Phase::Afternoon { Some(self.players[self.active].id) } else { None },
            winner: self.winner.map(|w| self.players[w].id),
            players,
            cards,
            regions,
            decision: viewer.and_then(|s| self.pending_for(s)).map(|p| self.decision_view(p)),
            waiting_for: self.pending.iter().map(|p| self.players[p.seat].id).collect(),
            log,
        }
    }

    /// The table as seen by a player (None: spectator), as a JSON value
    pub fn view(&self, viewer: Option<Seat>) -> Value {
        serde_json::to_value(self.game_view(viewer)).unwrap_or(Value::Null)
    }

    /// The table as seen by a player, serialized (no intermediate JSON tree)
    pub fn view_json(&self, viewer: Option<Seat>) -> Vec<u8> {
        serde_json::to_vec(&self.game_view(viewer)).unwrap_or_default()
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
