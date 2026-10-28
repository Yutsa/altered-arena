//! A simple opponent: it only answers with what the decision allows, so it can't break a rule.

use crate::answer::{expedition_key, Answer};
use crate::catalog::CardType;
use crate::game::*;
use crate::model::*;

impl Game {
    pub fn bot_answer(&mut self, seat: Seat) -> Option<Answer> {
        let p = self.pending_for(seat)?.clone();
        Some(match &p.kind {
            PendingKind::Turn { play, can_end, .. } => {
                let mut best: Option<(f32, CardId, Dest)> = None;
                for (id, dests) in play {
                    let c = self.card(*id);
                    let def = c.def();
                    let cost = self.play_cost(seat, *id, false) as f32;
                    let stats: i32 = def.stats.iter().sum();
                    let base = if stats > 0 { stats as f32 } else { 3.5 } - 0.6 * cost + self.rng.below(100) as f32 / 100.0;
                    for &d in dests {
                        let mut v = base;
                        if d.is_scout() {
                            v -= 1.5;
                        }
                        if def.card_type == CardType::Character {
                            if let Some(side) = d.zone().side() {
                                v += self.side_need(seat, side, def.stats);
                            }
                        }
                        if best.map(|b| v > b.0).unwrap_or(true) {
                            best = Some((v, *id, d));
                        }
                    }
                }
                match best {
                    Some((v, id, d)) if v > -2.0 => Answer::Play { card_id: id, destination: d },
                    _ if *can_end => Answer::EndTurn,
                    _ => Answer::Pass,
                }
            }
            PendingKind::Order { .. } => Answer::Choice { id: 0 },
            PendingKind::Choice { options } => {
                let enabled: Vec<usize> = options.iter().enumerate().filter(|(_, o)| o.enabled && o.node.is_some()).map(|(i, _)| i).collect();
                match enabled.first() {
                    Some(_) => Answer::Choice { id: enabled[self.rng.below(enabled.len())] },
                    None => Answer::Choice { id: options.iter().position(|o| o.enabled).unwrap_or(0) },
                }
            }
            PendingKind::Select { cards, min, max, then, costs, .. } => {
                let mut ids = cards.clone();
                let cost = |g: &Game, c: CardId| g.card(c).def().cost_hand;
                match then {
                    SelectThen::SetupMana => ids.sort_by_key(|&c| -cost(self, c)),
                    SelectThen::Move { to: Zone::Mana, .. } => {
                        if self.rng.below(10) < 2 && p.can_pass {
                            return Some(Answer::Pass);
                        }
                        ids.sort_by_key(|&c| -cost(self, c));
                    }
                    SelectThen::Move { .. } => ids.sort_by_key(|&c| cost(self, c)),
                    SelectThen::Effect { .. } => {
                        let opp_first = self.rng.below(2) == 0;
                        ids.sort_by_key(|&c| (self.card(c).controller == seat) == opp_first);
                        let affordable = self.mana_available(seat);
                        ids.retain(|c| costs.iter().find(|(x, _)| x == c).map(|(_, t)| *t <= affordable).unwrap_or(true));
                    }
                }
                let n = (*max).min(ids.len()).max((*min).min(ids.len()));
                if n < *min {
                    return Some(Answer::Pass);
                }
                Answer::Select { card_ids: ids.into_iter().take(n).collect() }
            }
            PendingKind::Expedition { options, min, .. } => {
                let mut opts = options.clone();
                opts.sort_by_key(|(s, _)| *s != seat);
                Answer::Expedition { keys: opts.iter().take((*min).max(1)).map(|(s, side)| expedition_key(*s, *side)).collect() }
            }
            PendingKind::Player { options, .. } => {
                let target = options.iter().copied().find(|&s| s != seat).unwrap_or(options[0]);
                Answer::Player { player: self.players[target].id }
            }
            PendingKind::Number { values, .. } => Answer::Number { value: *values.iter().max().unwrap_or(&0) },
            PendingKind::Location { options, .. } => {
                let own = options.iter().find(|o| o.2 == seat).unwrap_or(&options[0]);
                Answer::Location { key: own.0.clone() }
            }
            PendingKind::Cleanup { reserve, n_reserve, landmarks, n_landmarks } => {
                let mut r = reserve.clone();
                r.sort_by_key(|&c| self.card(c).def().cost_reserve);
                let mut l = landmarks.clone();
                l.sort_by_key(|&c| self.card(c).def().cost_hand);
                Answer::Cleanup { reserve: r.into_iter().take(*n_reserve).collect(), landmarks: l.into_iter().take(*n_landmarks).collect() }
            }
            PendingKind::Exchange { hand, reserve } => {
                if p.can_pass {
                    Answer::Pass
                } else {
                    Answer::Exchange { hand: hand[0], reserve: reserve[0] }
                }
            }
        })
    }

    /// How much a character's stats would help an expedition win its current region
    fn side_need(&self, seat: Seat, side: Side, stats: [i32; 3]) -> f32 {
        let mine = self.expedition_stats(seat, side);
        let theirs = self.expedition_stats(Game::other(seat), side);
        let region = self.region_biomes(self.players[seat].position[side.index()]);
        let mut gain = 0.0;
        for b in region {
            let i = b.index();
            let before = mine[i] > theirs[i];
            let after = mine[i] + stats[i] > theirs[i];
            if after && !before {
                gain += 1.5;
            }
        }
        gain
    }

    /// Lets bot players answer until a human must decide (or the game ends)
    pub fn run_bots(&mut self) -> usize {
        let mut n = 0;
        for _ in 0..2000 {
            let Some(seat) = self.pending.iter().map(|p| p.seat).find(|&s| self.players[s].bot) else { break };
            let answer = self.bot_answer(seat).unwrap_or(Answer::Pass);
            if self.answer(seat, answer.clone()).is_err() {
                // Fallbacks: pass, end the turn, or the first option
                let fallbacks = [Answer::Pass, Answer::EndTurn, Answer::Choice { id: 0 }];
                if !fallbacks.into_iter().any(|a| self.answer(seat, a).is_ok()) {
                    self.log_line(Some(seat), format!("The bot is stuck on: {}", self.pending_for(seat).map(|p| p.prompt.clone()).unwrap_or_default()));
                    break;
                }
            }
            n += 1;
        }
        n
    }
}
