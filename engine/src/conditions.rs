//! Conditions of triggered abilities and of CheckCondition ("name:arg1:arg2" strings of the
//! original engine, `Helpers/Conditions.php`). `me` is the card whose ability is checked, `ev`
//! the event that triggered it. Unknown conditions are false (an ability never fires by
//! mistake) and listed by `KNOWN`.

use crate::catalog::{CardDef, CardType};
use crate::game::{Ctx, Event, Game};
use crate::model::*;
use serde_json::{Map, Value};

/// Conditions implemented here (for coverage reports)
pub const KNOWN: &[&str] = &[
    "isMe",
    "isNotMe",
    "hasSameOwner",
    "hasControl",
    "isCardAddedAnyPlayer",
    "notTapped",
    "isStillSameLocation",
    "isFirstPlayer",
    "isNotFirstPlayer",
    "isCardPlayed",
    "isCardAdded",
    "isInStorms",
    "isSacrificed",
    "isSacrifice",
    "isOpponentDraw",
    "isAfternoon",
    "hasCounterOnCard",
    "isDiscarded",
    "isSourceSameLocation",
    "isToReserve",
    "isNotFirstTurn",
    "isThisFeatCompleted",
    "isThisFeatIncomplete",
    "hasBoost",
    "hasNoBoost",
    "isFromReserve",
    "realResupply",
    "canSacrifice",
    "hasDiscardPileCards",
    "isCardOfType",
    "isMyTurn",
    "isNotToken",
    "isSupportEffect",
    "isMyselfDiscarded",
    "hasDeckCards",
    "checkReserveCards",
    "isPlayedInSameLocation",
    "isPlayedInOpponentExpedition",
    "excludeSelf",
    "hasCardsInHand",
    "hasReserve",
    "isReallyPlayed",
    "hasCardInDiscardPile",
    "notUsed",
    "canPay",
    "isNotGigantic",
    "isOpponentExpeditionNotEmpty",
    "isFirstPassing",
    "myExpeditionHasMoved",
    "myExpeditionHasNotMoved",
    "isSelfPlayCardEvent",
    "isNotMeInvoke",
    "isCharacterBoostedAndUntap",
    // Expeditions, regions, ascension
    "isCardExpeditionAscended",
    "isNotCardExpeditionAscended",
    "wasCardExpeditionAscended",
    "cardPlayedAscended",
    "countSourceAscended",
    "hasSourcePlayerAscended",
    "hasNoSourcePlayerAscended",
    "hasSourcePlayerAllAscended",
    "isOpponentExpeditionIn",
    "movesStormsWithForest",
    "movesStormDueToAscension",
    "movesStormsDueToAscension",
    "movesAnyExpeditionDueToAscension",
    "movesAscendedAnyExpeditions",
    "zhenZephyr",
    "zhenZephyrMoveExpedition",
    "hasNotMoved",
    "myOtherExpeditionHasMoved",
    "anyOfMyExpeditionsHasMoved",
    "myExpeditionIsBehind",
    "myExpeditionIsAhead",
    "myExpeditionIsNotBehind",
    "companionExpeditionIsBehind",
    "heroExpeditionIsBehind",
    "cardPlayedExpeditionIsBehind",
    "allExpeditionsAreBehindOrTied",
    "allExpeditionsAreNotBehindOrTied",
    "countMonoVisibleRegions",
    "cardInMonoRegion",
    "hasMarkers",
    "isInContact",
    "isNotInContact",
    "allInContact",
    "hasOneContact",
    "isInBiome",
    "isNotInBiome",
    "allExpeditionsNotIn",
    "isInBiomeWithZeroStat",
    "isPlayedCardInBiome",
    "isDiscardedCardInBiome",
    "isDiscardedCardNotInBiome",
    "isNotBlockedByOpponentPower",
    // Played cards
    "isAddedCardAnyPlayer",
    "isAddedCardOpponentEvent",
    "isCardAddedRobotOrToken",
    "isCardOfTypeRobotOrToken",
    "isCardPlayedNotCharacter",
    "isCardPlayedWithZeroStat",
    "isCharacterCostHigherThanCounter",
    "cardPlayedCostCheck",
    "isCharacterFromReserveNotBlocked",
    "isPlayCard",
    "isPlayedInOpponentOtherExp",
    "isPlayedInOwnOtherExpedition",
    "isNotPlayedInSameLocation",
    "isMeInvoke",
    "isCardPlayedSameOwner",
    "isFromHand",
    "isToDiscard",
    "isSource",
    "isSourceOrAugment",
    "isFacingSource",
    "specialEffect",
    "selectedRoll",
    "hasRolled",
    "isRollEqualCostHand",
    // Gains
    "hasGained",
    "hasGainedBoost",
    "hasGainedFleeting",
    "hasPlayerGained",
    "isGain",
    "isGainCardType",
    "isMyGainInReserve",
    "isGainInStorms",
    "isCharacterBoosted",
    "boostedByOtherCard",
    "isNonTokenBoostedAndUntap",
    // Discarded cards
    "isDiscardByOwner",
    "hasSameEventController",
    "isDiscardedType",
    "costCheck",
    "leaveAndFleeting",
    "notDestroyed",
    "notInDiscard",
    "notDiscarded",
    "isCharacterFromTarget",
    "isPermanentFromTarget",
    "isSpellFromTarget",
    "isTargetAsleep",
    "isTargetSameOwner",
    "isTargetFeatCompleted",
    "ownerOneReserve",
    "hasEverReachXDiscardedCards",
    // Control
    "hasControlFeat",
    "hasControlFeatWithMaxBaseCost",
    "hasCompletedFeat",
    "hasOpponentControl",
    "hasControlExhaustedPermanentOrReserve",
    "has4PermanentsOrReserveExhausted",
    "hasNoTokensInLandmarks",
    "has6HandCostLandmarks",
    "has3WithZeroStat",
    "hasXWithZeroStat",
    "controlInAllExpeditions",
    "hasOtherCharacterInAllExpeditions",
    "hasOtherSupportCardInReserveOrExpeditions",
    "hasOtherAnimalInReserveOrExpeditions",
    "hasControlFleetingAnchoredAsleep",
    "noRobotnoPermanent",
    "noPlantnoPermanent",
    "isOpponentExpeditionEmpty",
    "isOpponentExpeditionFilled",
    "soundTheHowlPass",
    "gatherThePackPass",
    "countOwnerBoosts",
    "isAsleep",
    "isAnchored",
    "isNotAnchored",
    "hasFleeting",
    "notFleeting",
    // Players
    "isHandEmpty",
    "isReserveEmpty",
    "hasXCardsInHand",
    "hasXExhaustedReserve",
    "hasXMana",
    "hasLessReserveCards",
    "hasBiggerHand",
    "hasSmallerHand",
    "canSabotage",
    "checkAbilityActivatedThisTurn",
    "checkAbilityNotActivatedThisTurn",
    "checkAbilityActivatedThisTurnTypeCount",
    "isSmokeThemOutArmed",
    "smokeThemOutLyTrigger",
    "smokeThemOutAxTrigger",
    "isExhaustedInLocation",
    "isNight",
    "isNotNight",
    "heroHasCounter",
    "isUsed",
];

thread_local! {
    static UNKNOWN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Unknown conditions met since the last call (reported as unsupported by the game)
pub fn take_unknown() -> Vec<String> {
    UNKNOWN.with(|u| std::mem::take(&mut *u.borrow_mut()))
}

pub fn check_power(g: &Game, power: &Value, ctx: &Ctx) -> bool {
    let mut conds: Vec<&str> = vec![];
    if let Some(list) = power.get("listeningConditions").and_then(Value::as_array) {
        conds.extend(list.iter().filter_map(Value::as_str));
    }
    if let Some(list) = power.get("conditions").and_then(Value::as_array) {
        conds.extend(list.iter().filter_map(Value::as_str));
    }
    match power.get("condition") {
        Some(Value::String(s)) => conds.push(s),
        Some(Value::Array(list)) => conds.extend(list.iter().filter_map(Value::as_str)),
        _ => {}
    }
    conds.into_iter().all(|c| check(g, c, ctx))
}

/// CheckCondition action: as in the original engine, the conditions see the triggering event
/// only with `previousEvent`; otherwise an event of the resolving player about the card chosen
/// by an enclosing Target. Without a source card, the player's hero is checked.
pub fn check_condition_action(g: &Game, a: &Map<String, Value>, ctx: &Ctx) -> bool {
    let mut c = ctx.clone();
    if !a.get("previousEvent").and_then(Value::as_bool).unwrap_or(false) {
        c.event = Some(Box::new(Event { trigger: "CheckCondition".into(), seat: ctx.seat, card: ctx.target, ..Default::default() }));
    }
    if c.source.is_none() {
        c.source = g.players[ctx.seat].hero;
    }
    check_power(g, &Value::Object(a.clone()), &c)
}

// ----------------------------------------------------------------------
// State helpers (original engine's Player / Players helpers)
// ----------------------------------------------------------------------

impl Game {
    /// An ability of `kind` ("tap", "discard"...) was activated during `seat`'s turn
    /// (Globals AbilityActivatedThisTurn / TypeCount)
    pub fn note_ability(&mut self, seat: Seat, kind: &str) {
        *self.players[seat].records.abilities.entry(kind.to_string()).or_insert(0) += 1;
    }

    pub fn is_ascended(&self, seat: Seat, side: Side) -> bool {
        self.players[seat].ascended[side.index()]
    }

    /// The card is in an ascended expedition of its controller (a Gigantic card: either one)
    pub fn is_card_expedition_ascended(&self, id: CardId) -> bool {
        let c = self.card(id);
        let Some(side) = c.zone.side() else { return false };
        self.is_ascended(c.controller, side) || (c.is_gigantic() && self.is_ascended(c.controller, side.other()))
    }

    /// Terrain markers placed on a region (the last one placed wins)
    fn region_marker(&self, region: i32) -> Option<Biome> {
        self.players.iter().flat_map(|p| p.records.region_markers.iter()).filter(|(r, _)| *r == region).map(|(_, b)| *b).next_back()
    }

    /// Biomes of the region an expedition stands in, with terrain markers and the tiebreaker
    /// (Player::getBiomes; biome modifiers of cards are not supported)
    pub fn expedition_biomes(&self, seat: Seat, side: Side) -> Vec<Biome> {
        if self.tiebreaker {
            return Biome::ALL.to_vec();
        }
        let pos = self.players[seat].position[side.index()];
        match self.region_marker(pos) {
            Some(b) => vec![b],
            None => self.region_biomes(pos),
        }
    }

    /// Revealed regions and their biomes (Players::getRegionsInfo)
    pub fn visible_regions(&self) -> Vec<(i32, Vec<Biome>)> {
        if self.tiebreaker {
            return vec![(4, Biome::ALL.to_vec())];
        }
        (0..=7)
            .filter(|&r| r == 0 || r == 7 || self.storm.get(((r + 1) / 2) as usize).map(|s| s.revealed).unwrap_or(false))
            .map(|r| (r, self.region_marker(r).map(|b| vec![b]).unwrap_or_else(|| self.region_biomes(r))))
            .collect()
    }

    /// Player ahead on an expedition: Some(seat), or None when tied (Players::getWinningPlayerByStorms)
    pub fn expedition_leader(&self, side: Side) -> Option<Seat> {
        let (a, b) = (self.players[0].position[side.index()], self.players[1].position[side.index()]);
        let ahead0 = if side == Side::Hero { a > b } else { a < b };
        if a == b {
            None
        } else if ahead0 {
            Some(0)
        } else {
            Some(1)
        }
    }

    /// A card of `seat` in this expedition (or a Gigantic one) has the property (overrideBehind, overrideContact...)
    fn has_expedition_flag(&self, seat: Seat, side: Side, flag: &str) -> bool {
        self.in_play(seat).iter().any(|&id| {
            let c = self.card(id);
            (c.zone == side.zone() || (c.is_gigantic() && c.zone.is_expedition())) && c.def().flag(flag)
        })
    }

    /// The expedition stands in the same region as an opposing one (Player::isInContact)
    pub fn in_contact(&self, seat: Seat, side: Side) -> bool {
        if self.has_expedition_flag(seat, side, "overrideContact") {
            return true;
        }
        let pos = self.players[seat].position[side.index()];
        self.players[Game::other(seat)].position.contains(&pos)
    }

    /// An opposing card stops on-play abilities in this expedition (Players::hasOpponentBlockingPower)
    pub fn opponent_blocks(&self, seat: Seat, zone: Option<Zone>, gigantic: bool) -> bool {
        let opp = Game::other(seat);
        let p = &self.players[opp];
        let infinity = p.reserve.iter().copied().filter(|&c| self.card(c).def().effect("effectInfinity").is_some());
        self.in_play(opp).into_iter().chain(infinity).any(|id| {
            let c = self.card(id);
            (Some(c.zone) == zone || c.is_gigantic() || gigantic) && c.def().flag("blockingPower")
        })
    }
}

// ----------------------------------------------------------------------
// Card helpers
// ----------------------------------------------------------------------

fn cmp(value: i32, n: i32, op: &str) -> bool {
    match op {
        "LTE" => value <= n,
        "EQ" | "E" => value == n,
        "LT" => value < n,
        "GT" => value > n,
        _ => value >= n,
    }
}

fn type_name(t: CardType) -> &'static str {
    match t {
        CardType::Hero => "hero",
        CardType::Character => "character",
        CardType::Spell => "spell",
        CardType::Permanent => "permanent",
        CardType::Token => "token",
    }
}

fn additional_types(d: &CardDef) -> Vec<&str> {
    d.prop("additionalType").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

/// getType() == t or t in getAdditionalType()
fn is_type(c: &Card, t: &str) -> bool {
    type_name(c.def().card_type) == t || additional_types(c.def()).contains(&t)
}

const TYPE_WORDS: &[&str] = &["", "all", "hero", "character", "characterOnly", "token", "permanent", "spell"];

/// typeCheck of the original engine (card types only: any other word passes)
fn type_check(kind: &str, card: &Card) -> bool {
    let d = card.def();
    match kind {
        "permanent" | "spell" => is_type(card, kind),
        "character" => matches!(d.card_type, CardType::Character | CardType::Token),
        "characterOnly" => d.card_type == CardType::Character && !d.is_token,
        "token" => d.is_token,
        _ => true,
    }
}

/// typeCheck plus the subtype check the original engine does when the word is a subtype
fn type_matches(kind: &str, card: &Card) -> bool {
    type_check(kind, card) && (TYPE_WORDS.contains(&kind) || card.def().has_subtype(kind))
}

fn is_character(c: &Card) -> bool {
    matches!(c.def().card_type, CardType::Character | CardType::Token)
}

/// The original engine's "other expedition" of a card (stormRight unless it is in stormLeft)
fn other_expedition(c: &Card) -> Zone {
    if c.zone == Zone::HeroExpedition {
        Zone::CompanionExpedition
    } else {
        Zone::HeroExpedition
    }
}

fn reserve_cards(g: &Game, seat: Seat) -> impl Iterator<Item = &Card> {
    g.players[seat].reserve.iter().map(move |&c| g.card(c))
}

fn played_cards(g: &Game, seat: Seat) -> impl Iterator<Item = &Card> {
    g.in_play(seat).into_iter().map(move |c| g.card(c))
}

/// Number of printed statistics at 0 of the Characters (and tokens) among `cards`
fn zero_stats<'a>(cards: impl Iterator<Item = &'a Card>) -> i32 {
    cards.filter(|c| is_character(c)).map(|c| c.def().stats.iter().filter(|&&v| v == 0).count() as i32).sum()
}

fn uid_matches(c: &Card, name: &str) -> bool {
    let uid = c.def().prop("uid").and_then(Value::as_str).unwrap_or("");
    !name.is_empty() && (uid.contains(&format!("_{}_", name)) || uid.ends_with(&format!("_{}", name)))
}

/// hasControl of the original engine
#[allow(clippy::too_many_arguments)]
fn has_control(g: &Game, me: &Card, kind: &str, n: i32, exclude: bool, state: &str, op: &str, opponent: bool) -> bool {
    let seat = if opponent { Game::other(me.controller) } else { me.controller };
    let subtypes: Option<Vec<&str>> = if kind.contains('|') { Some(kind.split('|').collect()) } else { None };
    let is_sub = !TYPE_WORDS.contains(&kind) && subtypes.is_none();
    let types: &[&str] = match kind {
        "token" => &["character", "permanent"],
        "permanent" => &["permanent"],
        _ if is_sub || subtypes.is_some() => &["character", "token", "permanent", "spell"],
        _ => &["character", "token"],
    };
    let m = played_cards(g, seat)
        .filter(|c| types.iter().any(|t| is_type(c, t)))
        .filter(|c| kind != "token" || c.def().is_token)
        .filter(|c| !is_sub || c.def().has_subtype(kind))
        .filter(|c| subtypes.as_ref().map(|s| s.iter().any(|x| c.def().has_subtype(x))).unwrap_or(true))
        .filter(|c| !(exclude && c.id == me.id))
        .filter(|c| match state {
            "boosted" => c.boosts > 0,
            "fleeting" => c.has(Status::Fleeting),
            "exhausted" => c.exhausted,
            _ => true,
        })
        .count() as i32;
    match op {
        "LTE" => m <= n,
        "GTE" | "" => m >= n,
        _ => false,
    }
}

/// hasReserve of the original engine: more than n cards matching
#[allow(clippy::too_many_arguments)]
fn has_reserve(g: &Game, seat: Seat, kind: &str, cost_hand: &str, cost_reserve: &str, op: &str, state: &str, n: i32) -> bool {
    let types = ["hero", "character", "token", "permanent", "spell"];
    let count = reserve_cards(g, seat)
        .filter(|c| kind.is_empty() || if types.contains(&kind) { is_type(c, kind) } else { c.def().has_subtype(kind) })
        .filter(|c| cost_hand.parse::<i32>().map(|v| op_cost(c.def().cost_hand, v, op)).unwrap_or(true))
        .filter(|c| cost_reserve.parse::<i32>().map(|v| op_cost(c.def().cost_reserve, v, op)).unwrap_or(true))
        .filter(|c| match state {
            "boosted" => c.boosts > 0,
            "fleeting" => c.has(Status::Fleeting),
            "exhausted" => c.exhausted,
            _ => true,
        })
        .count() as i32;
    count > n
}

fn op_cost(value: i32, n: i32, op: &str) -> bool {
    match op {
        "LTE" => value <= n,
        "GTE" | "" => value >= n,
        _ => false,
    }
}

/// hasControlFeatWithMaxBaseCost of the original engine
fn feats(g: &Game, me: &Card, n: i32, exclude: bool, max_cost: i32, state: &str, op: &str) -> bool {
    let m = played_cards(g, me.controller)
        .filter(|c| c.def().has_subtype("feat"))
        .filter(|c| c.def().cost_hand.min(c.def().cost_reserve) <= max_cost)
        .filter(|c| !(exclude && c.id == me.id))
        .filter(|c| match state {
            "exhausted" => c.exhausted,
            "completed" => c.completed,
            "notcompleted" => !c.completed,
            _ => true,
        })
        .count() as i32;
    cmp(m, n, op)
}

/// countOpponentExpedition: opposing cards (of a type) facing the card
fn facing_opponents(g: &Game, me: &Card, kind: &str) -> i32 {
    let other = other_expedition(me);
    played_cards(g, Game::other(me.controller))
        .filter(|c| kind.is_empty() || is_type(c, kind))
        .filter(|c| c.zone == me.zone || (c.zone == other && (c.is_gigantic() || me.is_gigantic())))
        .count() as i32
}

/// No opposing Character in this expedition (nor a Gigantic one anywhere)
fn unopposed(g: &Game, seat: Seat, zone: Zone) -> bool {
    !played_cards(g, Game::other(seat)).any(|c| is_type(c, "character") && (c.zone == zone || (c.zone.is_expedition() && c.is_gigantic())))
}

/// isInBiome of the original engine
fn in_biome(g: &Game, c: &Card, biome: Option<Biome>) -> bool {
    let Some(b) = biome else { return false };
    let seat = c.controller;
    if c.is_gigantic() {
        Side::BOTH.iter().any(|&s| g.expedition_biomes(seat, s).contains(&b))
    } else {
        g.expedition_biomes(seat, expedition_side(c.zone)).contains(&b)
    }
}

/// Expedition of a location as the original engine reads it (Player::getBiomes): any location
/// but stormLeft reads as the companion expedition
fn expedition_side(z: Zone) -> Side {
    if z == Zone::HeroExpedition {
        Side::Hero
    } else {
        Side::Companion
    }
}

/// isMyExpeditionAheadOrBehind of the original engine
fn ahead_or_behind(g: &Game, c: &Card, ev: &Event, ahead: bool) -> bool {
    let seat = c.controller;
    if let Some(side) = c.zone.side() {
        if g.has_expedition_flag(seat, side, "overrideBehind") {
            return !ahead;
        }
    }
    let location = if ev.card != Some(c.id) {
        c.zone
    } else {
        match ev.to {
            Some(Zone::Limbo) | Some(Zone::Landmark) | None => c.zone,
            Some(z) => z,
        }
    };
    let Some(side) = location.side() else { return false };
    if g.tiebreaker {
        return false;
    }
    match g.expedition_leader(side) {
        None => false,
        Some(w) => (w == seat) == ahead,
    }
}

fn all_behind_or_tied(g: &Game, me: &Card) -> bool {
    let seat = me.controller;
    if g.has_expedition_flag(seat, Side::Hero, "overrideBehind") {
        return true;
    }
    let behind = |side: Side| g.has_expedition_flag(seat, side, "overrideBehind") || g.expedition_leader(side) != Some(seat);
    behind(Side::Hero) && behind(Side::Companion)
}

/// Visible region the expedition stands in has a single biome
fn mono_region(g: &Game, seat: Seat, side: Side) -> bool {
    let pos = g.players[seat].position[side.index()];
    g.visible_regions().iter().any(|(r, b)| *r == pos && b.len() == 1)
}

/// The three statuses on three different Characters
fn fleeting_anchored_asleep(cards: &[&Card]) -> bool {
    let with = |s: Status| cards.iter().filter(move |c| c.has(s)).map(|c| c.id).collect::<Vec<_>>();
    let (asleep, anchored, fleeting) = (with(Status::Asleep), with(Status::Anchored), with(Status::Fleeting));
    asleep.iter().any(|a| anchored.iter().any(|b| b != a && fleeting.iter().any(|f| f != a && f != b)))
}

pub fn check(g: &Game, cond: &str, ctx: &Ctx) -> bool {
    let parts: Vec<&str> = cond.split(':').collect();
    let name = parts[0];
    let arg = |i: usize| parts.get(i).copied().unwrap_or("");
    let num = |i: usize, d: i32| parts.get(i).and_then(|s| s.parse::<i32>().ok()).unwrap_or(d);
    let op = |i: usize| if arg(i).is_empty() { "GTE" } else { arg(i) };
    let Some(me_id) = ctx.source.or(g.players[ctx.seat].hero) else { return false };
    let me = g.card(me_id);
    let owner = me.controller;
    let opp = Game::other(owner);
    let default_event = Default::default();
    let ev: &Event = ctx.event.as_deref().unwrap_or(&default_event);
    let ev_card = ev.card.map(|c| g.card(c));
    let is_me = ev.seat == owner && ctx.event.is_some();
    let is_gain = ev.trigger == "Gain";
    let my_turn = g.active == owner;
    // Expedition an event is about (MoveExpedition), else the card's
    let storm = ev.side.or(me.zone.side());
    let moves = &g.players[owner].records.storm_moves;
    let ascended_move = |s: Side| moves[s.index()].as_ref().filter(|_| ev.seat == owner && g.is_ascended(owner, s));
    let abilities = |kind: &str| g.players[owner].records.abilities.get(kind).copied().unwrap_or(0);
    let biome = |i: usize| Biome::from_engine(arg(i));
    // isCardAdded / isCardAddedAnyPlayer / isAddedCardOpponentEvent:
    // (kind, cost, op, exclude) checks on the card of a play event
    let added = |kind: &str, cost: &str, cost_op: &str, exclude: &str| -> bool {
        let Some(c) = ev_card else { return false };
        if exclude == "true" && c.id == me_id {
            return false;
        }
        if !type_matches(kind, c) {
            return false;
        }
        if let Ok(cost) = cost.parse::<i32>() {
            let h = c.def().cost_hand;
            match cost_op {
                "LTE" if h > cost => return false,
                "E" if h != cost => return false,
                "GTE" | "" if h < cost => return false,
                _ => {}
            }
        }
        true
    };
    let any_player_added = |kind: &str, exclude: &str| -> bool {
        ev.play && !(ev.trigger == "MoveCard") && ev_card.map(|c| c.controller == owner).unwrap_or(false) && added(kind, arg(2), op(3), exclude)
    };

    match name {
        // ---------------- Players / events ----------------
        "isMe" => is_me,
        "isNotMe" => ctx.event.is_some() && ev.seat != owner,
        "isFirstPlayer" => is_me && owner == g.first,
        "isNotFirstPlayer" => is_me && owner != g.first,
        "isFirstPassing" => g.players[owner].records.passed_first,
        "isMyTurn" => g.phase == Phase::Afternoon && my_turn,
        "isAfternoon" => g.phase == Phase::Afternoon,
        "isNight" => g.phase == Phase::Night,
        "isNotNight" => g.phase != Phase::Night,
        "isNotFirstTurn" => g.day != 1,
        "isOpponentDraw" => ctx.event.is_some() && ev.seat != owner && ev.to != Some(Zone::Mana),
        "realResupply" => true,
        "isSupportEffect" => ev.support,
        "isFromReserve" => ev.from == Some(Zone::Reserve),
        "isFromHand" => ev.from == Some(Zone::Hand),
        "isToReserve" => ev.to == Some(Zone::Reserve),
        "isToDiscard" => ev.to == Some(Zone::Discard),
        "isStillSameLocation" => ev_card.map(|c| Some(c.zone) == ev.to).unwrap_or(false),
        // The listening card's location when the event happened: conditions are checked then
        "isSourceSameLocation" => true,
        "isSource" => ev.source == Some(me_id),
        "isSourceOrAugment" => ev.source == Some(me_id) || (ev.trigger == "SpecialEffect" && ev.card == Some(me_id)),
        "isFacingSource" => ev_card.map(|c| c.controller != owner && (c.zone == me.zone || c.is_gigantic())).unwrap_or(false),
        "excludeSelf" => ev.card.or(ev.source) != Some(me_id),
        "hasSameOwner" => ev_card.map(|c| c.controller == owner).unwrap_or(false),
        "hasSameEventController" => ctx.event.is_some() && ev.seat == owner,
        "specialEffect" => ev.trigger == "SpecialEffect" && ev.what.as_deref() == Some(arg(1)),
        "selectedRoll" => {
            let roll = if ev.trigger == "RollDie" { ev.n } else { -1 };
            match op(2) {
                "LTE" => roll <= num(1, 0),
                "EQ" => roll == num(1, 0),
                "GTE" => roll >= num(1, 0),
                _ => false,
            }
        }
        "hasRolled" => ev.trigger == "RollDie" && cmp(ev.n, num(1, 1), op(2)),
        // Rolled value equals the Hand cost of a card revealed by the roll (in Limbo here)
        "isRollEqualCostHand" => ev.trigger == "RollDie" && g.players[owner].limbo.iter().any(|&c| g.card(c).def().cost_hand == ev.n),

        // ---------------- The card itself ----------------
        "notTapped" => !me.exhausted,
        "notUsed" => !crate::special::is_used(g, me_id),
        "isUsed" => crate::special::is_used(g, me_id),
        "isInStorms" => me.zone.is_expedition(),
        "isAsleep" => me.has(Status::Asleep),
        "isAnchored" => me.has(Status::Anchored),
        "isNotAnchored" => !me.has(Status::Anchored),
        "hasFleeting" => me.has(Status::Fleeting) || ev.fleeting,
        "notFleeting" => !me.has(Status::Fleeting) && !ev.fleeting,
        "notInDiscard" => me.zone != Zone::Discard,
        "notDestroyed" => me.zone != Zone::Gone,
        "notDiscarded" => ev.card == Some(me_id) && ev.to == Some(Zone::Reserve),
        "isThisFeatCompleted" => me.completed,
        "isThisFeatIncomplete" => !me.completed,
        "hasCounterOnCard" => cmp(me.counter, num(1, 1), op(2)),
        "heroHasCounter" => g.players[owner].hero.map(|h| cmp(g.card(h).counter, num(1, 1), op(2))).unwrap_or(false),
        // A leave event keeps the boosts the card had
        "hasBoost" => cmp(ev.boosts.unwrap_or(me.boosts), num(1, 1), op(2)),
        "hasNoBoost" => ev.boosts.unwrap_or(me.boosts) <= 0,
        "isSmokeThemOutArmed" => g.players[owner].records.smoke_armed.contains(&me_id),
        "isCardExpeditionAscended" => g.is_card_expedition_ascended(me_id),
        "isNotCardExpeditionAscended" => !g.is_card_expedition_ascended(me_id),
        "wasCardExpeditionAscended" => ev.ascended,
        "cardPlayedAscended" => ev.card.map(|c| g.is_card_expedition_ascended(c)).unwrap_or(false),
        "countSourceAscended" | "hasSourcePlayerAscended" => g.players[owner].ascended.iter().any(|&a| a),
        "hasNoSourcePlayerAscended" => !g.players[owner].ascended.iter().any(|&a| a),
        "hasSourcePlayerAllAscended" => g.players[owner].ascended.iter().all(|&a| a),

        // ---------------- Played cards ----------------
        "isCardPlayed" | "isCardAdded" => is_me && ev.play && added(arg(1), arg(2), op(3), arg(4)),
        "isCardPlayedNotCharacter" => !(is_me && ev.play && added("character", "", "GTE", "")),
        "isCardAddedAnyPlayer" => any_player_added(arg(1), arg(4)),
        "isCardAddedRobotOrToken" => any_player_added("robot", "") || any_player_added("token", ""),
        "isAddedCardAnyPlayer" => ev.play && ev_card.map(|c| type_check(arg(1), c)).unwrap_or(false),
        "isAddedCardOpponentEvent" => ev.play && !is_me && added(arg(1), arg(2), op(3), arg(4)),
        "isPlayCard" => ev.play,
        "isSelfPlayCardEvent" => ev.play && ev.card == Some(me_id),
        // reallyPlayed is false only for PlayCard effects with reallyPlayed: false (not modelled)
        "isReallyPlayed" => true,
        "isCardOfType" => ev_card.map(|c| type_matches(arg(1), c)).unwrap_or(false),
        "isCardOfTypeRobotOrToken" => ev_card.map(|c| type_matches("robot", c) || type_matches("token", c)).unwrap_or(false),
        "isCardPlayedSameOwner" => ev_card.map(|c| c.controller == owner).unwrap_or(false),
        "isCardPlayedWithZeroStat" => {
            is_me && ev.play && added("character", "", "GTE", "") && ev_card.map(|c| c.def().stats.contains(&0)).unwrap_or(false)
        }
        "isInBiomeWithZeroStat" => {
            let Some(c) = ev_card else { return false };
            is_me && ev.play && added("character", "", "GTE", "") && {
                let d = c.def();
                Biome::ALL.iter().any(|&b| d.stats[b.index()] == 0 && in_biome(g, c, Some(b)))
            }
        }
        "isCharacterCostHigherThanCounter" => {
            is_me && ev.play && added("character", "", "GTE", "") && ev_card.map(|c| c.def().cost_hand >= me.counter).unwrap_or(false)
        }
        "cardPlayedCostCheck" => {
            let Some(c) = ev_card else { return false };
            let d = c.def();
            let value = match arg(2) {
                "" | "base" => {
                    if ev.from == Some(Zone::Reserve) {
                        d.cost_reserve
                    } else {
                        d.cost_hand
                    }
                }
                "hand" => d.cost_hand,
                _ => d.cost_reserve,
            };
            op_cost(value, num(1, 0), op(3))
        }
        "isCharacterFromReserveNotBlocked" => {
            is_me
                && ev.play
                && added("character", "", "GTE", "")
                && ev.from == Some(Zone::Reserve)
                && !g.opponent_blocks(owner, ev.to, me.is_gigantic())
        }
        "isNotBlockedByOpponentPower" => !g.opponent_blocks(owner, ev.to.or(Some(me.zone)), me.is_gigantic()),
        "isPlayedInSameLocation" => {
            ev_card.map(|c| c.controller == owner && (Some(me.zone) == ev.to || c.is_gigantic() || me.is_gigantic())).unwrap_or(false)
        }
        "isNotPlayedInSameLocation" => Some(me.zone) != ev.to,
        "isPlayedInOpponentExpedition" => ev_card
            .map(|c| {
                let to_storm = ev.to.map(Zone::is_expedition).unwrap_or(false);
                c.controller != owner && (ev.to == Some(me.zone) || (to_storm && (c.is_gigantic() || me.is_gigantic())))
            })
            .unwrap_or(false),
        "isPlayedInOpponentOtherExp" => {
            let other = other_expedition(me);
            ev_card.map(|c| c.controller != owner).unwrap_or(false)
                && ev.to == Some(other)
                && played_cards(g, opp).filter(|c| c.zone == other && is_type(c, "character")).count() == 1
        }
        "isPlayedInOwnOtherExpedition" => ev_card
            .map(|c| {
                let to_storm = ev.to.map(Zone::is_expedition).unwrap_or(false);
                c.controller == owner && (ev.to == Some(other_expedition(me)) || (c.is_gigantic() && to_storm))
            })
            .unwrap_or(false),
        // locationPId: the player whose zone the card entered (play events only)
        "isNotMeInvoke" => ev.play && ev_card.map(|c| c.controller != owner).unwrap_or(false),
        "isMeInvoke" => !ev.play || ev_card.map(|c| c.controller == owner).unwrap_or(true),
        "isNotGigantic" => ev_card.map(|c| !c.is_gigantic()).unwrap_or(true),
        "isNotToken" => ev_card.map(|c| !c.def().is_token).unwrap_or(true),
        "cardPlayedExpeditionIsBehind" => ev_card.map(|c| ahead_or_behind(g, c, ev, false)).unwrap_or(false),
        "isPlayedCardInBiome" => match (ev_card, biome(1)) {
            (Some(c), Some(b)) => g.expedition_biomes(owner, expedition_side(c.zone)).contains(&b),
            _ => false,
        },

        // ---------------- Gains ----------------
        "hasGained" => is_gain && ev.card == Some(me_id) && ev.what.as_deref() == Some(arg(1)),
        "hasGainedBoost" => is_gain && ev.card == Some(me_id) && ev.what.as_deref() == Some("boost"),
        "isGain" => is_gain && ev.what.as_deref() == Some(arg(1)),
        "isGainCardType" => ev_card.map(|c| type_check(arg(1), c)).unwrap_or(false),
        "hasPlayerGained" => is_gain && ev.what.as_deref() == Some(arg(1)) && ev_card.map(|c| c.controller == owner).unwrap_or(false),
        "hasGainedFleeting" => {
            is_gain && ev.what.as_deref() == Some("fleeting") && ev_card.map(|c| c.controller == owner && is_character(c)).unwrap_or(false)
        }
        "isMyGainInReserve" => is_gain && ev_card.map(|c| c.zone == Zone::Reserve && c.controller == owner).unwrap_or(false),
        "isGainInStorms" => is_gain && ev_card.map(|c| c.zone.is_expedition()).unwrap_or(false),
        "isCharacterBoosted" => is_me && is_gain && ev.what.as_deref() == Some("boost") && ev_card.map(|c| c.controller == owner).unwrap_or(false),
        "boostedByOtherCard" => is_gain && ev.source != Some(me_id) && ev.what.as_deref() == Some("boost") && ev.card == Some(me_id),
        "isCharacterBoostedAndUntap" => {
            !me.exhausted
                && is_gain
                && ev.what.as_deref() == Some("boost")
                && ev_card.map(|c| c.controller == owner && c.zone.is_expedition()).unwrap_or(false)
        }
        "isNonTokenBoostedAndUntap" => {
            !me.exhausted
                && is_gain
                && ev.what.as_deref() == Some("boost")
                && ev_card
                    .map(|c| c.controller == owner && c.zone.is_expedition() && c.def().card_type == CardType::Character && !c.def().is_token)
                    .unwrap_or(false)
        }

        // ---------------- Discarded / targeted cards ----------------
        "isSacrifice" => {
            ev.sacrifice
                && (arg(1).is_empty() || ev_card.map(|c| type_check(arg(1), c)).unwrap_or(false))
                && (arg(2).is_empty() || ev_card.map(|c| c.def().has_subtype(arg(2)) != (arg(3) == "true")).unwrap_or(false))
        }
        "isSacrificed" => ev.sacrifice && ev.discard && ev.card == Some(me_id),
        "isMyselfDiscarded" => ev.discard && ev.card == Some(me_id) && discarded_matches(ev, arg(1), arg(2)),
        "isDiscarded" => {
            ev.discard && discarded_matches(ev, arg(1), arg(2)) && (arg(3).is_empty() || ev_card.map(|c| type_check(arg(3), c)).unwrap_or(false))
        }
        "isDiscardedType" => arg(1).is_empty() || ev_card.map(|c| type_check(arg(1), c)).unwrap_or(false),
        "isDiscardByOwner" => {
            let Some(c) = ev_card else { return false };
            if c.owner != owner || ev.seat != owner {
                return false;
            }
            // Sabotage (discarded by an opposing effect): only from the Reserve
            let sabotage = ev.source.map(|s| g.card(s).controller != owner).unwrap_or(false);
            !sabotage || ev.from == Some(Zone::Reserve)
        }
        "leaveAndFleeting" => ev.fleeting,
        "costCheck" => ev_card.map(|c| op_cost(c.def().cost_hand, num(1, 0), op(2))).unwrap_or(false),
        "isCharacterFromTarget" => ev_card.map(is_character).unwrap_or(false),
        "isPermanentFromTarget" => ev_card.map(|c| is_type(c, "permanent")).unwrap_or(false),
        "isSpellFromTarget" => ev_card.map(|c| is_type(c, "spell")).unwrap_or(false),
        "isTargetAsleep" => ev_card.map(|c| c.has(Status::Asleep)).unwrap_or(false),
        "isTargetSameOwner" => ev_card.map(|c| c.controller == owner).unwrap_or(false),
        "isTargetFeatCompleted" => ev_card.map(|c| c.completed).unwrap_or(false),
        "ownerOneReserve" => ev_card.map(|c| g.players[c.controller].reserve.len() == 1).unwrap_or(false),
        "isDiscardedCardInBiome" | "isDiscardedCardNotInBiome" => {
            let inside = (|| {
                let c = ev_card?;
                let from = ev.from.or(ctx.target_from)?;
                let side = from.side()?;
                let b = biome(1)?;
                let seat = c.controller;
                Some(g.expedition_biomes(seat, side).contains(&b) || (c.is_gigantic() && g.expedition_biomes(seat, side.other()).contains(&b)))
            })()
            .unwrap_or(false);
            inside == (name == "isDiscardedCardInBiome")
        }
        "isExhaustedInLocation" => ev_card.map(|c| Some(c.zone) == Zone::from_engine(arg(1))).unwrap_or(false),

        // ---------------- Expeditions ----------------
        "isInContact" => me
            .zone
            .side()
            .map(|s| if me.is_gigantic() { Side::BOTH.iter().any(|&x| g.in_contact(owner, x)) } else { g.in_contact(owner, s) })
            .unwrap_or(false),
        "isNotInContact" => !me
            .zone
            .side()
            .map(|s| if me.is_gigantic() { Side::BOTH.iter().any(|&x| g.in_contact(owner, x)) } else { g.in_contact(owner, s) })
            .unwrap_or(false),
        "allInContact" => g.in_contact(owner, Side::Hero) && g.in_contact(owner, Side::Companion),
        "hasOneContact" => g.in_contact(owner, Side::Hero) || g.in_contact(owner, Side::Companion),
        "isInBiome" => in_biome(g, me, biome(1)),
        "isNotInBiome" => !in_biome(g, me, biome(1)),
        "allExpeditionsNotIn" => biome(1).map(|b| Side::BOTH.iter().all(|&s| !g.expedition_biomes(owner, s).contains(&b))).unwrap_or(false),
        "isOpponentExpeditionIn" => {
            let Some(b) = biome(1) else { return false };
            let in_side = |s: Side| g.expedition_biomes(opp, s).contains(&b);
            let side = expedition_side(me.zone);
            in_side(side) || (me.is_gigantic() && in_side(side.other()))
        }
        "countMonoVisibleRegions" => {
            let count = g.visible_regions().iter().filter(|(_, b)| b.len() == 1).count() as i32;
            match op(2) {
                "LTE" => count <= num(1, 1),
                "GTE" => count >= num(1, 1),
                _ => false,
            }
        }
        "cardInMonoRegion" => {
            let side = expedition_side(me.zone);
            mono_region(g, owner, side) || (me.is_gigantic() && mono_region(g, owner, side.other()))
        }
        "hasMarkers" => me.def().flag("createMarkers") && g.players[owner].records.markers_used < 3,
        "myExpeditionIsBehind" => ahead_or_behind(g, me, ev, false),
        "myExpeditionIsAhead" => ahead_or_behind(g, me, ev, true),
        "myExpeditionIsNotBehind" => !ahead_or_behind(g, me, ev, false),
        "heroExpeditionIsBehind" | "companionExpeditionIsBehind" => {
            let side = if name == "heroExpeditionIsBehind" { Side::Hero } else { Side::Companion };
            g.has_expedition_flag(owner, side, "overrideBehind") || g.expedition_leader(side).map(|w| w != owner).unwrap_or(false)
        }
        "allExpeditionsAreBehindOrTied" => all_behind_or_tied(g, me),
        "allExpeditionsAreNotBehindOrTied" => !all_behind_or_tied(g, me),
        "myExpeditionHasMoved" => {
            let Some(s) = me.zone.side() else { return false };
            ev.seat == owner
                && moves[s.index()].as_ref().map(|m| m.moves > 0).unwrap_or(false)
                && !g.tiebreaker
                && ev.side.map(|x| x == s).unwrap_or(true)
        }
        "myExpeditionHasNotMoved" => {
            let side = me.zone.side();
            ev.seat == owner
                && side.and_then(|s| moves[s.index()].as_ref()).map(|m| m.moves == 0).unwrap_or(true)
                && !g.tiebreaker
                && ev.side.map(|x| Some(x) == side).unwrap_or(true)
        }
        "myOtherExpeditionHasMoved" => {
            let other = other_expedition(me).side().unwrap_or(Side::Hero);
            ev.seat == owner
                && moves[other.index()].as_ref().map(|m| m.moves >= 0).unwrap_or(false)
                && !g.tiebreaker
                && ev.side.map(|x| x == other).unwrap_or(true)
        }
        "anyOfMyExpeditionsHasMoved" => ev.seat == owner && !g.tiebreaker && moves.iter().flatten().any(|m| m.moves > 0),
        "hasNotMoved" => ev.seat == owner && moves.iter().flatten().map(|m| m.moves.abs()).sum::<i32>() == 0 && !g.tiebreaker,
        "movesStormsWithForest" => {
            ev.seat == owner
                && storm.and_then(|s| moves[s.index()].as_ref()).map(|m| m.biomes.contains(&Biome::Forest) && m.moves >= 1).unwrap_or(false)
        }
        "movesStormDueToAscension" => storm.and_then(ascended_move).map(|m| m.moves >= 1 && m.from_ascension).unwrap_or(false),
        "movesStormsDueToAscension" => Side::BOTH.iter().all(|&s| ascended_move(s).map(|m| m.from_ascension).unwrap_or(false)),
        "movesAnyExpeditionDueToAscension" => Side::BOTH.iter().any(|&s| ascended_move(s).map(|m| m.from_ascension).unwrap_or(false)),
        "movesAscendedAnyExpeditions" => Side::BOTH.iter().any(|&s| ascended_move(s).map(|m| m.moves >= 1).unwrap_or(false)),
        "zhenZephyr" => Side::BOTH
            .iter()
            .any(|&s| unopposed(g, owner, s.zone()) && moves[s.index()].as_ref().map(|m| m.ascended && m.moves >= 1).unwrap_or(false)),
        "zhenZephyrMoveExpedition" => ev.n >= 1 && ev.ascended && ev.side.map(|s| unopposed(g, owner, s.zone())).unwrap_or(false),

        // ---------------- Control ----------------
        "hasControl" => has_control(g, me, arg(1), num(2, 1), arg(3) == "true", arg(4), op(5), arg(6) == "true"),
        "hasOpponentControl" => has_control(g, me, arg(1), num(2, 1), arg(3) == "true", arg(4), op(5), true),
        "canSacrifice" => has_control(g, me, "character", 1, false, "all", "GTE", false),
        "hasControlFeat" => has_control(g, me, "feat", num(1, 1), arg(2) == "true", arg(3), op(4), false),
        "hasControlFeatWithMaxBaseCost" => feats(g, me, num(1, 1), arg(2) == "true", num(3, 99), arg(4), op(5)),
        "hasCompletedFeat" => feats(g, me, num(1, 1), false, 99, "completed", op(2)),
        "hasControlExhaustedPermanentOrReserve" => {
            has_control(g, me, "permanent", 1, false, "exhausted", "GTE", false) || has_reserve(g, owner, "", "", "", "GTE", "exhausted", 0)
        }
        "has4PermanentsOrReserveExhausted" => {
            let permanents = played_cards(g, owner).filter(|c| is_type(c, "permanent") && c.exhausted).count();
            permanents + reserve_cards(g, owner).filter(|c| c.exhausted).count() >= 4
        }
        "hasNoTokensInLandmarks" => !played_cards(g, owner).any(|c| c.zone == Zone::Landmark && c.def().is_token),
        "has6HandCostLandmarks" => g.players[owner].landmarks.iter().map(|&c| g.card(c).def().cost_hand).sum::<i32>() >= 6,
        "has3WithZeroStat" => (ctx.event.is_none() || ev.seat == owner) && zero_stats(played_cards(g, owner)) >= 3,
        "hasXWithZeroStat" => {
            let n = if arg(1) == "all" || arg(1).is_empty() {
                zero_stats(played_cards(g, owner).chain(reserve_cards(g, owner)))
            } else {
                zero_stats(played_cards(g, owner))
            };
            match op(3) {
                "GTE" => n >= num(2, 1),
                "LTE" => n <= num(2, 1),
                _ => false,
            }
        }
        "controlInAllExpeditions" | "hasOtherCharacterInAllExpeditions" => {
            let (kind, include_self) = if name == "controlInAllExpeditions" { (arg(1), arg(2) == "true") } else { ("", false) };
            let cards: Vec<&Card> = played_cards(g, owner)
                .filter(|c| if kind.is_empty() || kind == "character" { is_character(c) } else { is_type(c, kind) })
                .filter(|c| include_self || c.id != me_id)
                .collect();
            let both = cards.iter().any(|c| c.is_gigantic())
                || (cards.iter().any(|c| c.zone == Zone::HeroExpedition) && cards.iter().any(|c| c.zone == Zone::CompanionExpedition));
            if name == "hasOtherCharacterInAllExpeditions" && arg(1) == "LTE" {
                !both
            } else {
                both
            }
        }
        "hasOtherSupportCardInReserveOrExpeditions" | "hasOtherAnimalInReserveOrExpeditions" => reserve_cards(g, owner)
            .chain(played_cards(g, owner))
            .filter(|c| c.id != me_id && c.zone != Zone::Landmark && c.def().card_type != CardType::Hero)
            .any(|c| {
                if name == "hasOtherAnimalInReserveOrExpeditions" {
                    c.def().has_subtype("animal")
                } else {
                    c.def().effect("effectSupport").is_some()
                }
            }),
        "hasControlFleetingAnchoredAsleep" => {
            let cards: Vec<&Card> = played_cards(g, owner).filter(|c| is_character(c)).collect();
            fleeting_anchored_asleep(&cards)
        }
        "noRobotnoPermanent" | "noPlantnoPermanent" => {
            let sub = if name == "noRobotnoPermanent" { "robot" } else { "plant" };
            !played_cards(g, owner).any(|c| c.id != me_id && (is_type(c, "permanent") || c.def().has_subtype(sub)))
        }
        "isOpponentExpeditionEmpty" => facing_opponents(g, me, "character") == 0,
        "isOpponentExpeditionNotEmpty" => facing_opponents(g, me, "character") != 0,
        "isOpponentExpeditionFilled" => {
            let n = facing_opponents(g, me, arg(1));
            match if arg(3).is_empty() { "EQ" } else { arg(3) } {
                "EQ" => n == num(2, 1),
                "GTE" => n >= num(2, 1),
                "LTE" => n <= num(2, 1),
                _ => false,
            }
        }
        "gatherThePackPass" | "soundTheHowlPass" => {
            let animals_only = name == "soundTheHowlPass" || arg(1).is_empty() || arg(1) == "animal";
            let mut values: Vec<i32> = played_cards(g, owner)
                .filter(|c| is_character(c) && (c.zone.is_expedition() || c.is_gigantic()))
                .filter(|c| !animals_only || c.def().has_subtype("animal"))
                .map(|c| {
                    let d = c.def();
                    if name == "soundTheHowlPass" {
                        d.stats[0] + c.boosts
                    } else if c.has(Status::Fleeting) {
                        d.cost_reserve
                    } else {
                        d.cost_hand
                    }
                })
                .collect();
            values.sort();
            values.dedup();
            values.len() >= 3
        }
        "countOwnerBoosts" => {
            let boosts: i32 = played_cards(g, owner).chain(reserve_cards(g, owner)).map(|c| c.boosts).sum();
            match op(2) {
                "GTE" => boosts >= num(1, 1),
                "LTE" => boosts <= num(1, 1),
                _ => false,
            }
        }

        // ---------------- Players ----------------
        "canPay" => g.mana_available(owner) >= num(1, 1),
        "hasCardsInHand" => !g.players[owner].hand.is_empty(),
        "isHandEmpty" => g.players[owner].hand.is_empty(),
        "hasXCardsInHand" => matches!(op(2), "GTE" | "LTE" | "EQ") && cmp(g.players[owner].hand.len() as i32, num(1, 0), op(2)),
        "hasBiggerHand" => g.players[owner].hand.len() > g.players[opp].hand.len(),
        "hasSmallerHand" => g.players[owner].hand.len() < g.players[opp].hand.len(),
        "hasDeckCards" => !g.players[owner].deck.is_empty(),
        "hasDiscardPileCards" => cmp(g.players[owner].discard.len() as i32, num(1, 1), op(2)),
        "hasEverReachXDiscardedCards" => {
            let r = &g.players[owner].records;
            let count = g.players[owner].discard.len() as i32;
            match op(2) {
                "GTE" => r.discard_peak.max(count) >= num(1, 1),
                "LTE" => count <= num(1, 1),
                "EQ" => count == num(1, 1),
                _ => false,
            }
        }
        "hasCardInDiscardPile" => g.players[owner].discard.iter().any(|&c| uid_matches(g.card(c), arg(1))),
        "checkReserveCards" => matches!(op(2), "GTE" | "LTE") && cmp(g.players[owner].reserve.len() as i32, num(1, 1), op(2)),
        "hasReserve" => has_reserve(g, owner, arg(1), arg(2), arg(3), op(4), arg(5), num(6, 0)),
        "isReserveEmpty" => g.players[owner].reserve.is_empty(),
        "hasLessReserveCards" => g.players[owner].reserve.len() < g.players[opp].reserve.len(),
        "hasXExhaustedReserve" => {
            let seats: Vec<Seat> = match arg(3) {
                "" => vec![0, 1],
                "me" => vec![owner],
                "opponent" => vec![opp],
                id => id.parse::<u32>().ok().and_then(|id| g.seat_of(id)).into_iter().collect(),
            };
            let n = seats.iter().map(|&s| reserve_cards(g, s).filter(|c| c.exhausted).count() as i32).sum::<i32>();
            matches!(op(2), "GTE" | "LTE" | "EQ") && cmp(n, num(1, 1), op(2))
        }
        "hasXMana" => matches!(op(2), "GTE" | "LTE" | "LT" | "GT") && cmp(g.players[owner].mana.len() as i32, num(1, 1), op(2)),
        "canSabotage" => g.players.iter().any(|p| !p.reserve.is_empty()),
        "checkAbilityActivatedThisTurn" | "checkAbilityNotActivatedThisTurn" => {
            let r = &g.players[owner].records.abilities;
            let done = if arg(1).is_empty() || arg(1) == "any" { r.values().any(|&n| n > 0) } else { abilities(arg(1)) > 0 };
            done == (name == "checkAbilityActivatedThisTurn")
        }
        "checkAbilityActivatedThisTurnTypeCount" => matches!(op(3), "GTE" | "LTE" | "LT" | "GT") && cmp(abilities(arg(1)), num(2, 1), op(3)),
        "smokeThemOutLyTrigger" => {
            if !is_me || !my_turn || !ev.support {
                return false;
            }
            if !me.completed {
                abilities("discard") >= 2
            } else {
                g.players[owner].records.smoke_armed.contains(&me_id) && abilities("discard") >= 1
            }
        }
        "smokeThemOutAxTrigger" => {
            if !is_me || !my_turn {
                return false;
            }
            if !me.completed {
                abilities("tap") >= 3
            } else {
                g.players[owner].records.smoke_armed.contains(&me_id) && abilities("tap") >= 1 && ev.card.or(ev.source) != Some(me_id)
            }
        }
        _ => {
            UNKNOWN.with(|u| u.borrow_mut().push(format!("condition {}", name)));
            false
        }
    }
}

fn discarded_matches(ev: &Event, from: &str, to: &str) -> bool {
    let from_ok = from.is_empty() || ev.from == Zone::from_engine(from);
    let to_ok = to.is_empty() || ev.to == Zone::from_engine(to);
    from_ok && to_ok
}
