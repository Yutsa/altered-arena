//! Players' answers to decisions (protocol v1 `Answer`, without decisionId which the view layer checks).

use crate::actions::{apply_targets, create_token, discard_add, move_with_events, resolve_die};
use crate::game::*;
use crate::model::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Answer {
    Play { card_id: CardId, destination: Dest },
    Support { card_id: CardId },
    Tap { card_id: CardId },
    EndTurn,
    Select { card_ids: Vec<CardId> },
    Choice { id: usize },
    Confirm,
    Expedition { keys: Vec<String> },
    Player { player: u32 },
    Number { value: i32 },
    Location { key: String },
    Exchange { hand: CardId, reserve: CardId },
    Cleanup { reserve: Vec<CardId>, landmarks: Vec<CardId> },
    Deck { deck: String },
    Button { key: String },
    Pass,
    Undo,
}

pub fn expedition_key(seat: Seat, side: Side) -> String {
    format!("{}:{}", seat, if side == Side::Hero { "hero" } else { "companion" })
}

impl Game {
    pub fn pending_for(&self, seat: Seat) -> Option<&Pending> {
        self.pending.iter().find(|p| p.seat == seat)
    }

    pub fn can_undo(&self, seat: Seat) -> bool {
        // Something must have happened since the snapshot
        self.undo_owner == Some(seat)
            && self.undo_point.as_ref().map(|u| u.seq < self.seq).unwrap_or(false)
            && self.pending.len() == 1
            && self.pending[0].seat == seat
    }

    /// Applies a player's answer and runs the game until the next decision
    pub fn answer(&mut self, seat: Seat, answer: Answer) -> Res<()> {
        if let Answer::Undo = answer {
            if !self.can_undo(seat) {
                return err("Nothing to undo");
            }
            let snap = self.undo_point.take().unwrap();
            let seq = self.seq;
            *self = *snap;
            self.seq = seq + 1;
            self.set_undo_point(seat);
            return Ok(());
        }
        let Some(index) = self.pending.iter().position(|p| p.seat == seat) else { return err("Nothing to decide") };
        // Answers are validated before anything changes: a refused answer leaves the game as it was
        let pending = self.pending.remove(index);
        if let Err(e) = self.apply(pending.clone(), answer) {
            self.pending.insert(index, pending);
            return Err(e);
        }
        self.seq += 1;
        self.run();
        Ok(())
    }

    fn apply(&mut self, p: Pending, answer: Answer) -> Res<()> {
        let seat = p.seat;
        let ctx = p.ctx.clone();
        if let Answer::Pass = answer {
            if !p.can_pass {
                return err("This step can't be skipped");
            }
            return Ok(());
        }
        match (p.kind, answer) {
            (PendingKind::Turn { bonus, .. }, Answer::Play { card_id, destination }) => self.play_card(seat, card_id, destination, bonus.as_ref()),
            (PendingKind::Turn { bonus: None, .. }, Answer::Support { card_id }) => self.use_support(seat, card_id),
            (PendingKind::Turn { bonus: None, .. }, Answer::Tap { card_id }) => self.use_tap(seat, card_id),
            (PendingKind::Turn { can_end: true, .. }, Answer::EndTurn) => {
                self.end_turn(seat);
                Ok(())
            }
            (PendingKind::Choice { options }, Answer::Choice { id }) => {
                let Some(o) = options.get(id).filter(|o| o.enabled) else { return err("Unknown choice") };
                if let Some(n) = &o.node {
                    self.stack.push(Task::Run { node: n.clone(), ctx });
                }
                Ok(())
            }
            (PendingKind::Order { reactions, spell, auto, options }, Answer::Choice { id }) => {
                let mut rest = reactions;
                if id >= rest.len() || !options[id].enabled {
                    return err("Unknown choice");
                }
                let (mut node, c) = rest.remove(id);
                // Choosing an optional ability is accepting it
                if let Value::Object(m) = &mut node {
                    m.remove("optional");
                }
                self.stack.push(if spell { Task::SpellEffects(rest) } else { Task::Reactions(rest, auto) });
                self.stack.push(Task::Run { node, ctx: c });
                Ok(())
            }
            (PendingKind::Select { cards, min, max, costs, then, .. }, Answer::Select { card_ids }) => {
                let mut ids = card_ids.clone();
                ids.dedup();
                if ids.len() < min || ids.len() > max || ids.iter().any(|c| !cards.contains(c)) {
                    return err(format!("Select between {} and {} of the proposed cards", min, max));
                }
                match then {
                    SelectThen::SetupMana => {
                        self.setup_mana[seat] = Some(ids.clone());
                        if self.setup_mana.iter().all(|m| m.is_some()) {
                            for s in 0..2 {
                                let ids = self.setup_mana[s].take().unwrap_or_default();
                                for &c in &ids {
                                    self.move_card(c, Zone::Mana, None);
                                }
                                let name = self.players[s].name.clone();
                                self.log_line(Some(s), format!("{} puts {} cards in Mana", name, ids.len()));
                            }
                        }
                    }
                    SelectThen::Effect { effect, limits } => {
                        let (mut cost, mut mountain) = (0, 0);
                        for &c in &ids {
                            cost += self.card(c).def().cost_hand;
                            mountain += self.card(c).stats()[1];
                        }
                        if limits.0.map(|l| cost > l).unwrap_or(false) {
                            return err("Total hand cost exceeds the limit of the effect");
                        }
                        if limits.1.map(|l| mountain > l).unwrap_or(false) {
                            return err("Total mountain exceeds the limit of the effect");
                        }
                        let extra: i32 = ids.iter().map(|c| costs.iter().find(|(x, _)| x == c).map(|(_, t)| *t).unwrap_or(0)).sum();
                        if extra > 0 && !self.pay(seat, extra) {
                            return err("You can't pay the extra cost of Tough");
                        }
                        apply_targets(self, &ids, effect.as_ref(), &ctx);
                    }
                    SelectThen::Move { to, exhausted, sacrifice } => {
                        for &c in &ids {
                            move_with_events(self, c, to, sacrifice, exhausted, &ctx);
                        }
                    }
                }
                Ok(())
            }
            (PendingKind::Expedition { options, min, max, effect }, Answer::Expedition { keys }) => {
                if keys.len() < min || keys.len() > max {
                    return err("Wrong number of expeditions");
                }
                let mut chosen = vec![];
                for k in &keys {
                    let Some(&(s, side)) = options.iter().find(|(s, side)| expedition_key(*s, *side) == *k) else { return err("Unknown expedition") };
                    chosen.push((s, side));
                }
                if let Some(e) = effect {
                    for (s, side) in chosen.into_iter().rev() {
                        let mut c = ctx.clone();
                        c.target_seat = Some(s);
                        c.side = Some(side);
                        c.side_seat = Some(s);
                        self.stack.push(Task::Run { node: e.clone(), ctx: c });
                    }
                }
                Ok(())
            }
            (PendingKind::Player { options, effect }, Answer::Player { player }) => {
                let Some(s) = self.seat_of(player).filter(|s| options.contains(s)) else { return err("Unknown player") };
                if let Some(e) = effect {
                    let mut c = ctx.clone();
                    c.target_seat = Some(s);
                    self.stack.push(Task::Run { node: e, ctx: c });
                }
                Ok(())
            }
            (PendingKind::Number { values, then }, Answer::Number { value }) => {
                if !values.contains(&value) {
                    return err("Unknown value");
                }
                match then {
                    NumberThen::Pay => {
                        self.pay(seat, value);
                        self.mods.last_paid = value;
                    }
                    NumberThen::Die { args, .. } => resolve_die(self, args.as_object().unwrap_or(&Default::default()), ctx, value, values.len() > 1),
                }
                Ok(())
            }
            // All In!: a Reserve card discarded instead of choosing a result
            (PendingKind::Number { then: NumberThen::Die { args, discard }, .. }, Answer::Select { card_ids }) => {
                let [c] = card_ids[..] else { return err("Select one card of your Reserve") };
                if !discard.contains(&c) {
                    return err("Unknown card");
                }
                discard_add(self, args.as_object().unwrap_or(&Default::default()), ctx, c);
                Ok(())
            }
            (PendingKind::Location { options, token, n }, Answer::Location { key }) => {
                let Some((_, _, s, zone)) = options.iter().find(|o| o.0 == key).cloned() else { return err("Unknown location") };
                let def = crate::catalog::catalog().get(&token).map(|d| d.index).ok_or(EngineError("Unknown token".into()))?;
                for _ in 0..n {
                    create_token(self, def, s, zone, &ctx);
                }
                Ok(())
            }
            (PendingKind::Cleanup { reserve, n_reserve, landmarks, n_landmarks }, Answer::Cleanup { reserve: r, landmarks: l }) => {
                if r.len() != n_reserve || l.len() != n_landmarks || r.iter().any(|c| !reserve.contains(c)) || l.iter().any(|c| !landmarks.contains(c)) {
                    return err(format!("Discard exactly {} Reserve card(s) and {} Landmark(s)", n_reserve, n_landmarks));
                }
                // Excess Landmarks are sacrificed; reactions wait for the end of the Night
                let before = self.reactions.len();
                for &c in r.iter() {
                    move_with_events(self, c, Zone::Discard, false, false, &ctx);
                }
                for &c in l.iter() {
                    move_with_events(self, c, Zone::Discard, true, false, &ctx);
                }
                let fired: Vec<_> = self.reactions.drain(before..).collect();
                self.after_night.extend(fired);
                Ok(())
            }
            (PendingKind::Exchange { hand, reserve }, Answer::Exchange { hand: h, reserve: r }) => {
                if !hand.contains(&h) || !reserve.contains(&r) {
                    return err("Unknown cards");
                }
                self.move_card(r, Zone::Hand, None);
                self.move_card(h, Zone::Reserve, None);
                Ok(())
            }
            (PendingKind::Buttons { buttons }, Answer::Button { key }) => {
                if !buttons.iter().any(|(k, _)| *k == key) {
                    return err("Unknown button");
                }
                let (biome, region) = key.split_once(':').unwrap_or(("", ""));
                let (Some(b), Ok(r)) = (Biome::from_engine(biome), region.parse::<i32>()) else { return err("Bad button") };
                self.players[seat].records.region_markers.push((r, b));
                self.players[seat].records.markers_used += 1;
                let name = self.players[seat].name.clone();
                self.log_line(Some(seat), format!("{} places a {} marker on region {}", name, biome, r));
                Ok(())
            }
            (_, Answer::Confirm) => Ok(()),
            (kind, a) => err(format!("Answer {:?} does not fit this decision ({})", a, kind_name(&kind))),
        }
    }
}

pub fn kind_name(k: &PendingKind) -> &'static str {
    match k {
        PendingKind::Turn { .. } => "turn",
        PendingKind::Choice { .. } | PendingKind::Order { .. } => "choice",
        PendingKind::Select { .. } => "select",
        PendingKind::Expedition { .. } => "expedition",
        PendingKind::Player { .. } => "player",
        PendingKind::Number { .. } => "number",
        PendingKind::Location { .. } => "location",
        PendingKind::Cleanup { .. } => "cleanup",
        PendingKind::Exchange { .. } => "exchange",
        PendingKind::Buttons { .. } => "buttons",
    }
}
