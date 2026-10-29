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
        if crate::special::spell_is_free(self, seat, id) {
            return 0;
        }
        cost.max(0)
    }

    /// Cost to play a card to a given destination: some cards cost more or less depending on
    /// where they go (playLimitation: Lyra DJ "-2Contact", "+3StartingRegion", "-2Multi")
    pub fn play_cost_at(&self, seat: Seat, id: CardId, dest: Dest) -> i32 {
        let mut cost = self.play_cost(seat, id, dest.is_scout());
        if dest.is_scout() {
            return cost;
        }
        let limitation = self.card(id).def().prop("playLimitation").and_then(Value::as_str).unwrap_or("");
        let side = dest.zone().side();
        match (limitation, side) {
            ("+3StartingRegion", Some(Side::Hero)) if self.players[seat].position[0] == 0 => cost += 3,
            ("+3StartingRegion", Some(Side::Companion)) if self.players[seat].position[1] == 7 => cost += 3,
            ("-2Contact", Some(side)) if self.in_contact(seat, side) => cost -= 2,
            ("-2Multi", _) => {
                let opp = &self.players[Game::other(seat)];
                if opp.expeditions.iter().flatten().any(|&c| self.card(c).is_character_like()) {
                    cost -= 2;
                }
            }
            _ => {}
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
        let only = bonus.and_then(|b| b.card);
        let cards: Vec<CardId> = match only {
            Some(c) => vec![c],
            None => p.hand.iter().chain(p.reserve.iter()).copied().collect(),
        };
        for id in cards {
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
            for d in self.destinations(id, false) {
                if free || self.play_cost_at(seat, id, d) <= mana {
                    dests.push(d);
                }
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
        let cost = if free { 0 } else { self.play_cost_at(seat, id, dest) };
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
        // Afanas & Senka: Spells with Hand Cost 1 or less gain Fleeting
        let hero_flag = self.players[seat].hero.map(|h| self.card(h).def().flag("allSpell1Fleeting")).unwrap_or(false);
        if hero_flag && def.card_type == CardType::Spell && def.cost_hand <= 1 {
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
        if bonus.is_some() {
            self.mods.bonus_played = true;
        }
        self.stack.push(Task::PlayDone { turn_play: bonus.is_none() });
        if def.card_type == CardType::Spell {
            self.stack.push(Task::SpellCleanup { card: id });
        }
        let mut effects: Vec<Value> = vec![];
        // Scout: "{H} Send me to Reserve" comes first, as in the original engine
        if dest.is_scout() {
            effects.push(json!({"action": "Discard", "args": {"cardId": "me", "destination": "reserve"}}));
        }
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
        // Effects set up by special effects (next character gains Asleep, activates its {R}...)
        effects.extend(crate::special::on_play(self, seat, id, from, to));
        // The effects of a played card, and the abilities its play triggers, resolve in the
        // order its player chooses
        let ctx = Ctx::of(seat, Some(id));
        let before = self.reactions.len();
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
        let mut all: Vec<(Value, Ctx)> = effects.into_iter().map(|e| (e, ctx.clone())).collect();
        // A spell resolves on its own; the abilities it triggers wait until it is done
        if def.card_type != CardType::Spell {
            // Card effects plus triggered abilities: the player orders everything (noIndependent)
            let triggered: Vec<(Value, Ctx)> = self.reactions.drain(before..).collect();
            let auto = all.is_empty() || triggered.is_empty();
            all.extend(triggered);
            self.stack.push(Task::Reactions(all, auto));
        } else {
            self.stack.push(Task::SpellEffects(all));
        }
        // "Immediate" abilities (Teija & Nauraa...) resolve before anything else
        let before_immediate = self.reactions.len();
        self.emit(Event {
            trigger: "ImmediateChooseAssignment".into(),
            seat,
            card: Some(id),
            from: Some(from),
            to: Some(to),
            listeners: vec![id],
            play: true,
            ..Default::default()
        });
        let immediate: Vec<(Value, Ctx)> = self.reactions.drain(before_immediate..).collect();
        if !immediate.is_empty() {
            self.stack.push(Task::Reactions(immediate, true));
        }
        Ok(())
    }

    pub fn use_support(&mut self, seat: Seat, id: CardId) -> Res<()> {
        if !self.support_options(seat).contains(&id) {
            return err("This support ability can't be used now");
        }
        let effect = self.card(id).def().effect("effectSupport").cloned();
        let name = self.card(id).name();
        self.move_card(id, Zone::Discard, None);
        self.note_ability(seat, "discard");
        self.note_ability(seat, "discardFromHandOrReserve");
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
        self.note_ability(seat, "tap");
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
        if !self.players[Game::other(seat)].passed {
            self.players[seat].records.passed_first = true;
        }
        self.players[seat].passed = true;
        let name = self.players[seat].name.clone();
        self.log_line(Some(seat), format!("{} passes", name));
        self.emit(Event { trigger: "EndTurn".into(), seat, ..Default::default() });
    }
}

impl Game {
    /// Dynamic Tough strings of a card (completed Feats can add some)
    fn dynamic_tough(&self, id: CardId) -> Vec<String> {
        let c = self.card(id);
        let def = c.def();
        let mut out: Vec<String> = match def.prop("dynamicTough") {
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            _ => vec![],
        };
        if c.completed {
            match def.prop("effectCompleted").and_then(|e| e.get("dynamicTough")) {
                Some(Value::String(s)) if !s.is_empty() => out.push(s.clone()),
                Some(Value::Array(a)) => out.extend(a.iter().filter_map(|v| v.as_str().map(String::from))),
                _ => {}
            }
        }
        // "value:condition": the condition is not evaluated here (approximation)
        out.into_iter().map(|t| t.split(':').next().unwrap_or("").to_string()).collect()
    }

    /// Tough of a card: the extra mana an opponent pays to target it (Card::getTough)
    pub fn tough(&self, id: CardId) -> i32 {
        let c = self.card(id);
        if !c.zone.in_play() {
            return 0;
        }
        let def = c.def();
        let seat = c.controller;
        let mut tough = def.int("tough");
        for t in self.dynamic_tough(id) {
            match t.as_str() {
                "region" if !self.tiebreaker => tough += (self.players[seat].gap() - 1).max(0),
                "controlledPlants" => tough += self.in_play(seat).iter().filter(|&&x| self.card(x).def().has_subtype("plant")).count() as i32,
                "tough1" => tough += 1,
                "tough2" => tough += 2,
                "exhaustedReserve" => {
                    tough += self.players.iter().flat_map(|p| p.reserve.iter()).filter(|&&x| self.card(x).exhausted).count() as i32
                }
                _ => {}
            }
        }
        let played = self.in_play(seat);
        if c.is_character_like() {
            for &src in &played {
                let scope = self.card(src).def().prop("effectCompleted").and_then(|e| e.get("universalToughScope")).and_then(Value::as_str);
                let applies = match scope {
                    Some("expedition") => c.zone.is_expedition(),
                    Some("expeditionAnchored") => c.zone.is_expedition() && c.has(Status::Anchored),
                    _ => true,
                };
                if !applies {
                    continue;
                }
                for t in self.dynamic_tough(src) {
                    tough += match t.as_str() {
                        "universalCharacter2" => 2,
                        "universalCharacter1" => 1,
                        _ => 0,
                    };
                }
            }
            let boosted = played.iter().any(|&x| {
                let o = self.card(x);
                (o.zone == c.zone || c.is_gigantic()) && o.def().prop("expeditionTough").and_then(Value::as_str) == Some("boosted")
            });
            if boosted && c.boosts > 0 {
                tough += 1;
            }
            let anchored_asleep = played.iter().any(|&x| self.dynamic_tough(x).iter().any(|t| t == "anchoredOrAsleep"));
            if anchored_asleep && (c.has(Status::Anchored) || c.has(Status::Asleep)) {
                tough += 1;
            }
        }
        if def.card_type == CardType::Permanent {
            let mut best = 0;
            for &l in &self.players[seat].landmarks {
                let lc = self.card(l);
                if !lc.completed {
                    continue;
                }
                if let Some(Value::String(dt)) = lc.def().prop("effectCompleted").and_then(|e| e.get("dynamicTough")) {
                    if let Some(n) = dt.strip_prefix("universalLandmarks").and_then(|n| n.parse::<i32>().ok()) {
                        best = best.max(n);
                    }
                }
            }
            tough += best;
        }
        tough + crate::special::global_tough(self, id)
    }

    /// Extra cost to target a card of the opponent (Tough, and completed-Feat protection in Reserve)
    pub fn target_cost(&self, id: CardId) -> i32 {
        let c = self.card(id);
        let mut cost = self.tough(id).max(0);
        if c.zone == Zone::Reserve && c.is_character_like() {
            for &l in &self.players[c.controller].landmarks {
                let lc = self.card(l);
                if lc.completed {
                    cost += lc.def().prop("effectCompleted").and_then(|e| e.get("reserveCharacterTough")).and_then(Value::as_i64).unwrap_or(0) as i32;
                }
            }
        }
        cost
    }
}
