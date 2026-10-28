//! Afternoon actions: playing cards, support abilities (discard from Reserve), exhaust abilities, passing.

use crate::catalog::CardType;
use crate::game::*;
use crate::model::*;
use serde_json::{json, Value};

impl Game {
    /// Cost to play a card from its zone (reductions included), or None if it can't be paid this way
    pub fn play_cost(&self, seat: Seat, id: CardId, scout: bool) -> i32 {
        let c = self.card(id);
        let def = c.def();
        let mut cost = if scout {
            def.int("scout")
        } else if c.zone == Zone::Reserve {
            def.cost_reserve
        } else {
            def.cost_hand
        };
        for (s, kind, reduction) in &self.mods.cost_reductions {
            if *s != seat {
                continue;
            }
            let type_name = match def.card_type {
                CardType::Character => "character",
                CardType::Spell => "spell",
                CardType::Permanent => "permanent",
                CardType::Token => "token",
                CardType::Hero => "hero",
            };
            if kind == "all" || kind == type_name || def.has_subtype(kind) {
                cost -= reduction;
            }
        }
        cost.max(0)
    }

    fn destinations(&self, id: CardId, scout: bool) -> Vec<Dest> {
        let def = self.card(id).def();
        if scout {
            return vec![Dest::HeroExpeditionScout, Dest::CompanionExpeditionScout];
        }
        match def.card_type {
            CardType::Character | CardType::Token => vec![Dest::HeroExpedition, Dest::CompanionExpedition],
            CardType::Spell => vec![Dest::Spell],
            CardType::Permanent if def.has_subtype("expedition") => vec![Dest::HeroExpedition, Dest::CompanionExpedition],
            CardType::Permanent => vec![Dest::Landmark],
            CardType::Hero => vec![],
        }
    }

    /// Cards a player can play now, with where they can go
    pub fn playable(&self, seat: Seat, bonus: Option<&BonusPlay>) -> Vec<(CardId, Vec<Dest>)> {
        let p = &self.players[seat];
        let mana = self.mana_available(seat);
        let mut out = vec![];
        for &id in p.hand.iter().chain(p.reserve.iter()) {
            let c = self.card(id);
            let def = c.def();
            if c.zone == Zone::Reserve && c.exhausted {
                continue;
            }
            if let Some(b) = bonus {
                if !b.types.is_empty() && !b.types.contains(&def.card_type) {
                    continue;
                }
                if let Some(max) = b.max_base_cost {
                    let base = if c.zone == Zone::Reserve { def.cost_reserve } else { def.cost_hand };
                    if base > max {
                        continue;
                    }
                }
            }
            let free = bonus.map(|b| b.free).unwrap_or(false);
            let mut dests = vec![];
            if free || self.play_cost(seat, id, false) <= mana {
                dests.extend(self.destinations(id, false));
            }
            if c.zone == Zone::Hand && def.int("scout") > 0 && bonus.is_none() && self.play_cost(seat, id, true) <= mana {
                dests.extend(self.destinations(id, true));
            }
            if !dests.is_empty() {
                out.push((id, dests));
            }
        }
        out
    }

    pub fn support_options(&self, seat: Seat) -> Vec<CardId> {
        self.players[seat]
            .reserve
            .iter()
            .copied()
            .filter(|&id| {
                let c = self.card(id);
                !c.exhausted
                    && c.def().effect("effectSupport").is_some()
                    && c.def().prop("supportIcon").and_then(Value::as_str) != Some("infinity")
            })
            .collect()
    }

    pub fn tap_options(&self, seat: Seat) -> Vec<CardId> {
        let p = &self.players[seat];
        let mut ids: Vec<CardId> = self.in_play(seat);
        if let Some(h) = p.hero {
            ids.push(h);
        }
        ids.into_iter()
            .filter(|&id| {
                let c = self.card(id);
                if c.exhausted {
                    return false;
                }
                match c.def().effect("effectTap") {
                    Some(effect) => crate::flow::is_doable(self, effect, &Ctx::of(seat, Some(id))),
                    None => false,
                }
            })
            .collect()
    }

    pub fn play_card(&mut self, seat: Seat, id: CardId, dest: Dest, bonus: Option<&BonusPlay>) -> Res<()> {
        let options = self.playable(seat, bonus);
        let Some((_, dests)) = options.iter().find(|(c, _)| *c == id) else { return err("This card can't be played now") };
        if !dests.contains(&dest) {
            return err("This card can't be played there");
        }
        let free = bonus.map(|b| b.free).unwrap_or(false);
        let cost = if free { 0 } else { self.play_cost(seat, id, dest.is_scout()) };
        if !self.pay(seat, cost) {
            return err("Not enough mana");
        }
        // Cost reductions apply to one card
        let def = self.card(id).def();
        self.mods.cost_reductions.retain(|(s, kind, _)| {
            !(*s == seat && (kind == "all" || def.has_subtype(kind) || format!("{:?}", def.card_type).to_lowercase() == *kind))
        });

        let from = self.card(id).zone;
        let to = dest.zone();
        self.move_card(id, to, Some(seat));
        self.card_mut(id).played_from = Some(from);
        let is_char = matches!(def.card_type, CardType::Character | CardType::Token);
        if from == Zone::Reserve && !def.has_subtype("landmark") {
            self.card_mut(id).add_status(Status::Fleeting);
        }
        if is_char {
            if self.mods.next_character_fleeting[seat] {
                self.card_mut(id).add_status(Status::Fleeting);
                self.mods.next_character_fleeting[seat] = false;
            }
            if self.mods.next_character_anchored[seat] {
                self.card_mut(id).add_status(Status::Anchored);
                self.mods.next_character_anchored[seat] = false;
            }
            let mut boost = std::mem::take(&mut self.mods.next_character_boost[seat]);
            if def.has_subtype("animal") {
                boost += std::mem::take(&mut self.mods.next_animal_boost[seat]);
            }
            if from == Zone::Reserve {
                boost += std::mem::take(&mut self.mods.next_reserve_character_boost[seat]);
            }
            self.card_mut(id).boosts += boost;
        }

        let name = self.players[seat].name.clone();
        let where_ = match dest {
            Dest::HeroExpedition | Dest::HeroExpeditionScout => " in their hero expedition",
            Dest::CompanionExpedition | Dest::CompanionExpeditionScout => " in their companion expedition",
            Dest::Landmark => " as a Landmark",
            Dest::Reserve => " in Reserve",
            Dest::Spell => "",
        };
        let from_label = if from == Zone::Reserve { " from Reserve" } else { "" };
        self.log_line(Some(seat), format!("{} plays [[{}]]{}{} ({} mana)", name, def.name, from_label, where_, cost));

        // Resolution: effects, then (spells) cleanup, then the end of the play
        self.stack.push(Task::PlayDone { turn_play: bonus.is_none() });
        if def.card_type == CardType::Spell {
            self.stack.push(Task::SpellCleanup { card: id });
        }
        let mut effects: Vec<Value> = vec![];
        let mut add = |e: Option<&Value>| {
            if let Some(e) = e {
                if e.get("childs").is_some() && e.get("type").is_none() {
                    effects.extend(e["childs"].as_array().cloned().unwrap_or_default());
                } else if e.get("type").and_then(Value::as_str) == Some("parallel") {
                    effects.extend(e["childs"].as_array().cloned().unwrap_or_default());
                } else {
                    effects.push(e.clone());
                }
            }
        };
        add(def.effect("effectPlayed"));
        if from == Zone::Hand {
            add(def.effect("effectHand"));
        }
        if from == Zone::Reserve {
            add(def.effect("effectReserve"));
        }
        if dest.is_scout() {
            effects.push(json!({"action": "Discard", "args": {"cardId": "me", "destination": "reserve"}}));
        }
        let ctx = Ctx::of(seat, Some(id));
        for e in effects.into_iter().rev() {
            self.stack.push(Task::Run { node: e, ctx: ctx.clone() });
        }
        self.emit(Event {
            trigger: "ChooseAssignment".into(),
            seat,
            card: Some(id),
            from: Some(from),
            to: Some(to),
            listeners: vec![id],
            play: true,
            ..Default::default()
        });
        Ok(())
    }

    pub fn use_support(&mut self, seat: Seat, id: CardId) -> Res<()> {
        if !self.support_options(seat).contains(&id) {
            return err("This support ability can't be used now");
        }
        let effect = self.card(id).def().effect("effectSupport").cloned();
        let name = self.card(id).name();
        self.move_card(id, Zone::Discard, None);
        let pname = self.players[seat].name.clone();
        self.log_line(Some(seat), format!("{} discards [[{}]] from Reserve for its support ability", pname, name));
        if let Some(e) = effect {
            self.stack.push(Task::Run { node: e, ctx: Ctx::of(seat, Some(id)) });
        }
        self.emit(Event {
            trigger: "Discard".into(),
            seat,
            card: Some(id),
            from: Some(Zone::Reserve),
            to: Some(Zone::Discard),
            listeners: vec![id],
            discard: true,
            support: true,
            ..Default::default()
        });
        Ok(())
    }

    pub fn use_tap(&mut self, seat: Seat, id: CardId) -> Res<()> {
        if !self.tap_options(seat).contains(&id) {
            return err("This ability can't be used now");
        }
        self.card_mut(id).exhausted = true;
        let effect = self.card(id).def().effect("effectTap").cloned();
        let name = self.card(id).name();
        let pname = self.players[seat].name.clone();
        self.log_line(Some(seat), format!("{} exhausts [[{}]]", pname, name));
        if let Some(e) = effect {
            self.stack.push(Task::Run { node: e, ctx: Ctx::of(seat, Some(id)) });
        }
        self.emit(Event { trigger: "Exhaust".into(), seat, card: Some(id), source: Some(id), ..Default::default() });
        Ok(())
    }

    pub fn end_turn(&mut self, seat: Seat) {
        self.players[seat].passed = true;
        let name = self.players[seat].name.clone();
        self.log_line(Some(seat), format!("{} passes", name));
        self.emit(Event { trigger: "EndTurn".into(), seat, ..Default::default() });
    }
}
