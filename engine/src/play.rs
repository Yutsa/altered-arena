//! Afternoon actions: playing cards, support abilities (discard from Reserve), exhaust abilities, passing.
//!
//! Costs and playability mirror the original engine: `Card::getCost` (cost from the card's zone),
//! `Card::canBePlayed` (affordable at all), `Card::getPlayableLocation` (where it can go),
//! `ChooseAssignment::argsChooseAssignment` (the cards offered) and `ChooseAssignment::playCard`
//! (the cost actually paid, which depends on the destination).

use crate::catalog::CardType;
use crate::game::*;
use crate::model::*;
use serde_json::{json, Map, Value};

/// Type name of the original engine (getType)
pub fn type_str(c: &Card) -> &'static str {
    match c.card_type() {
        CardType::Character => "character",
        CardType::Spell => "spell",
        CardType::Permanent => "permanent",
        CardType::Token => "token",
        CardType::Hero => "hero",
    }
}

/// getAdditionalType
fn additional_types(c: &Card) -> Vec<&'static str> {
    c.def().prop("additionalType").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

/// The card is of this type, or also of this type (additionalType)
pub fn is_type(c: &Card, t: &str) -> bool {
    type_str(c) == t || additional_types(c).contains(&t)
}

/// Utils::costReductionBucketsOverlap: two cost reductions can apply to the same card
pub fn cost_buckets_overlap(a: &str, b: &str) -> bool {
    if a == "all" || b == "all" || a == b {
        return true;
    }
    // Utils::costReductionOverlapProbeCards: (type, additional types, subtypes)
    const PROBES: &[(&str, &[&str], &[&str])] = &[
        ("character", &[], &["artist"]),
        ("character", &[], &["animal", "spirit"]),
        ("character", &[], &["bureaucrat"]),
        ("character", &[], &["apprentice", "mage"]),
        ("spell", &[], &["song"]),
        ("permanent", &[], &["plant"]),
        ("permanent", &[], &["robot"]),
        ("token", &[], &[]),
        ("character", &["feat"], &[]),
    ];
    let applies = |bucket: &str, (t, add, sub): &(&str, &[&str], &[&str])| bucket == *t || add.contains(&bucket) || sub.contains(&bucket);
    PROBES.iter().any(|p| applies(a, p) && applies(b, p))
}

/// ChooseAssignment arguments of an effect's play ("you may play a card...")
pub fn bonus_play(a: &Map<String, Value>) -> BonusPlay {
    let int = |k: &str| a.get(k).and_then(Value::as_i64).map(|v| v as i32);
    let strings = |k: &str| crate::actions::list(a, k).unwrap_or_default();
    BonusPlay {
        types: strings("types")
            .iter()
            .filter_map(|t| match t.as_str() {
                "character" => Some(CardType::Character),
                "spell" => Some(CardType::Spell),
                "permanent" => Some(CardType::Permanent),
                _ => None,
            })
            .collect(),
        free: crate::actions::b(a, "free"),
        max_base_cost: int("maxBaseCost"),
        min_base_cost: int("minBaseCost"),
        max_hand_cost: int("maxHandCost"),
        min_hand_cost: int("minHandCost"),
        mandatory: crate::actions::b(a, "mandatory"),
        sub_types: if a.get("subType").and_then(Value::as_str) == Some("disabled") { vec![] } else { strings("subType") },
        forced: crate::actions::s(a, "forcedLocation").and_then(Zone::from_engine).and_then(Zone::side),
        reserve_flip: crate::actions::b(a, "reserveFlipCost"),
        ..Default::default()
    }
}

/// Name of a play destination in the original engine (location of PlayCard)
fn dest_location(dest: Dest) -> &'static str {
    match dest.zone() {
        Zone::HeroExpedition => "stormLeft",
        Zone::CompanionExpedition => "stormRight",
        Zone::Landmark => "landmark",
        _ => "limbo",
    }
}

impl Game {
    /// A card property, or while the card is ready in Reserve, the one of its effectInfinity
    /// (Card::__call)
    fn card_prop(&self, id: CardId, key: &str) -> Option<&'static Value> {
        let c = self.card(id);
        let def = c.def();
        def.prop(key).or_else(|| {
            if c.zone == Zone::Reserve && !c.exhausted {
                def.prop("effectInfinity").and_then(|e| e.get(key)).filter(|v| !v.is_null())
            } else {
                None
            }
        })
    }

    fn card_int(&self, id: CardId, key: &str) -> i32 {
        self.card_prop(id, key).and_then(Value::as_i64).unwrap_or(0) as i32
    }

    /// Utils::checkAttributeCondition: "value" or "value:condition" (None when the condition fails)
    fn attribute_condition(&self, data: &str, card: CardId) -> Option<String> {
        match data.split_once(':') {
            None => Some(data.to_string()),
            Some((value, cond)) => {
                let ctx = Ctx::of(self.card(card).controller, Some(card));
                crate::conditions::check(self, cond, &ctx).then(|| value.to_string())
            }
        }
    }

    /// Cards in play of a player and their ready Reserve cards with an {∞} ability
    /// (getPlayedCards()->merge(getInfinityCards()))
    fn played_and_infinity(&self, seat: Seat) -> Vec<CardId> {
        let mut ids = self.in_play(seat);
        ids.extend(self.players[seat].reserve.iter().copied().filter(|&c| self.card(c).def().effect("effectInfinity").is_some()));
        ids
    }

    /// Player::countCardsInLocation(expedition, [TOKEN, CHARACTER])
    fn characters_in(&self, seat: Seat, side: Side) -> usize {
        self.players[seat].expeditions[side.index()].iter().filter(|&&c| is_type(self.card(c), "character") || is_type(self.card(c), "token")).count()
    }

    /// Player::hasGigantic
    fn has_gigantic(&self, seat: Seat) -> bool {
        self.played_and_infinity(seat).iter().any(|&c| self.card(c).is_gigantic())
    }

    /// Player::isInContact() without location: either expedition
    fn in_contact_any(&self, seat: Seat) -> bool {
        self.in_contact(seat, Side::Hero) || self.in_contact(seat, Side::Companion)
    }

    /// Player::getOpponentMinimumCost (Robin Hood): the highest minimum set by the opponent's cards
    fn opponent_minimum_cost(&self, seat: Seat, c: &Card) -> i32 {
        let opp = Game::other(seat);
        let mut cost = 0;
        for src in self.played_and_infinity(opp) {
            let mut penalties: Vec<&Value> = vec![];
            if type_str(c) == "character" {
                penalties.extend(self.card_prop(src, "opponentCharactersMinimumCost"));
            }
            match self.card_prop(src, "opponentCardsMinimumCost") {
                Some(Value::Array(a)) => penalties.extend(a.iter()),
                Some(v) => penalties.push(v),
                None => {}
            }
            for p in penalties {
                let n = match p {
                    Value::Number(n) => n.as_i64().unwrap_or(0) as i32,
                    Value::String(s) => self.attribute_condition(s, src).and_then(|v| v.parse().ok()).unwrap_or(0),
                    _ => 0,
                };
                cost = cost.max(n);
            }
        }
        cost
    }

    /// Players::getMinimumReserveCost (minimumReserveCost of the cards of every player)
    fn minimum_reserve_cost(&self) -> i32 {
        (0..2).map(|s| self.played_and_infinity(s).iter().map(|&c| self.card_int(c, "minimumReserveCost")).sum::<i32>()).max().unwrap_or(0)
    }

    /// Players::getIncreaseReserveCost (Ebenezer Scrooge): every card in play
    fn increase_reserve_cost(&self) -> i32 {
        (0..2).flat_map(|s| self.in_play(s)).map(|c| self.card_int(c, "increaseReserveCost").max(0)).sum()
    }

    /// Players::getReduceReserveCost (Ebenezer Scrooge, BLISS Drummer): every card in play
    fn reduce_reserve_cost(&self, seat: Seat, played: CardId) -> i32 {
        let c = self.card(played);
        let mut total = 0;
        for src in (0..2).flat_map(|s| self.in_play(s)) {
            let fixed = self.card_int(src, "reduceReserveCost");
            if fixed > 0 {
                total += fixed;
                continue;
            }
            if src == played {
                continue;
            }
            let Some(data) = self.card_prop(src, "dynamicReduceReserveCost").and_then(Value::as_str).filter(|d| !d.is_empty()) else { continue };
            let mine = seat == self.card(src).controller;
            total += match self.attribute_condition(data, src).as_deref() {
                Some("myCharacter") => (type_str(c) == "character" && mine) as i32,
                Some("myArtist") if mine => c.def().has_subtype("artist") as i32,
                Some("myRobot") if mine => c.def().has_subtype("robot") as i32,
                Some(n) => n.parse().unwrap_or(0),
                None => 0,
            };
        }
        total
    }

    /// Player::getReduceCostType (Scholar's Vault, Studious Acolyte, Reka Welder): (reduction, minimum)
    fn reduce_cost_type(&self, seat: Seat, played: CardId) -> (i32, i32) {
        let c = self.card(played);
        let def = c.def();
        let (mut reduction, mut minimum) = (0, 0);
        for src in self.played_and_infinity(seat) {
            let Some(Value::Object(types)) = self.card_prop(src, "reduceCostType") else { continue };
            for (kind, info) in types {
                if !is_type(c, kind) {
                    continue;
                }
                let get = |k: &str| info.get(k).and_then(Value::as_i64).map(|v| v as i32);
                let applies = if get("maxHandCost").is_some_and(|m| def.cost_hand <= m) || get("minHandCost").is_some_and(|m| def.cost_hand >= m) {
                    true
                } else if let Some(base) = get("minBaseCost") {
                    (c.zone == Zone::Reserve && def.cost_reserve >= base) || (c.zone == Zone::Hand && def.cost_hand >= base)
                } else {
                    false
                };
                if applies {
                    reduction += get("reduction").unwrap_or(0);
                    minimum = minimum.max(get("minimum").unwrap_or(0));
                }
            }
        }
        (reduction, minimum)
    }

    /// dynamicCostReduction of a card (Card::getCost)
    fn dynamic_cost_reduction(&self, seat: Seat, id: CardId) -> i32 {
        let list: Vec<&str> = match self.card(id).def().prop("dynamicCostReduction") {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
            Some(Value::String(s)) => vec![s.as_str()],
            _ => vec![],
        };
        let mut total = 0;
        for r in list {
            if r.contains(':') {
                total += self.attribute_condition(r, id).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                continue;
            }
            total += match r {
                "" => 0,
                "exhaustedReserve" => self.players.iter().flat_map(|p| p.reserve.iter()).filter(|&&c| self.card(c).exhausted).count() as i32,
                "eachOwnerAscended" => self.is_ascended(seat, Side::Hero) as i32 + self.is_ascended(seat, Side::Companion) as i32,
                "playedCards" => self.mods.played_count,
                "hasPlayedCards" => (self.mods.played_count > 0) as i32,
                "hasInContact" => self.in_contact_any(seat) as i32,
                "expeditionsInContact" => self.in_contact(seat, Side::Hero) as i32 + self.in_contact(seat, Side::Companion) as i32,
                n => n.parse().unwrap_or(0),
            };
        }
        total
    }

    /// Card::getCost: cost to play a card from its zone (Hand, or Reserve), with the reductions
    /// and minimums in effect; `reserve_flip`: the lower of its Hand and Reserve costs
    pub fn card_cost(&self, seat: Seat, id: CardId, scout: bool, reserve_flip: bool) -> i32 {
        let c = self.card(id);
        let def = c.def();
        if crate::special::spell_is_free(self, seat, id) {
            return 0;
        }
        let mut minimum = self.opponent_minimum_cost(seat, c).max(self.minimum_reserve_cost());
        // Cost reductions of effects (Globals costReduction): by type, then by subtype
        let mut type_reduction = 0;
        let matches_type = |kind: &str| kind == "all" || is_type(c, kind);
        for (s, kind, r) in &self.mods.cost_reductions {
            if *s == seat && matches_type(kind) {
                type_reduction += r;
            }
        }
        for (s, kind, m) in &self.mods.cost_minimums {
            if *s == seat && matches_type(kind) {
                minimum = minimum.max(*m);
            }
        }
        for sub in &def.subtypes {
            for (s, kind, r) in &self.mods.cost_reductions {
                if *s == seat && kind == sub {
                    type_reduction += r;
                }
            }
            for (s, kind, m) in &self.mods.cost_minimums {
                if *s == seat && kind == sub {
                    minimum = minimum.max(*m);
                }
            }
        }
        let mut dynamic = self.dynamic_cost_reduction(seat, id);
        let increase_reserve = self.increase_reserve_cost();
        let reduce_reserve = self.reduce_reserve_cost(seat, id);
        if reduce_reserve > 0 && c.zone == Zone::Reserve {
            minimum = minimum.max(1);
        }
        let (type_cost_reduction, floor) = self.reduce_cost_type(seat, id);
        minimum = minimum.max(floor);
        dynamic += type_cost_reduction;
        match c.zone {
            Zone::Hand => {
                let initial = if scout && def.int("scout") > 0 { def.int("scout") } else { def.cost_hand };
                minimum.max(initial - type_reduction - dynamic)
            }
            Zone::Reserve => {
                let from = |base: i32| minimum.max(base - type_reduction - dynamic + increase_reserve - reduce_reserve);
                if reserve_flip {
                    from(def.cost_reserve).min(from(def.cost_hand))
                } else {
                    from(def.cost_reserve)
                }
            }
            // getCost has no cost for other zones
            _ => 0,
        }
    }

    /// Cost to play a card from its zone (Card::getCost)
    pub fn play_cost(&self, seat: Seat, id: CardId, scout: bool) -> i32 {
        self.card_cost(seat, id, scout, false).max(0)
    }

    /// Cost paid to play a card to a given destination (ChooseAssignment::playCard): some cards
    /// cost more or less depending on where they go (playLimitation: Diocles "+3StartingRegion",
    /// Lyra DJ "-2Contact", "-2Multi"; Amarok costReductionIfEmpty)
    pub fn play_cost_at(&self, seat: Seat, id: CardId, dest: Dest) -> i32 {
        self.paid_cost(seat, id, dest, false).max(0)
    }

    /// ChooseAssignment::playCard's cost, before the optional reductions it offers (discard,
    /// exhaust, sacrifice, limited effect) and costReductionIfEmpty
    fn paid_cost_before_offer(&self, seat: Seat, id: CardId, dest: Dest, reserve_flip: bool) -> i32 {
        let mut cost = self.card_cost(seat, id, dest.is_scout(), reserve_flip);
        let limitation = self.card(id).def().prop("playLimitation").and_then(Value::as_str).unwrap_or("");
        let side = dest.zone().side();
        match (limitation, side) {
            ("+3StartingRegion", Some(Side::Hero)) if self.players[seat].position[0] == 0 => cost += 3,
            ("+3StartingRegion", Some(Side::Companion)) if self.players[seat].position[1] == 7 => cost += 3,
            ("-2Contact", Some(side)) if self.in_contact(seat, side) => cost -= 2,
            ("-2Multi", _) if self.opponent_has_characters(seat) => cost -= 2,
            _ => {}
        }
        cost
    }

    fn paid_cost(&self, seat: Seat, id: CardId, dest: Dest, reserve_flip: bool) -> i32 {
        self.paid_cost_before_offer(seat, id, dest, reserve_flip) - self.empty_expedition_reduction(seat, id, dest)
    }

    /// Amarok (costReductionIfEmpty): less when played in an expedition without Characters
    fn empty_expedition_reduction(&self, seat: Seat, id: CardId, dest: Dest) -> i32 {
        let n = self.card(id).def().int("costReductionIfEmpty");
        match dest.zone().side() {
            Some(side) if n > 0 && self.characters_in(seat, side) == 0 && !self.has_gigantic(seat) => n,
            _ => 0,
        }
    }

    /// "-2Multi": the opponent has Characters in an expedition
    fn opponent_has_characters(&self, seat: Seat) -> bool {
        let opp = &self.players[Game::other(seat)];
        opp.expeditions.iter().flatten().any(|&c| self.card(c).is_character_like())
    }

    /// Optional cost reductions of the card itself, offered when it is played
    /// (ChooseAssignment::playCard): pay `cost`, or discard / exhaust a Reserve card, sacrifice a
    /// Permanent, or accept a limited effect, to pay less
    fn cost_reduction_offer(&self, seat: Seat, id: CardId, dest: Dest, cost: i32) -> Option<Value> {
        let c = self.card(id);
        let def = c.def();
        let loc = dest_location(dest);
        let play = |cost: i32, limited: bool| {
            json!({"action": "PlayCard", "args": {"cardId": id, "free": true, "location": loc, "cost": cost, "countsAsTurnPlay": true, "limited": limited}})
        };
        let p = &self.players[seat];
        let from_reserve = c.zone == Zone::Reserve;
        let all_types = json!(["character", "token", "spell", "permanent"]);
        let (discard, sacrifice, limitation, tap) = (
            def.int("costReductionDiscard"),
            def.int("costReductionSacrificePermanent"),
            def.int("costReductionLimitation"),
            def.int("costReductionTap"),
        );
        let target_then = |target: Value, reduction: i32| json!({"type": "xor", "childs": [play(cost, false), {"type": "seq", "childs": [target, play(cost - reduction, false)]}]});
        if discard > 0 {
            let n = p.reserve.len();
            if (from_reserve && n > 1) || (!from_reserve && n > 0) {
                return Some(target_then(
                    json!({"action": "Target", "args": {"targetLocation": ["reserve"], "targetPlayer": "me", "excludeSelf": true, "targetType": all_types, "effect": {"action": "Discard", "args": {}}}}),
                    discard,
                ));
            }
        } else if sacrifice > 0 {
            if self.in_play(seat).iter().any(|&x| is_type(self.card(x), "permanent")) {
                return Some(target_then(
                    json!({"action": "Target", "args": {"targetType": ["permanent"], "targetPlayer": "me", "effect": {"action": "Discard", "args": {"desc": "sacrifice"}}}}),
                    sacrifice,
                ));
            }
        } else if limitation > 0 {
            return Some(json!({"type": "xor", "childs": [play(cost, false), play(cost - limitation, true)]}));
        } else if tap > 0 {
            let n = p.reserve.iter().filter(|&&x| !self.card(x).exhausted).count();
            if (from_reserve && n > 1) || (!from_reserve && n > 0) {
                return Some(target_then(
                    json!({"action": "Target", "args": {"targetLocation": ["reserve"], "targetPlayer": "me", "excludeSelf": true, "isNotTapped": true, "targetType": all_types, "effect": {"action": "Exhaust", "args": {}}}}),
                    tap,
                ));
            }
        }
        None
    }

    /// Player::canPlayTappedCards (Vaike, Kelonic Heater, Icarus): exhausted Reserve cards of this
    /// type can be played (to `side`, when given)
    fn can_play_tapped(&self, seat: Seat, c: &Card, side: Option<Side>) -> bool {
        let kind = type_str(c);
        let additional = additional_types(c);
        for src in self.played_and_infinity(seat) {
            let flag = |key: &str| match self.card_prop(src, key) {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => self.attribute_condition(s, src).as_deref() == Some("1"),
                _ => false,
            };
            let play_tap = self.card_prop(src, "playTappedCards").filter(|v| v.as_object().is_some_and(|o| !o.is_empty()));
            let (characters, all) = (flag("playTappedCharacters"), flag("playTappedAllCards"));
            if !characters && !all && play_tap.is_none() {
                continue;
            }
            let tap_type = play_tap.and_then(|t| t.get("type")).and_then(Value::as_str);
            if tap_type == Some("all") || all {
                return true;
            }
            if let (Some(_), Some(side)) = (play_tap.and_then(|t| t.get("location")), side) {
                if self.card(src).zone != side.zone() {
                    continue;
                }
            }
            if characters && kind == "character" {
                return true;
            }
            if tap_type.is_some_and(|t| t == kind || additional.contains(&t)) {
                return true;
            }
        }
        false
    }

    /// Card::isExhaustedReservePlayBlocked: exhausted Reserve cards can't be played without
    /// Vaike or the like
    pub fn exhausted_reserve_blocked(&self, seat: Seat, id: CardId) -> bool {
        let c = self.card(id);
        c.zone == Zone::Reserve && c.exhausted && !self.can_play_tapped(seat, c, None)
    }

    /// Card::canBePlayed: the player can pay for the card (at its cheapest)
    pub fn can_be_played(&self, seat: Seat, id: CardId, scout: bool, reserve_flip: bool) -> bool {
        if self.exhausted_reserve_blocked(seat, id) {
            return false;
        }
        let c = self.card(id);
        let def = c.def();
        if let Some(cond) = def.prop("playCondition").and_then(Value::as_str).filter(|s| !s.is_empty()) {
            if !crate::conditions::check(self, cond, &Ctx::of(c.controller, Some(id))) {
                return false;
            }
        }
        let mut cost = self.card_cost(seat, id, scout, reserve_flip);
        let p = &self.players[seat];
        let if_empty = def.int("costReductionIfEmpty");
        if if_empty > 0 && (self.characters_in(seat, Side::Hero) == 0 || self.characters_in(seat, Side::Companion) == 0) && !self.has_gigantic(seat) {
            cost -= if_empty;
        }
        // The optional reductions count as taken. (With 1 Reserve card, the card itself from
        // Reserve, the original engine still counts the reduction.)
        let reserve = p.reserve.len();
        if def.int("costReductionDiscard") > 0 && reserve >= 1 {
            cost -= def.int("costReductionDiscard");
        }
        let ready = p.reserve.iter().filter(|&&x| !self.card(x).exhausted).count();
        if def.int("costReductionTap") > 0 && ready >= 1 {
            cost -= def.int("costReductionTap");
        }
        if def.int("costReductionSacrificePermanent") > 0 && self.in_play(seat).iter().any(|&x| type_str(self.card(x)) == "permanent") {
            cost -= def.int("costReductionSacrificePermanent");
        }
        cost -= def.int("costReductionLimitation").max(0);
        // Lyra DJ: in contact in either expedition (the original engine then offers both)
        match def.prop("playLimitation").and_then(Value::as_str) {
            Some("-2Contact") if self.in_contact_any(seat) => cost -= 2,
            Some("-2Multi") if self.opponent_has_characters(seat) => cost -= 2,
            _ => {}
        }
        cost <= self.mana_available(seat) && def.int("minManaOrbs") <= p.mana.len() as i32
    }

    /// Card::getPlayableLocation: where a card can be played (`forced`: only this expedition)
    pub fn playable_locations(&self, seat: Seat, id: CardId, forced: Option<Side>, free: bool) -> Vec<Dest> {
        let c = self.card(id);
        let def = c.def();
        let both = || vec![Dest::HeroExpedition, Dest::CompanionExpedition];
        let side_dest = |s: Side| if s == Side::Hero { Dest::HeroExpedition } else { Dest::CompanionExpedition };
        let forced_or_both = || forced.map(|s| vec![side_dest(s)]).unwrap_or_else(both);
        if def.has_subtype("landmark") {
            return if self.exhausted_reserve_blocked(seat, id) { vec![] } else { vec![Dest::Landmark] };
        }
        if type_str(c) == "spell" {
            return if self.exhausted_reserve_blocked(seat, id) { vec![] } else { vec![Dest::Spell] };
        }
        if c.zone == Zone::Reserve && c.exhausted {
            return [Side::Hero, Side::Companion]
                .into_iter()
                .filter(|&s| self.can_play_tapped(seat, c, Some(s)) && forced.map_or(true, |f| f == s))
                .map(side_dest)
                .collect();
        }
        let mana = self.mana_available(seat);
        let p = &self.players[seat];
        if def.int("costReductionIfEmpty") > 0 {
            // Affordable anyway: everywhere; else only in an expedition without Characters
            if self.card_cost(seat, id, false, false) <= mana {
                return forced_or_both();
            }
            return [Side::Hero, Side::Companion]
                .into_iter()
                .filter(|&s| self.characters_in(seat, s) == 0 && forced.map_or(true, |f| f == s))
                .map(side_dest)
                .collect();
        }
        match def.prop("playLimitation").and_then(Value::as_str).unwrap_or("") {
            "noOtherCardInHand" => {
                // Conditions::hasXCardsInHandExceptCurrentCard:0:LTE
                let others = p.hand.len() as i32 - (c.zone != Zone::Reserve) as i32;
                if others > 0 {
                    return vec![];
                }
                forced_or_both()
            }
            "controlFeat" => {
                if !self.in_play(seat).iter().any(|&x| self.card(x).def().has_subtype("feat")) {
                    return vec![];
                }
                forced_or_both()
            }
            "nonStartingRegion" => {
                let mut out = vec![];
                if p.position[0] != 0 {
                    out.push(Dest::HeroExpedition);
                }
                if p.position[1] != 7 {
                    out.push(Dest::CompanionExpedition);
                }
                out
            }
            "+3StartingRegion" => {
                if free {
                    return both();
                }
                let cost = self.card_cost(seat, id, false, false);
                let mut out = vec![];
                if p.position[0] != 0 || cost + 3 <= mana {
                    out.push(Dest::HeroExpedition);
                }
                if p.position[1] != 7 || cost + 3 <= mana {
                    out.push(Dest::CompanionExpedition);
                }
                out
            }
            // hasContactCostReduction ("hasOneContact" dynamic reductions) would check each
            // expedition, but getCost ignores the location: every expedition is offered
            _ => forced_or_both(),
        }
    }

    /// Card::getScoutableLocations
    fn scout_locations(&self, seat: Seat, id: CardId, forced: Option<Side>) -> Vec<Dest> {
        let scout_dest = |s: Side| if s == Side::Hero { Dest::HeroExpeditionScout } else { Dest::CompanionExpeditionScout };
        let all = |f: &dyn Fn(Side) -> bool| [Side::Hero, Side::Companion].into_iter().filter(|&s| f(s)).map(scout_dest).collect::<Vec<_>>();
        if self.card(id).def().int("costReductionIfEmpty") > 0 && self.card_cost(seat, id, true, false) > self.mana_available(seat) {
            return all(&|s| self.characters_in(seat, s) == 0 && forced.map_or(true, |f| f == s));
        }
        all(&|s| forced.map_or(true, |f| f == s))
    }

    /// Free plays only take the cost limits the effect sets
    /// (ChooseAssignment::cardMeetsExplicitFreeCostLimits)
    fn meets_free_cost_limits(&self, id: CardId, b: &BonusPlay) -> bool {
        let c = self.card(id);
        let hand = c.def().cost_hand;
        let base = if c.zone == Zone::Reserve { c.def().cost_reserve } else { hand };
        b.min_hand_cost.map_or(true, |m| hand >= m)
            && b.max_hand_cost.map_or(true, |m| hand <= m)
            && b.min_base_cost.map_or(true, |m| base >= m)
            && b.max_base_cost.map_or(true, |m| base <= m)
    }

    /// Cards a player can play now, with where they can go (ChooseAssignment::argsChooseAssignment;
    /// with `bonus.card`, PlayCard::argsPlayCard)
    pub fn playable(&self, seat: Seat, bonus: Option<&BonusPlay>) -> Vec<(CardId, Vec<Dest>)> {
        let p = &self.players[seat];
        let total_mana = p.mana.len() as i32;
        let free = bonus.map(|b| b.free).unwrap_or(false);
        let reserve_flip = bonus.map(|b| b.reserve_flip).unwrap_or(false);
        let forced = bonus.and_then(|b| b.forced);
        let mut out = vec![];
        // PlayCard: that card only, no Scout
        if let Some(id) = bonus.and_then(|b| b.card) {
            let ok = if free {
                !self.exhausted_reserve_blocked(seat, id) && self.card(id).def().int("minManaOrbs") <= total_mana
            } else {
                self.can_be_played(seat, id, false, false)
            };
            let dests = if ok { self.playable_locations(seat, id, None, free) } else { vec![] };
            if !dests.is_empty() {
                out.push((id, dests));
            }
            return out;
        }
        let type_ok = |c: &Card, additional: bool| match bonus {
            Some(b) if !b.types.is_empty() => {
                b.types.contains(&c.def().card_type)
                    || (additional && additional_types(c).iter().any(|t| b.types.iter().any(|bt| format!("{:?}", bt).to_lowercase() == *t)))
            }
            _ => true,
        };
        let subtype_ok = |c: &Card| bonus.map_or(true, |b| b.sub_types.is_empty() || b.sub_types.iter().any(|s| c.def().has_subtype(s)));
        for id in p.hand.iter().chain(p.reserve.iter()).copied() {
            let c = self.card(id);
            if !type_ok(c, true) || !subtype_ok(c) {
                continue;
            }
            let ok = match bonus.filter(|_| free) {
                None => self.can_be_played(seat, id, false, reserve_flip),
                Some(b) => c.def().int("minManaOrbs") <= total_mana && !self.exhausted_reserve_blocked(seat, id) && self.meets_free_cost_limits(id, b),
            };
            let mut dests = if ok { self.playable_locations(seat, id, forced, free) } else { vec![] };
            // Scout: from hand only
            if c.zone == Zone::Hand && c.def().int("scout") > 0 && type_ok(c, false) {
                let scout_ok = match bonus.filter(|_| free) {
                    None => self.can_be_played(seat, id, true, false),
                    Some(b) => self.meets_free_cost_limits(id, b),
                };
                if scout_ok {
                    dests.extend(self.scout_locations(seat, id, forced));
                }
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
        let def = self.card(id).def();
        let cost = if !free {
            let reserve_flip = bonus.map(|b| b.reserve_flip).unwrap_or(false);
            let cost = self.paid_cost_before_offer(seat, id, dest, reserve_flip);
            let offer = self.cost_reduction_offer(seat, id, dest, cost);
            let cost = if offer.is_some() { cost } else { cost - self.empty_expedition_reduction(seat, id, dest) };
            if offer.is_none() && cost > self.mana_available(seat) {
                return err("Not enough mana");
            }
            // The effects' cost reductions apply to one card
            let c = self.card(id);
            let used = |s: &Seat, kind: &String| *s == seat && (kind == "all" || is_type(c, kind) || def.has_subtype(kind));
            let reductions = self.mods.cost_reductions.iter().filter(|(s, k, _)| !used(s, k)).cloned().collect();
            let minimums = self.mods.cost_minimums.iter().filter(|(s, k, _)| !used(s, k)).cloned().collect();
            self.mods.cost_reductions = reductions;
            self.mods.cost_minimums = minimums;
            // Pay full price, or take the card's own reduction: the play happens after the choice
            if let Some(node) = offer {
                self.stack.push(Task::Run { node, ctx: Ctx::of(seat, Some(id)) });
                return Ok(());
            }
            cost.max(0)
        } else {
            bonus.and_then(|b| b.cost).unwrap_or(0).max(0)
        };
        if !self.pay(seat, cost) {
            return err("Not enough mana");
        }
        // Globals playedCards: free plays at no cost don't count, unless chosen as the turn's play
        let turn_play = bonus.map_or(true, |b| b.turn_play);
        if turn_play || !(free && cost == 0) {
            self.mods.played_count += 1;
        }

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
        self.stack.push(Task::PlayDone { turn_play });
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
        let limited = bonus.map(|b| b.limited).unwrap_or(false);
        add(def.effect(if limited { "effectPlayedLimited" } else { "effectPlayed" }));
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
