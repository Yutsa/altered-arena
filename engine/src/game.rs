//! The game: state, the resolution loop and the structure of a Day.
//!
//! Work to do is a stack of tasks: game steps (Morning, Noon, Afternoon turns, Dusk, Night...)
//! and effect nodes (card effects, as flow trees). The loop pops tasks until a player has to
//! decide something (`pending`), or the game ends. Card reactions to events are queued and run
//! once the effect that caused them is fully resolved, before the next game step.

use crate::catalog::{catalog, CardType};
use crate::model::*;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Context of an effect being resolved
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ctx {
    /// Player who controls the effect
    pub seat: Seat,
    /// Card whose ability it is
    pub source: Option<CardId>,
    /// Card chosen by an enclosing Target (the "EFFECT" card)
    pub target: Option<CardId>,
    pub target_from: Option<Zone>,
    /// Player chosen by TargetPlayer / expedition chosen by TargetExpedition
    pub target_seat: Option<Seat>,
    pub side: Option<Side>,
    /// Owner of the expedition chosen by TargetExpedition (kept when a TargetPlayer follows)
    #[serde(default)]
    pub side_seat: Option<Seat>,
    /// Event that triggered the ability
    pub event: Option<Box<Event>>,
}

impl Ctx {
    pub fn of(seat: Seat, source: Option<CardId>) -> Ctx {
        Ctx { seat, source, ..Default::default() }
    }
}

/// Something that happened, that cards may react to (trigger names of the original engine)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Event {
    pub trigger: String,
    /// Player who did it / whose card it is about
    pub seat: Seat,
    pub card: Option<CardId>,
    pub from: Option<Zone>,
    pub to: Option<Zone>,
    pub sacrifice: bool,
    pub n: i32,
    /// Gain/Loose: what was gained
    pub what: Option<String>,
    /// Cards that react even though they are not in play anymore (the discarded card itself)
    pub listeners: Vec<CardId>,
    pub source: Option<CardId>,
    /// A card entered play (playCard in the original engine); `put` when put there without being played
    pub play: bool,
    pub put: bool,
    /// A card was discarded / moved out of a zone (discardCard)
    pub discard: bool,
    /// Discarded for its support ability
    pub support: bool,
    /// Only the cards in `listeners` react (a card leaving a zone)
    #[serde(default)]
    pub only_listeners: bool,
    /// MoveExpedition: the expedition that moved
    #[serde(default)]
    pub side: Option<Side>,
    /// Leave events: the card was Fleeting / its expedition was ascended / its boosts, before it left
    #[serde(default)]
    pub fleeting: bool,
    #[serde(default)]
    pub ascended: bool,
    #[serde(default)]
    pub boosts: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dest {
    HeroExpedition,
    CompanionExpedition,
    Landmark,
    Reserve,
    Spell,
    HeroExpeditionScout,
    CompanionExpeditionScout,
}

impl Dest {
    pub fn zone(self) -> Zone {
        match self {
            Dest::HeroExpedition | Dest::HeroExpeditionScout => Zone::HeroExpedition,
            Dest::CompanionExpedition | Dest::CompanionExpeditionScout => Zone::CompanionExpedition,
            Dest::Landmark => Zone::Landmark,
            Dest::Reserve => Zone::Reserve,
            Dest::Spell => Zone::Limbo,
        }
    }
    pub fn is_scout(self) -> bool {
        matches!(self, Dest::HeroExpeditionScout | Dest::CompanionExpeditionScout)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChoiceOpt {
    pub label: String,
    /// Effect to run if chosen (None: skip)
    pub node: Option<Value>,
    pub enabled: bool,
    #[serde(default)]
    pub source: Option<CardId>,
}

/// Limits of a play allowed by an effect ("you may play a card for free...")
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BonusPlay {
    pub types: Vec<CardType>,
    pub free: bool,
    pub max_base_cost: Option<i32>,
    pub mandatory: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SelectThen {
    /// Mana at setup
    SetupMana,
    /// Run `effect` once per selected card, with the card as target
    Effect {
        effect: Option<Value>,
        /// Limits on the selected cards' total hand cost and mountain (totalCost, totalMountain)
        #[serde(default)]
        limits: (Option<i32>, Option<i32>),
    },
    /// Move the cards to a zone (Discard action with a source)
    Move { to: Zone, exhausted: bool, sacrifice: bool },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PendingKind {
    Turn { play: Vec<(CardId, Vec<Dest>)>, support: Vec<CardId>, tap: Vec<CardId>, can_end: bool, bonus: Option<BonusPlay> },
    Choice { options: Vec<ChoiceOpt> },
    /// Order of triggered abilities (shown as a choice)
    Order {
        options: Vec<ChoiceOpt>,
        reactions: Vec<(Value, Ctx)>,
        #[serde(default)]
        spell: bool,
        /// Independent abilities resolve without asking (false: the player orders everything)
        #[serde(default)]
        auto: bool,
    },
    Select { cards: Vec<CardId>, min: usize, max: usize, costs: Vec<(CardId, i32)>, then: SelectThen, picker: bool },
    Expedition { options: Vec<(Seat, Side)>, min: usize, max: usize, effect: Option<Value> },
    Player { options: Vec<Seat>, effect: Option<Value> },
    Number { values: Vec<i32>, then: NumberThen },
    Location { options: Vec<(String, String, Seat, Zone)>, token: String, n: i32 },
    Cleanup { reserve: Vec<CardId>, n_reserve: usize, landmarks: Vec<CardId>, n_landmarks: usize },
    Exchange { hand: Vec<CardId>, reserve: Vec<CardId> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NumberThen {
    Pay,
    Die { effects: Value, bonus: i32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    /// Unique per decision (answers to a former decision are refused)
    pub id: u64,
    pub seat: Seat,
    pub prompt: String,
    pub source: Option<CardId>,
    pub can_pass: bool,
    pub kind: PendingKind,
    pub ctx: Ctx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    SetupMana,
    Morning,
    Expand(Seat),
    Noon,
    Afternoon,
    /// The active player's action is resolved: next player or Dusk
    AfterAction,
    Dusk,
    AfterDusk,
    Rest,
    BeforeNight,
    NightCleanup(Seat),
    EndOfNight,
    /// Triggers of a phase for every player in turn order (first player first)
    Triggers(TriggerPhase),
    /// Triggers of a phase for one player (all cards may react to it)
    TriggersFor(TriggerPhase, Seat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerPhase {
    Noon,
    Morning,
    BeforeDusk,
    AfterDusk,
    BeforeNight,
    EndOfNight,
}

impl TriggerPhase {
    pub fn name(self) -> &'static str {
        match self {
            TriggerPhase::Noon => "Noon",
            TriggerPhase::Morning => "Morning",
            TriggerPhase::BeforeDusk => "BeforeDusk",
            TriggerPhase::AfterDusk => "AfterDusk",
            TriggerPhase::BeforeNight => "BeforeNight",
            TriggerPhase::EndOfNight => "EndOfNight",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Task {
    Step(Step),
    Run { node: Value, ctx: Ctx },
    /// End of a spell: to Reserve (or discard if Fleeting)
    SpellCleanup { card: CardId },
    /// A play from hand/reserve finished resolving (ends the turn unless it was a bonus play)
    PlayDone { turn_play: bool },
    /// The rest of a sequence after a Rush (ChooseAssignment): only if a card was played
    IfBonusPlayed { node: Value, ctx: Ctx },
    /// Triggered abilities waiting to resolve: the player picks the order
    /// (abilities, whether independent ones resolve first without asking). Abilities triggered
    /// meanwhile join the list (the original engine's "after finishing" parallel node)
    Reactions(Vec<(Value, Ctx)>, bool),
    /// A spell's own effects: abilities they trigger wait until the spell is done
    SpellEffects(Vec<(Value, Ctx)>),
}

/// Per-turn and per-day modifiers set by effects ("the next character you play gains a boost"...)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Modifiers {
    /// (seat, card type or subtype, reduction, minimum cost)
    pub cost_reductions: Vec<(Seat, String, i32)>,
    pub next_character_boost: [i32; 2],
    pub next_reserve_character_boost: [i32; 2],
    pub next_animal_boost: [i32; 2],
    pub next_character_fleeting: [bool; 2],
    pub next_character_anchored: [bool; 2],
    pub next_token_anchored: [bool; 2],
    /// The active player played a card this turn (ends the turn)
    pub played: bool,
    /// A card was played through an effect (Rush...) since the effect asked for it
    #[serde(default)]
    pub bonus_played: bool,
    /// State of the special effects (global Tough, after-Rest effects, next-card modifiers...)
    #[serde(default)]
    pub special: crate::special::SpecialMods,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub cards: Vec<Card>,
    pub players: [Player; 2],
    pub storm: Vec<StormCard>,
    pub day: u32,
    pub phase: Phase,
    pub first: Seat,
    pub active: Seat,
    pub winner: Option<Seat>,
    pub tiebreaker: bool,
    pub rng: Rng,
    pub stack: Vec<Task>,
    pub pending: Vec<Pending>,
    pub reactions: Vec<(Value, Ctx)>,
    pub mods: Modifiers,
    pub log: Vec<LogLine>,
    pub seq: u64,
    pub next_decision: u64,
    /// Reactions to cards leaving during the Rest, resolved at the end of the Night
    #[serde(default)]
    pub after_night: Vec<(Value, Ctx)>,
    /// First-day mana choices, applied once both players have chosen
    #[serde(default)]
    pub setup_mana: [Option<Vec<CardId>>; 2],
    /// Effects the engine does not implement yet, met during this game (for coverage reports)
    pub unsupported: Vec<String>,
    /// State at the start of the current decision sequence, for undo (no hidden information revealed since)
    #[serde(skip)]
    pub undo_point: Option<Box<Game>>,
    pub undo_owner: Option<Seat>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerConfig {
    pub id: u32,
    pub name: String,
    pub bot: bool,
    /// Starter deck id; None: chosen in game (not supported yet: a random deck is used)
    pub deck: Option<String>,
}

#[derive(Debug)]
pub struct EngineError(pub String);

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type Res<T> = Result<T, EngineError>;

pub fn err<T>(msg: impl Into<String>) -> Res<T> {
    Err(EngineError(msg.into()))
}

impl Game {
    pub fn new(id: impl Into<String>, players: [PlayerConfig; 2], seed: u64) -> Game {
        let mut rng = Rng::new(seed);
        let cat = catalog();
        let make_player = |p: &PlayerConfig| Player {
            id: p.id,
            name: p.name.clone(),
            bot: p.bot,
            deck_id: p.deck.clone(),
            hero: None,
            hand: vec![],
            deck: vec![],
            reserve: vec![],
            landmarks: vec![],
            expeditions: [vec![], vec![]],
            discard: vec![],
            mana: vec![],
            limbo: vec![],
            position: [0, 7],
            passed: false,
            blocked: [false; 2],
            moved: [false; 2],
            token: String::new(),
            ascended: [false; 2],
            records: Default::default(),
        };
        let mut game = Game {
            id: id.into(),
            cards: vec![],
            players: [make_player(&players[0]), make_player(&players[1])],
            storm: vec![],
            day: 1,
            phase: Phase::Setup,
            first: 0,
            active: 0,
            winner: None,
            tiebreaker: false,
            rng: Rng::new(0),
            stack: vec![],
            pending: vec![],
            reactions: vec![],
            mods: Modifiers::default(),
            log: vec![],
            seq: 0,
            next_decision: 0,
            after_night: vec![],
            setup_mana: [None, None],
            unsupported: vec![],
            undo_point: None,
            undo_owner: None,
        };

        // Decks
        for seat in 0..2 {
            let deck_id = players[seat].deck.clone().unwrap_or_else(|| {
                let i = rng.below(cat.starters.len());
                cat.starters[i].0.clone()
            });
            game.players[seat].deck_id = Some(deck_id.clone());
            let list = cat.starter(&deck_id).cloned().unwrap_or_default();
            for (reference, count) in list {
                let Some(def) = cat.get(&reference) else { continue };
                if def.card_type == CardType::Hero {
                    let id = game.create_card(def.index, seat, Zone::Hero);
                    game.players[seat].hero = Some(id);
                } else {
                    for _ in 0..count {
                        game.create_card(def.index, seat, Zone::Deck);
                    }
                }
            }
            let mut deck = std::mem::take(&mut game.players[seat].deck);
            rng.shuffle(&mut deck);
            game.players[seat].deck = deck;
        }

        // Tumult: hero region, 3 random face-down tumult cards, companion region
        game.storm.push(StormCard { card: 0, rotated: false, revealed: true });
        let mut keys = [1u8, 2, 3];
        rng.shuffle(&mut keys);
        for k in keys {
            let rotated = rng.below(2) == 1;
            game.storm.push(StormCard { card: k, rotated, revealed: false });
        }
        game.storm.push(StormCard { card: 4, rotated: false, revealed: true });

        game.first = rng.below(2);
        game.active = game.first;
        game.rng = rng;
        for seat in 0..2 {
            let hero = game.players[seat].hero.map(|h| game.cards[h as usize].name()).unwrap_or("?");
            game.log_line(Some(seat), format!("{} plays [[{}]]", game.players[seat].name, hero));
        }
        for seat in game.turn_order() {
            game.draw(seat, 6, None);
        }
        game.stack.push(Task::Step(Step::SetupMana));
        game.run();
        game
    }

    // ------------------------------------------------------------------
    // Basic accessors
    // ------------------------------------------------------------------

    pub fn card(&self, id: CardId) -> &Card {
        &self.cards[id as usize]
    }
    pub fn card_mut(&mut self, id: CardId) -> &mut Card {
        &mut self.cards[id as usize]
    }
    pub fn other(seat: Seat) -> Seat {
        1 - seat
    }
    /// First player, then the other
    pub fn turn_order(&self) -> [Seat; 2] {
        [self.first, Game::other(self.first)]
    }
    pub fn seat_of(&self, player_id: u32) -> Option<Seat> {
        self.players.iter().position(|p| p.id == player_id)
    }
    pub fn mana_available(&self, seat: Seat) -> i32 {
        self.players[seat].mana.iter().filter(|&&c| !self.card(c).exhausted).count() as i32
    }
    pub fn log_line(&mut self, seat: Option<Seat>, text: String) {
        self.seq += 1;
        self.log.push(LogLine { seq: self.seq, text, player: seat });
        // Views show the last 100 lines
        if self.log.len() > 120 {
            self.log.drain(0..20);
        }
    }
    pub fn unsupported(&mut self, what: String) {
        if !self.unsupported.contains(&what) {
            self.unsupported.push(what);
        }
    }

    pub fn create_card(&mut self, def: u16, seat: Seat, zone: Zone) -> CardId {
        let id = self.cards.len() as CardId;
        self.cards.push(Card {
            id,
            def,
            owner: seat,
            controller: seat,
            zone,
            exhausted: false,
            boosts: 0,
            statuses: vec![],
            counter: 0,
            stat_mods: [0; 3],
            played_from: None,
            completed: false,
        });
        if zone != Zone::Hero {
            self.players[seat].zone_mut(zone).push(id);
        }
        id
    }

    /// Cards in play (expeditions and landmarks) of a player
    pub fn in_play(&self, seat: Seat) -> Vec<CardId> {
        let p = &self.players[seat];
        p.expeditions[0].iter().chain(p.expeditions[1].iter()).chain(p.landmarks.iter()).copied().collect()
    }

    // ------------------------------------------------------------------
    // Moving cards
    // ------------------------------------------------------------------

    /// Moves a card to a zone of its owner (or of `to_seat`), clearing what it loses on the way.
    /// Tokens that leave play are removed from the game.
    pub fn move_card(&mut self, id: CardId, to: Zone, to_seat: Option<Seat>) {
        let (owner, from) = {
            let c = self.card(id);
            (c.controller, c.zone)
        };
        if from == Zone::Hero {
            return;
        }
        self.players[owner].zone_mut(from).retain(|&x| x != id);
        let seat = to_seat.unwrap_or(self.card(id).owner);
        let seasoned = self.card(id).def().flag("seasoned");
        let leaving_play = from.in_play() || from == Zone::Limbo;
        let is_token = self.card(id).def().is_token;
        // Fleeting: a card in play that would go to Reserve is discarded instead
        let to = if to == Zone::Reserve && leaving_play && self.card(id).has(Status::Fleeting) { Zone::Discard } else { to };
        let to = if is_token && !to.in_play() && to != Zone::Limbo { Zone::Gone } else { to };
        {
            let c = self.card_mut(id);
            c.zone = to;
            c.controller = seat;
            if leaving_play && !to.in_play() {
                if !(seasoned && to == Zone::Reserve) {
                    c.boosts = 0;
                }
                c.statuses.clear();
                c.stat_mods = [0; 3];
                c.played_from = None;
                c.exhausted = false;
            }
            if matches!(to, Zone::Hand | Zone::Deck | Zone::Discard) {
                c.boosts = 0;
                c.statuses.clear();
                c.exhausted = false;
                c.counter = 0;
            }
        }
        if to != Zone::Gone {
            self.players[seat].zone_mut(to).push(id);
        }
        if to == Zone::Discard {
            let p = &mut self.players[seat];
            p.records.discard_peak = p.records.discard_peak.max(p.discard.len() as i32);
        }
    }

    /// Draws n cards (refilling the deck from the discard pile if needed)
    pub fn draw(&mut self, seat: Seat, n: i32, _source: Option<CardId>) -> Vec<CardId> {
        let mut drawn = vec![];
        for _ in 0..n {
            if self.players[seat].deck.is_empty() {
                self.reshuffle_discard(seat);
            }
            let Some(id) = self.players[seat].deck.last().copied() else { break };
            self.move_card(id, Zone::Hand, None);
            drawn.push(id);
        }
        if !drawn.is_empty() {
            self.forget_undo();
            let name = self.players[seat].name.clone();
            self.log_line(Some(seat), format!("{} draws {} card(s)", name, drawn.len()));
            // Plain draws (Morning, setup) trigger nothing; the Draw action emits its event
        }
        drawn
    }

    pub fn reshuffle_discard(&mut self, seat: Seat) {
        let mut pile = std::mem::take(&mut self.players[seat].discard);
        if pile.is_empty() {
            return;
        }
        self.rng.shuffle(&mut pile);
        for &id in &pile {
            self.card_mut(id).zone = Zone::Deck;
        }
        self.players[seat].deck.splice(0..0, pile);
        let name = self.players[seat].name.clone();
        self.log_line(Some(seat), format!("{} shuffles their discard pile into their deck", name));
    }

    /// Exhausts n ready mana orbs; false if not enough
    pub fn pay(&mut self, seat: Seat, n: i32) -> bool {
        if n <= 0 {
            return true;
        }
        if self.mana_available(seat) < n {
            return false;
        }
        let ready: Vec<CardId> = self.players[seat].mana.iter().copied().filter(|&c| !self.card(c).exhausted).collect();
        for id in ready.into_iter().take(n as usize) {
            self.card_mut(id).exhausted = true;
        }
        true
    }

    // ------------------------------------------------------------------
    // Events and reactions
    // ------------------------------------------------------------------

    /// Queues the reactions of every listening card to `event`
    /// A card left a zone: the card itself reacts to "Leave<Zone>", every other card to
    /// "OtherLeave<Zone>" (Card::checkLeaveListener). At night, the reactions wait for the end of
    /// the Night (AfterNightCleanup in the original engine).
    pub fn emit_leave(&mut self, mut ev: Event, night: bool) {
        let from = ev.from.unwrap_or(Zone::Gone);
        let kind = if from.is_expedition() {
            "LeaveExpedition"
        } else if from == Zone::Landmark {
            "LeaveLandmark"
        } else {
            "LeaveOther"
        };
        if let Some(c) = ev.card {
            ev.seat = self.card(c).controller;
        }
        let before = self.reactions.len();
        self.emit(Event { trigger: kind.into(), only_listeners: true, ..ev.clone() });
        self.emit(Event { trigger: format!("Other{}", kind), listeners: vec![], only_listeners: false, ..ev });
        if night {
            let fired: Vec<(Value, Ctx)> = self.reactions.drain(before..).collect();
            self.after_night.extend(fired);
        }
    }

    pub fn emit(&mut self, event: Event) {
        let mut listeners: Vec<CardId> = vec![];
        for seat in self.turn_order() {
            if event.only_listeners {
                break;
            }
            let p = &self.players[seat];
            if let Some(h) = p.hero {
                listeners.push(h);
            }
            listeners.extend(self.in_play(seat));
            listeners.extend(p.reserve.iter().copied());
        }
        for &l in &event.listeners {
            if !listeners.contains(&l) {
                listeners.push(l);
            }
        }
        let mut queued = vec![];
        for id in listeners {
            let card = self.card(id);
            let in_reserve = card.zone == Zone::Reserve && !event.listeners.contains(&id);
            let passive = if in_reserve {
                card.def().prop("effectInfinity").and_then(|v| v.get("effectPassive"))
            } else {
                card.def().prop("effectPassive")
            };
            let Some(Value::Object(passive)) = passive else { continue };
            let Some(power) = passive.get(&event.trigger) else { continue };
            let powers: Vec<&Value> = match power.get("childs").and_then(Value::as_array) {
                Some(list) => list.iter().collect(),
                None => vec![power],
            };
            for power in powers {
                let ctx = Ctx { seat: card.controller, source: Some(id), event: Some(Box::new(event.clone())), ..Default::default() };
                let react = |g: &Game, ctx: &Ctx| {
                    if crate::conditions::check_power(g, power, ctx) {
                        power.get("output").cloned()
                    } else {
                        power.get("oppositeOutput").filter(|v| v.as_str() != Some("OPPOSITE")).cloned()
                    }
                };
                // "n": "eachExpedition": checked once per expedition, outputs in sequence
                let node = if power.get("n").and_then(Value::as_str) == Some("eachExpedition") {
                    let outputs: Vec<Value> = Side::BOTH
                        .iter()
                        .filter_map(|&side| {
                            let mut c = ctx.clone();
                            if let Some(e) = c.event.as_mut() {
                                e.side = Some(side);
                            }
                            react(self, &c)
                        })
                        .collect();
                    if outputs.is_empty() {
                        None
                    } else {
                        Some(serde_json::json!({"type": "seq", "childs": outputs}))
                    }
                } else {
                    react(self, &ctx)
                };
                if let Some(mut node) = node {
                    if let (Some(pid), Value::Object(m)) = (power.get("pId").cloned(), &mut node) {
                        m.insert("pId".into(), pid);
                    }
                    queued.push((node, ctx));
                }
            }
        }
        self.reactions.extend(queued);
    }

    // ------------------------------------------------------------------
    // Resolution loop
    // ------------------------------------------------------------------

    pub fn run(&mut self) {
        for _ in 0..200_000 {
            if self.phase == Phase::Ended || !self.pending.is_empty() {
                return;
            }
            let at_step = matches!(self.stack.last(), None | Some(Task::Step(_)) | Some(Task::PlayDone { .. }));
            if at_step && !self.reactions.is_empty() {
                let reactions = std::mem::take(&mut self.reactions);
                self.stack.push(Task::Reactions(reactions, true));
                continue;
            }
            let Some(task) = self.stack.pop() else { return };
            for u in crate::conditions::take_unknown() {
                self.unsupported(u);
            }
            match task {
                Task::Step(step) => self.step(step),
                Task::Run { node, ctx } => crate::flow::run_node(self, &node, ctx),
                Task::SpellCleanup { card } => self.spell_cleanup(card),
                Task::Reactions(mut list, auto) => {
                    list.extend(self.reactions.drain(..));
                    self.order_reactions(list, auto)
                }
                Task::SpellEffects(list) => self.order_reactions_ex(list, true, true),
                Task::IfBonusPlayed { node, ctx } => {
                    if self.mods.bonus_played {
                        crate::flow::run_node(self, &node, ctx);
                    }
                }
                Task::PlayDone { turn_play } => {
                    if turn_play {
                        self.mods.played = true;
                    }
                }
            }
        }
        self.log_line(None, "The engine stopped: too many steps".into());
        self.phase = Phase::Ended;
    }

    /// Several abilities triggered at once: the player chooses which resolves first (as in the
    /// original engine, the choice is asked even when the order doesn't matter)
    fn order_reactions(&mut self, list: Vec<(Value, Ctx)>, auto_independent: bool) {
        self.order_reactions_ex(list, auto_independent, false)
    }

    fn order_reactions_ex(&mut self, mut list: Vec<(Value, Ctx)>, auto_independent: bool, spell: bool) {
        let again = |rest: Vec<(Value, Ctx)>| if spell { Task::SpellEffects(rest) } else { Task::Reactions(rest, auto_independent) };
        if list.len() <= 1 {
            if let Some((node, ctx)) = list.pop() {
                self.stack.push(Task::Run { node, ctx });
            }
            return;
        }
        if auto_independent {
            if let Some(i) = list.iter().position(|(n, c)| crate::flow::is_independent(self, n, c)) {
                let (node, ctx) = list.remove(i);
                self.stack.push(again(list));
                self.stack.push(Task::Run { node, ctx });
                return;
            }
        }
        let seats: Vec<Seat> = list.iter().map(|(_, c)| c.seat).collect();
        let chooser = if seats.iter().all(|&s| s == seats[0]) { seats[0] } else { self.active };
        let options = list
            .iter()
            .map(|(node, ctx)| ChoiceOpt {
                label: format!("{}{}", crate::flow::source_prefix(self, ctx.source), crate::flow::describe(self, node, ctx)),
                node: None,
                enabled: true,
                source: ctx.source,
            })
            .collect();
        // When every remaining ability is optional, the player may skip them all
        let all_optional = list.iter().all(|(n, _)| n.get("optional").and_then(Value::as_bool).unwrap_or(false));
        self.ask(Pending {
            id: 0,
            seat: chooser,
            prompt: "Choose which effect to resolve first".into(),
            source: None,
            can_pass: all_optional,
            kind: PendingKind::Order { options, reactions: list, spell, auto: auto_independent },
            ctx: Ctx::of(chooser, None),
        });
    }

    pub fn ask(&mut self, mut p: Pending) {
        self.next_decision += 1;
        p.id = self.next_decision;
        self.pending.push(p);
    }

    fn spell_cleanup(&mut self, id: CardId) {
        if self.card(id).zone != Zone::Limbo {
            return;
        }
        let fleeting = self.card(id).has(Status::Fleeting);
        let cooldown = self.card(id).def().flag("cooldown");
        if fleeting {
            self.move_card(id, Zone::Discard, None);
        } else {
            self.move_card(id, Zone::Reserve, None);
            if cooldown {
                self.card_mut(id).exhausted = true;
            }
        }
    }

    // ------------------------------------------------------------------
    // Structure of a Day
    // ------------------------------------------------------------------

    fn step(&mut self, step: Step) {
        match step {
            Step::SetupMana => {
                for seat in 0..2 {
                    let cards = self.players[seat].hand.clone();
                    self.ask(Pending {
                        id: 0,
                        seat,
                        prompt: "Choose 3 cards to put in your Mana".into(),
                        source: None,
                        can_pass: false,
                        kind: PendingKind::Select { cards, min: 3, max: 3, costs: vec![], then: SelectThen::SetupMana, picker: false },
                        ctx: Ctx::of(seat, None),
                    });
                }
                // Day 1 starts at Noon
                self.stack.push(Task::Step(Step::Noon));
            }
            Step::Morning => {
                if self.check_victory() {
                    return;
                }
                self.day += 1;
                self.phase = Phase::Morning;
                crate::special::on_morning(self);
                self.log_line(None, format!("Day {} — Morning", self.day));
                for c in self.cards.iter_mut() {
                    c.exhausted = false;
                }
                for p in self.players.iter_mut() {
                    p.passed = false;
                    p.blocked = [false; 2];
                    p.moved = [false; 2];
                    p.records.storm_moves = Default::default();
                    p.records.passed_first = false;
                }
                self.first = Game::other(self.first);
                self.active = self.first;
                let name = self.players[self.first].name.clone();
                self.log_line(Some(self.first), format!("{} is the first player", name));
                let mut expands = vec![];
                for seat in self.turn_order() {
                    let hero = self.players[seat].hero;
                    // Fen & Crowbar: one card drawn, one card put in Mana from the deck, no Expand
                    if hero.map(|h| self.card(h).def().prop("uid").and_then(Value::as_str) == Some("ALT_CORE_B_LY_03_C")).unwrap_or(false) {
                        self.draw(seat, 1, hero);
                        if self.players[seat].deck.is_empty() {
                            self.reshuffle_discard(seat);
                        }
                        if let Some(&c) = self.players[seat].deck.last() {
                            self.move_card(c, Zone::Mana, None);
                        }
                    } else {
                        self.draw(seat, 2, hero);
                        expands.push(seat);
                    }
                }
                self.stack.push(Task::Step(Step::Noon));
                for &seat in expands.iter().rev() {
                    self.stack.push(Task::Step(Step::Expand(seat)));
                }
                self.stack.push(Task::Step(Step::Triggers(TriggerPhase::Morning)));
            }
            Step::Expand(seat) => {
                let hand = self.players[seat].hand.clone();
                if !hand.is_empty() {
                    self.ask(Pending {
                        id: 0,
                        seat,
                        prompt: "Expand: you may put a card from your hand in your Mana".into(),
                        source: None,
                        can_pass: true,
                        kind: PendingKind::Select {
                            cards: hand,
                            min: 1,
                            max: 1,
                            costs: vec![],
                            then: SelectThen::Move { to: Zone::Mana, exhausted: false, sacrifice: false },
                            picker: false,
                        },
                        ctx: Ctx::of(seat, None),
                    });
                }
            }
            Step::Noon => {
                self.phase = Phase::Noon;
                self.stack.push(Task::Step(Step::Afternoon));
                self.stack.push(Task::Step(Step::Triggers(TriggerPhase::Noon)));
            }
            Step::Triggers(phase) => {
                let order = self.turn_order();
                self.stack.push(Task::Step(Step::TriggersFor(phase, order[1])));
                self.stack.push(Task::Step(Step::TriggersFor(phase, order[0])));
            }
            Step::TriggersFor(phase, seat) => {
                let before = self.reactions.len();
                self.emit(Event { trigger: phase.name().into(), seat, ..Default::default() });
                if phase == TriggerPhase::BeforeNight {
                    // Effects delayed "after Rest" (AfterRestSabotage...)
                    let delayed = std::mem::take(&mut self.mods.special.after_rest[seat]);
                    self.reactions.extend(delayed);
                }
                if phase == TriggerPhase::EndOfNight {
                    // Reactions to cards that left during the Rest
                    let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut self.after_night).into_iter().partition(|(_, c)| c.seat == seat);
                    self.after_night = others;
                    self.reactions.extend(mine);
                }
                // Several abilities on the same phase: the player orders them all (noIndependent)
                let fired: Vec<(Value, Ctx)> = self.reactions.drain(before..).collect();
                let auto = fired.len() <= 1;
                self.stack.push(Task::Reactions(fired, auto));
            }
            Step::Afternoon => {
                if self.phase != Phase::Afternoon {
                    self.phase = Phase::Afternoon;
                    self.active = self.first;
                    self.log_line(None, format!("Day {} — Afternoon", self.day));
                }
                self.start_turn();
            }
            Step::AfterAction => {
                if !self.mods.played && !self.players[self.active].passed {
                    // Quick actions (support, exhaust abilities) don't end the turn
                    self.stack.push(Task::Step(Step::AfterAction));
                    self.ask_turn(self.active, None);
                    return;
                }
                self.mods.played = false;
                let next = Game::other(self.active);
                if self.players[0].passed && self.players[1].passed {
                    self.stack.push(Task::Step(Step::Dusk));
                    self.stack.push(Task::Step(Step::Triggers(TriggerPhase::BeforeDusk)));
                    return;
                }
                if !self.players[next].passed {
                    self.active = next;
                }
                self.start_turn();
            }
            Step::Dusk => {
                self.phase = Phase::Dusk;
                self.log_line(None, format!("Day {} — Dusk", self.day));
                // Only Dusk moves count after Dusk (stPreBeforeDusk resets stormMoves)
                for p in self.players.iter_mut() {
                    p.records.storm_moves = Default::default();
                }
                self.dusk();
                self.stack.push(Task::Step(Step::AfterDusk));
            }
            Step::AfterDusk => {
                self.stack.push(Task::Step(Step::Rest));
                self.stack.push(Task::Step(Step::Triggers(TriggerPhase::AfterDusk)));
            }
            Step::Rest => {
                self.phase = Phase::Night;
                self.log_line(None, format!("Day {} — Night", self.day));
                for seat in self.turn_order() {
                    self.rest(seat);
                }
                // Ascension lasts until Rest
                for p in self.players.iter_mut() {
                    p.ascended = [false; 2];
                }
                let order = self.turn_order();
                self.stack.push(Task::Step(Step::EndOfNight));
                self.stack.push(Task::Step(Step::NightCleanup(order[1])));
                self.stack.push(Task::Step(Step::NightCleanup(order[0])));
                self.stack.push(Task::Step(Step::Triggers(TriggerPhase::BeforeNight)));
            }
            Step::BeforeNight => {}
            Step::NightCleanup(seat) => self.night_cleanup(seat),
            Step::EndOfNight => {
                self.stack.push(Task::Step(Step::Morning));
                self.stack.push(Task::Step(Step::Triggers(TriggerPhase::EndOfNight)));
            }
        }
    }

    fn start_turn(&mut self) {
        self.mods.played = false;
        let seat = self.active;
        self.players[seat].records.abilities.clear();
        self.players[seat].records.smoke_armed.clear();
        crate::special::on_turn_start(self);
        self.stack.push(Task::Step(Step::AfterAction));
        self.ask_turn(seat, None);
        // Bots never undo
        if !self.players[seat].bot {
            self.set_undo_point(seat);
        } else {
            self.forget_undo();
        }
    }

    /// Asks a player for their Afternoon action (or a bonus play allowed by an effect)
    pub fn ask_turn(&mut self, seat: Seat, bonus: Option<BonusPlay>) {
        let play = self.playable(seat, bonus.as_ref());
        let (support, tap) = if bonus.is_some() { (vec![], vec![]) } else { (self.support_options(seat), self.tap_options(seat)) };
        let can_pass = bonus.as_ref().map(|b| !b.mandatory || play.is_empty()).unwrap_or(false);
        if bonus.is_some() && play.is_empty() {
            return;
        }
        let prompt = if bonus.is_some() { "You may play a card".to_string() } else { "Play a card, use an ability, or pass".to_string() };
        self.ask(Pending {
            id: 0,
            seat,
            prompt,
            source: None,
            can_pass,
            kind: PendingKind::Turn { play, support, tap, can_end: bonus.is_none(), bonus },
            ctx: Ctx::of(seat, None),
        });
    }

    // ------------------------------------------------------------------
    // Dusk, Night, victory
    // ------------------------------------------------------------------

    /// Biome totals of an expedition (asleep characters don't count; gigantic ones count in both)
    pub fn expedition_stats(&self, seat: Seat, side: Side) -> [i32; 3] {
        let p = &self.players[seat];
        let mut total = [0; 3];
        let mut ids: Vec<CardId> = p.expeditions[side.index()].clone();
        for &id in &p.expeditions[side.other().index()] {
            if self.card(id).is_gigantic() {
                ids.push(id);
            }
        }
        for id in ids {
            let c = self.card(id);
            if c.has(Status::Asleep) || !c.is_character_like() {
                continue;
            }
            let s = c.stats();
            for i in 0..3 {
                total[i] += s[i];
            }
        }
        total
    }

    /// Biomes of the region an expedition stands in
    pub fn region_biomes(&self, position: i32) -> Vec<Biome> {
        if position <= 0 || position >= 7 {
            return Biome::ALL.to_vec();
        }
        let card = &self.storm[((position + 1) / 2) as usize];
        let mut sides = STORM_CARDS[card.card as usize];
        if card.rotated {
            sides.reverse();
        }
        sides[((position - 1) % 2) as usize].to_vec()
    }

    fn dusk(&mut self) {
        if self.tiebreaker {
            self.dusk_tiebreaker();
            return;
        }
        let mut moves: Vec<(Seat, Side, Vec<Biome>, bool)> = vec![];
        for side in Side::BOTH {
            let stats = [self.expedition_stats(0, side), self.expedition_stats(1, side)];
            for seat in 0..2 {
                if self.players[seat].blocked[side.index()] || self.defended(seat, side) {
                    continue;
                }
                let biomes = self.region_biomes(self.players[seat].position[side.index()]);
                // An ascended expedition also wins ties of 1 or more
                let ascended = self.players[seat].ascended[side.index()];
                let won: Vec<Biome> = biomes
                    .iter()
                    .copied()
                    .filter(|b| {
                        let i = b.index();
                        stats[seat][i] > stats[1 - seat][i] || (ascended && stats[seat][i] >= 1 && stats[seat][i] == stats[1 - seat][i])
                    })
                    .collect();
                let from_ascension = ascended && biomes.iter().any(|b| stats[seat][b.index()] >= 1 && stats[seat][b.index()] == stats[1 - seat][b.index()]);
                if !won.is_empty() {
                    moves.push((seat, side, won, from_ascension));
                }
            }
        }
        for (seat, side, biomes, from_ascension) in moves {
            if self.move_expedition_ex(seat, side, 1, false) {
                if let Some(m) = self.players[seat].records.storm_moves[side.index()].as_mut() {
                    m.biomes = biomes;
                    m.from_ascension = from_ascension;
                }
            }
        }
    }

    /// An opposing Defender in the facing expedition keeps an expedition from moving forward
    fn defended(&self, seat: Seat, side: Side) -> bool {
        let opp = Game::other(seat);
        self.players[opp].expeditions[side.index()]
            .iter()
            .any(|&c| self.card(c).def().flag("defender") && !self.card(c).has(Status::Asleep))
    }

    /// Moves an expedition by an effect (cards react to it)
    pub fn move_expedition(&mut self, seat: Seat, side: Side, n: i32) -> bool {
        self.move_expedition_ex(seat, side, n, true)
    }

    /// `notify`: false for the Dusk advance, which the original engine doesn't announce to listeners
    pub fn move_expedition_ex(&mut self, seat: Seat, side: Side, n: i32, notify: bool) -> bool {
        let pos = self.players[seat].position[side.index()];
        let delta = if side == Side::Hero { n } else { -n };
        let new = (pos + delta).clamp(0, 7);
        if new == pos {
            return false;
        }
        self.players[seat].position[side.index()] = new;
        if n > 0 {
            self.players[seat].moved[side.index()] = true;
        }
        let ascended = self.players[seat].ascended[side.index()];
        self.players[seat].records.storm_moves[side.index()] = Some(StormMove { moves: n, ascended, ..Default::default() });
        let tile = ((new + 1) / 2) as usize;
        if !self.storm[tile].revealed {
            self.storm[tile].revealed = true;
            self.forget_undo();
        }
        let name = self.players[seat].name.clone();
        let label = if side == Side::Hero { "hero" } else { "companion" };
        self.log_line(Some(seat), format!("{}'s {} expedition moves {} to region {}", name, label, if n > 0 { "forward" } else { "back" }, new));
        if notify {
            self.emit(Event { trigger: "MoveExpedition".into(), seat, n, side: Some(side), ascended, ..Default::default() });
        }
        true
    }

    fn dusk_tiebreaker(&mut self) {
        let mut wins = [0; 2];
        for b in 0..3 {
            let t: Vec<i32> = (0..2)
                .map(|s| self.expedition_stats(s, Side::Hero)[b] + self.expedition_stats(s, Side::Companion)[b])
                .collect();
            if t[0] > t[1] {
                wins[0] += 1;
            } else if t[1] > t[0] {
                wins[1] += 1;
            }
        }
        if wins[0] != wins[1] {
            let w = if wins[0] > wins[1] { 0 } else { 1 };
            self.end_game(w);
        } else {
            self.log_line(None, "No winner: a new tiebreaker round starts".into());
        }
    }

    /// Night: characters in expeditions go to Reserve (Fleeting ones are discarded) unless Anchored or Asleep
    fn rest(&mut self, seat: Seat) {
        let cards: Vec<CardId> = self.players[seat].expeditions.iter().flatten().copied().collect();
        for id in cards {
            let c = self.card(id);
            let def = c.def();
            let anchored = c.has(Status::Anchored) || c.has(Status::Asleep);
            let eternal = def.flag("eternal");
            if def.card_type == CardType::Permanent && def.has_subtype("expedition") {
                let side = c.zone.side().unwrap();
                if !self.players[seat].moved[side.index()] {
                    continue;
                }
            }
            // Jinns: at Rest, to Mana (exhausted) or to Reserve (and draw, for the rare one)
            let to_mana = def.flag("leaveExpeditionToMana") || def.flag("leaveExpeditionToManaOrDraw");
            if to_mana && !anchored {
                let mut reserve = serde_json::json!({"action": "Discard", "args": {"cardId": id, "destination": "reserve"}});
                if def.flag("leaveExpeditionToManaOrDraw") {
                    reserve = serde_json::json!({"type": "seq", "childs": [reserve, {"action": "Draw", "args": {"players": "me"}}]});
                }
                let node = serde_json::json!({"type": "xor", "childs": [
                    {"action": "Discard", "args": {"cardId": id, "destination": "mana", "tapped": true}},
                    reserve,
                ]});
                self.reactions.push((node, Ctx::of(seat, Some(id))));
                continue;
            }
            if anchored || eternal {
                let c = self.card_mut(id);
                c.remove_status(Status::Anchored);
                c.remove_status(Status::Asleep);
                continue;
            }
            let from = c.zone;
            let (fleeting, boosts) = (c.has(Status::Fleeting), c.boosts);
            let ascended = self.is_card_expedition_ascended(id);
            if fleeting {
                self.move_card(id, Zone::Discard, None);
                self.emit(Event { trigger: "Discard".into(), seat, card: Some(id), from: Some(from), to: Some(Zone::Discard), listeners: vec![id], discard: true, ..Default::default() });
            } else {
                self.move_card(id, Zone::Reserve, None);
            }
            let to = self.card(id).zone;
            self.emit_leave(Event { seat, card: Some(id), from: Some(from), to: Some(to), listeners: vec![id], fleeting, ascended, boosts: Some(boosts), ..Default::default() }, true);
        }
    }

    fn night_cleanup(&mut self, seat: Seat) {
        let p = &self.players[seat];
        let hero = p.hero.map(|h| self.card(h).def());
        let reserve_slots = hero.map(|h| h.reserve_slots).unwrap_or(2) as usize;
        let landmark_slots = hero.map(|h| h.landmark_slots).unwrap_or(2) as usize;
        let n_reserve = p.reserve.len().saturating_sub(reserve_slots);
        let n_landmarks = p.landmarks.len().saturating_sub(landmark_slots);
        if n_reserve == 0 && n_landmarks == 0 {
            return;
        }
        self.ask(Pending {
            id: 0,
            seat,
            prompt: format!("Night: discard {} card(s) from your Reserve and {} Landmark(s)", n_reserve, n_landmarks),
            source: None,
            can_pass: false,
            kind: PendingKind::Cleanup { reserve: p.reserve.clone(), n_reserve, landmarks: p.landmarks.clone(), n_landmarks },
            ctx: Ctx::of(seat, None),
        });
    }

    /// At the start of each Morning: a player whose expeditions met wins; both → the one who went
    /// further; equal → tiebreaker (expeditions back to the middle)
    fn check_victory(&mut self) -> bool {
        if self.tiebreaker {
            return false;
        }
        let progress = |p: &Player| p.position[0] + (7 - p.position[1]);
        let met: Vec<Seat> = (0..2).filter(|&s| self.players[s].gap() == 0).collect();
        match met.len() {
            0 => false,
            1 => {
                self.end_game(met[0]);
                true
            }
            _ => {
                let (a, b) = (progress(&self.players[0]), progress(&self.players[1]));
                if a != b {
                    self.end_game(if a > b { 0 } else { 1 });
                    true
                } else {
                    self.tiebreaker = true;
                    for p in self.players.iter_mut() {
                        p.position = [3, 4];
                    }
                    self.log_line(None, "Both expeditions met at the same time: tiebreaker in the Arena".into());
                    false
                }
            }
        }
    }

    pub fn end_game(&mut self, winner: Seat) {
        self.winner = Some(winner);
        self.phase = Phase::Ended;
        self.pending.clear();
        let name = self.players[winner].name.clone();
        self.log_line(Some(winner), format!("{} wins the game!", name));
    }

    // ------------------------------------------------------------------
    // Undo: a snapshot when a player starts deciding; dropped once hidden information is revealed
    // ------------------------------------------------------------------

    pub fn set_undo_point(&mut self, seat: Seat) {
        let mut snap = self.clone();
        snap.undo_point = None;
        self.undo_point = Some(Box::new(snap));
        self.undo_owner = Some(seat);
    }

    pub fn forget_undo(&mut self) {
        self.undo_point = None;
        self.undo_owner = None;
    }
}
