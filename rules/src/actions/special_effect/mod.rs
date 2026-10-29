//! Actions/SpecialEffect.php: the bespoke effects of cards, one `case` of `stSpecialEffect` each.
//!
//! The switch is split by theme: `state` (globals, counters, delayed effects), `boosts` (boost
//! counts), `flows` (flows for every player, mass effects, tokens), `reveal` (draws and
//! reveals). Each case is a function `(g, &mut Se) -> R<Out>`: `Out::Break` ends the case as
//! `break` does (then `resolveAction([])` runs), `Out::Done(r)` is a `return r` from
//! `stSpecialEffect` (the case resolved the action itself, or returns a value for takeAction).
//! PHP's `$card` is the source card (`Se::card`); cases that reuse `$card` as a loop variable
//! use a local instead, as nothing reads it after the loop.

mod boosts;
mod flows;
mod reveal;
mod state;

use crate::core::node::NodeId;
use crate::framework::ST_SPECIAL_EFFECT;
use crate::game::{CardId, Game, PId};
use crate::helpers::ft;
use crate::managers::cards;
use crate::models::action::{self, get_arg, get_ctx_args, get_source, ActionClass, Ctx};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct SpecialEffect;

/// The state of one `stSpecialEffect` run
pub(crate) struct Se {
    pub ctx: Ctx,
    /// `$effect`
    pub effect: String,
    /// `$args` (local copy: some cases change it)
    pub args: Value,
    /// `$card`: the source card
    pub card: Option<CardId>,
}

/// How a case ends
pub(crate) enum Out {
    /// `break`: `resolveAction([])` follows
    Break,
    /// `return $x` from `stSpecialEffect` (None for `return;`)
    Done(Option<Value>),
}

pub(crate) type Case = fn(&mut Game, &mut Se) -> R<Out>;

impl Se {
    /// `$card` used as an object
    pub fn card(&self) -> R<CardId> {
        self.card.ok_or_else(|| PhpError::new("Error: Call to a member function on null ($card in SpecialEffect)"))
    }

    /// `$this->getSourceId()`
    pub fn source_id(&self, g: &Game) -> R<Value> {
        action::get_source_id(g, self.ctx)
    }

    /// `$this->getCtxArg($k)`
    pub fn ctx_arg(&self, g: &Game, k: &str) -> Value {
        action::get_ctx_arg(g, self.ctx, k)
    }

    /// `$this->getArg($k)`
    pub fn arg(&self, g: &Game, k: &str) -> R<Value> {
        get_arg(&SpecialEffect, g, self.ctx, k)
    }

    /// `$this->getEvent()`
    pub fn event(&self, g: &Game) -> R<Value> {
        action::get_event(g, self.ctx)
    }

    pub fn node(&self) -> R<NodeId> {
        self.ctx.ok_or_else(|| PhpError::new("Error: Call to a member function toArray() on null"))
    }

    /// `$this->insertAsChild($flow)`
    pub fn insert(&self, g: &mut Game, flow: &Value) -> R<()> {
        action::insert_as_child(g, self.ctx, flow)
    }

    /// `$this->insertAsChild(['type' => NODE_SEQ, 'childs' => $nodes])`
    pub fn insert_seq(&self, g: &mut Game, nodes: Vec<Value>) -> R<()> {
        self.insert(g, &ft::seq(nodes))
    }

    /// `$this->pushParallelChilds($childs)`
    pub fn push_parallel(&self, g: &mut Game, childs: Vec<Value>) -> R<()> {
        action::push_parallel_childs(g, self.ctx, &Value::Array(childs))
    }

    /// `$this->checkAfterListeners($player, $args, $during, $overrideMethod)`
    pub fn after_listeners(&self, g: &mut Game, player: PId, args: Value, during: bool, override_method: Option<&str>) -> R<()> {
        action::check_after_listeners(&SpecialEffect, g, self.ctx, player, &args, during, override_method)
    }

    /// `$this->resolveAction($args)`
    pub fn resolve(&self, g: &mut Game, args: Value) -> R<()> {
        action::resolve_action(&SpecialEffect, g, self.ctx, args, json!(false))
    }

    /// `$this->getCard()`
    pub fn get_card(&self, g: &mut Game) -> R<CardId> {
        get_card(g, self.ctx)
    }
}

// ---------------------------------------------------------------------------
// Small shared helpers (PHP idioms of the cases)
// ---------------------------------------------------------------------------

/// `$card->getPlayer()` used as an object
pub(crate) fn player_of(g: &mut Game, c: CardId) -> R<PId> {
    card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function on null (card player)"))
}

/// A player used as an object (`Players::get(...)->...`)
pub(crate) fn req_player(p: Option<PId>) -> R<PId> {
    p.ok_or_else(|| PhpError::new("Error: Call to a member function on null (player)"))
}

/// `$card->getType()`
pub(crate) fn ctype(g: &mut Game, c: CardId) -> R<Value> {
    card::get(g, c, "type")
}

/// `in_array($c->getType(), $types)`
pub(crate) fn type_in(g: &mut Game, c: CardId, types: &[&str]) -> R<bool> {
    Ok(php::in_list(&ctype(g, c)?, types))
}

/// `in_array($x, $c->getSubtypes())`
pub(crate) fn has_subtype(g: &mut Game, c: CardId, x: &str) -> R<bool> {
    Ok(php::in_array(&json!(x), &card::get_subtypes(g, c)?))
}

/// `$c->isGigantic()` as a boolean
pub(crate) fn gigantic(g: &mut Game, c: CardId) -> R<bool> {
    Ok(php::truthy(&card::is_gigantic(g, c, false)?))
}

/// PHP `$a + $b` on numbers (ints stay ints)
pub(crate) fn add(a: &Value, b: &Value) -> Value {
    let is_float = |v: &Value| matches!(v, Value::Number(n) if n.is_f64()) || matches!(v, Value::String(s) if php::is_numeric_str(s) && (s.contains('.') || s.contains('e') || s.contains('E')));
    if is_float(a) || is_float(b) {
        json!(php::floatval(a) + php::floatval(b))
    } else {
        json!(php::intval(a) + php::intval(b))
    }
}

/// PHP `$a - $b` on numbers
pub(crate) fn sub(a: &Value, b: &Value) -> Value {
    let neg = match b {
        Value::Number(n) if n.is_f64() => json!(-n.as_f64().unwrap_or(0.0)),
        _ => json!(-php::intval(b)),
    };
    add(a, &neg)
}

/// `FT::GAIN($card, $token, $n, $upTo)` with the PHP defaults (n = 1, upTo = 99)
pub(crate) fn gain(card_id: Value, token: &str, n: Value) -> Value {
    ft::gain(card_id, token, n, json!(99))
}

pub(crate) const STORMS: [&str; 2] = ["stormLeft", "stormRight"];

/// `getCard()`: the card of `args.cardId` (`ME` = the source)
// PHP: Actions/SpecialEffect.php:321
pub(crate) fn get_card(g: &mut Game, ctx: Ctx) -> R<CardId> {
    let args = get_ctx_args(g, ctx);
    let mut card_id = php::coalesce(php::get(&args, "cardId"), &Value::Null).clone();
    if card_id.is_null() {
        return Err(PhpError::new("no card in args (special effect). Should not happen"));
    }
    if php::loose_eq(&card_id, &json!("me")) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
        card_id = json!(s);
    }
    cards::get(g, &card_id)
}

/// The case of an effect name (`switch ($effect)`, loose string comparison)
fn case_of(effect: &str) -> Option<Case> {
    if let Some(c) = state::case_of(effect) {
        return Some(c);
    }
    if let Some(c) = boosts::case_of(effect) {
        return Some(c);
    }
    if let Some(c) = flows::case_of(effect) {
        return Some(c);
    }
    reveal::case_of(effect)
}

/// `getDescription()` texts (only empty vs non-empty matters)
// PHP: Actions/SpecialEffect.php:24
fn description(effect: &str, args: &Value, ctx_args_args: &Value) -> Value {
    let s = match effect {
        "gainCounter" | "incCounter" => {
            return json!({
                "log": "Gain ${n} ${counter_name}",
                "args": {"n": php::get(ctx_args_args, "counter"), "counter_name": php::get(ctx_args_args, "counterName"), "i18n": ["counter_name"]},
            })
        }
        "nextCharacterGains1Boost" => "Next character <BOOST>",
        "nextReserveCharacterGains1Boost" => "Next character played from Reserve <BOOST>",
        "nextCharacterGains2Boost" => "Next character 2<BOOST>",
        "AuraqKibble" => "Draw and keep card or play it for free",
        "useCard" => "Flag the card",
        "costReduction" => "Reduce cost of next card",
        "activateAllPermanents" => "Activate all permanents",
        "activateAllOtherCharacters" => "Activate all other abilities",
        "nextSpellIsFree" => "Next spell is free",
        "nextCharacterCost3Anchored" | "nextCharacterAnchored" | "nextTokenAnchored" | "nextCharacterBaseCost3Anchored" => "Next character gains <ANCHORED>",
        "removeFleetingIfPlayedHand" => "Remove fleeting if played from hand",
        "removeFleetingSpellPlayed" => "Remove fleeting if next card is a spell",
        "removeFleetingCharacterPlayed" => "Remove fleeting if next card is a character",
        "removeFleetingIfSpellPlayedHand" => "Remove fleeting if next card is a spell played from hand",
        "boostAllSubtype" => "Boost all subtype",
        "boostAllCharacters" => "Boost all characters",
        "boostAllCharactersInExpedition" => "Boost all characters",
        "fleetingAllCharactersInExpedition" => "Fleeting all characters in target expedition",
        "boostAllCharactersExceptSelf" => "Boost all characters except me",
        "boostXReserve" => "Boost number of cards in reserve",
        "boostXLandmark" => "Boost number of landmarks",
        "boostXFeat" => "Boost number of feats",
        "boostXReserveAll" => "Boost number of all cards in reserve",
        "boost3Stat0" => "Boost if more than 3 0 statistics",
        "boost23Stat0" => "Boost if more than 3 0 statistics",
        "discardAllHand" => "Discard all hands",
        "discardAllHandReserve" => "Discard all hands and reserve",
        "discardAllReserve" => "Each player discards their Reserve",
        "instantWin" => "Immediate win",
        "MindApotheosis" => "Mind Apotheosis",
        "triggerEffectOfNextCharacter" => "Trigger ability of next character",
        "AfterRestSabotage" => "Sabotage after rest",
        "AfterRestOrdisRecruit" => "Invoke Ordis recruit after rest",
        "AfterRest2OrdisRecruit" => "Invoke 2 Ordis recruit after rest",
        "invokeOrdisRecruitBureaucrat" => "Invoke 1 Ordirs recruit for each Bureaucrat you control",
        "invokeManaMothPerStigmaInDiscard" => "Invoke 1 Mana Moth for each Stigma of Fallacy in your discard pile",
        "afterRest" => "Trigger the effect after rest",
        "AllPlayersSacrifice1" => "All players sacrifice 1 character",
        "eachPlayerOptionalResupply" => "All players may resupply",
        "eachPlayerResupply" => "All players resupply",
        "eachPlayerOptionalHandReserve" => "All players may put a card from their Hand in Reserve",
        "eachPlayerOptionalHandReserveDraw" => "All players may put a card from their Hand in Reserve to draw a card",
        "fleetingAllCharacters" => "All characters gain fleeting",
        "sleepingAllCharactersinExpedition" => "Put to sleep characters in the expedition",
        "boostXFleetingChar" => "1 Boost for each Fleeting character",
        "nextCharacterFleeting" => "Next character gains <FLEETING",
        "playAll1Card" => "All players play for free one card with hand cost {3} or less",
        "boostXForForest" => "1 Boost for each expedition in Forest",
        "eachPlayerAsleep" => "Each player target a Character, gain <ASLEEP>",
        "sendToReserveCharactersAndExhaustInExpedition" => "Send to reserve characters and exhaust them",
        "exhaustAllCards" => "Exhaust all cards in Reseve",
        "eachPlayerExhaust" => "Each player exhausts a card in reserve",
        "boostReserve" => "Characters in your Reserve gain 1 boost",
        "boostXBoostedChar" => "1 Boost for each Boosted character",
        "boostXAnchoredChar" => "1 Boost for each Anchored character",
        "boostXreserveBoost" => "For each boost, boost 1 character in reserve",
        "augmentXreserveBoost" => "For each boost, augment 1 card",
        "boostXMana" => "1 Boost for each Mana Orb",
        "boostedRevealBaseStat" => {
            if php::truthy(php::coalesce(php::get(args, "bypass"), &json!(false))) {
                "Draw a card and keep Character with a base statistic of 0"
            } else {
                "If boosted, draw a card and keep Character with a base statistic of 0"
            }
        }
        "boostedRevealArtistSong" => {
            if php::truthy(php::coalesce(php::get(args, "bypass"), &json!(false))) {
                "Draw a card and keep Artist or Song"
            } else {
                "If boosted, draw a card and keep Artist or Song"
            }
        }
        "boostedRevealRobotPermanent" => {
            if php::truthy(php::coalesce(php::get(args, "bypass"), &json!(false))) {
                "Draw a card and keep Robot or Permanent"
            } else {
                "If boosted, draw a card and keep Robot or Permanent"
            }
        }
        "ManInTheMaze" | "ManInTheMazeRare" | "ManInTheMazeYzmir" => "Remove all boost and resolve effect",
        "removeFleetingCharacterStat0Played" => "Remove fleeting if next card is a character with base statistic 0",
        "removeFleetingSongArtistPlayed" => "Remove fleeting if next card is an Artist or Song",
        "doubleBoosts" => "Double the boosts in reserve & expeditions",
        "counterPerCharacter" => "Gain 1 counter per Character you control",
        "boostAndRemoveFromExpedition" => "Gain 1 boost per character then remove characters",
        "RunesTestamentLook4" => "Look at the 4 first cards, keep 1 and discard the others",
        "invokeRecruitBehind" => "Invoke 1 Ordis recruit on each tied/behind expedition",
        "invokeBrassbugBehind" => "Invoke 1 Brassbug on each tied/behind expedition",
        "counterPerOpponentCharacter" => "1 boost per opponent controlled characters",
        "boostHandCards" => "Gain 1 boost per card in hand",
        "boostReserveCards" => "Gain 1 boost per card in reserve",
        "revealTopAndDraw" => "Reveal the top card. If cost = die, draw it",
        "revealTop" => "Reveal the top card.",
        "exhaustPlayFree" => "Exhaust Wayfarer and play card for free",
        "manInTheMazeUnique" => "Trigger Man in the maze unique effects",
        "hunger" => "Discard all other cards in play or in Reserve",
        "reveal" => "Reveal a card",
        "ascend" => "Ascend",
        "ascendOnLeave" => "Ascend when character is leaving expedition zone",
        "switchPlayer" => "Change First player",
        "allCharacterFleeting" => "All characters gain <FLEETING>",
        "allPass" => "All players pass",
        "boostTargetReserveCards" => "Gain 1 boost per card in reserve",
        "boostXOpponentExpedition" => "Gain 1 boost per character in expeditions facing card",
        "nextTokenAsleep" => "Token gains <ASLEEP>",
        "boostReserveSubtype" => "Each reserve of specific subtype gain 1 <BOOST>",
        "defect" => "defect",
        "drawReveal" => "Draw the reveal card",
        "boostXExhaustedMax3" => "Boost 1 per exhausted (max 3)",
        "copyGift" => "Copy the Gift",
        "boostXLandmarkMax3" => "Boost max up to 3",
        "sacrificeHighestCharacter" => "Sacrifice highest opponent character",
        "sacrificeHighestCharacterPermanent" => "Sacrifice highest opponent character or permanent",
        "boostXAnimalsMax2" => "1 Boost per other Animal (max 2)",
        "nextCharacterAsleep" => "Next character gains <ASLEEP>",
        "nextCharacterGains1BoostAndAsleep" => "Next character gains <BOOST> and <ASLEEP>",
        "boostXCompletedFeat" => "1 Boost for each Completed Feat in your Landmarks",
        "drawXCompletedFeat" => "Draw one card per Completed Feat",
        "nextAnimalGains1Boost" => "Next Animal gains <BOOST>",
        "playAnotherTurn" => "Play another turn",
        "boostXAnimalsInExpeditions" => "1 Boost per other Animal in expeditions",
        "discardTopDeck" => "Discard top card(s) of your deck",
        "swapBoostsWithSource" => "Exchange boosts with source character",
        "targetOpponentOptionalExhaustedResupply" => {
            if php::gt(php::coalesce(php::get(ctx_args_args, "n"), &json!(1)), &json!(1)) {
                "Target opponent may exhausted-resupply twice"
            } else {
                "Target opponent may exhausted-resupply"
            }
        }
        _ => "",
    };
    json!(s)
}

impl ActionClass for SpecialEffect {
    fn name(&self) -> &'static str {
        "SpecialEffect"
    }

    // PHP: Actions/SpecialEffect.php:334
    fn default_args(&self) -> Value {
        json!({"effect": null, "args": []})
    }

    // PHP: Actions/SpecialEffect.php:19
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_SPECIAL_EFFECT))
    }

    // PHP: Actions/SpecialEffect.php:24
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let effect = get_arg(self, g, ctx, "effect")?;
        let a = get_arg(self, g, ctx, "args")?;
        let args = if a.is_null() { php::arr() } else { a.clone() };
        Ok(description(&php::strval(&effect), &args, &a))
    }

    // PHP: Actions/SpecialEffect.php:275
    fn is_automatic(&self, _g: &mut Game, _ctx: Ctx, _player: Option<PId>) -> R<bool> {
        Ok(true)
    }

    /// Always false (the switch only has `return false` branches)
    // PHP: Actions/SpecialEffect.php:280
    fn is_independent(&self, g: &mut Game, ctx: Ctx, _player: Option<PId>) -> R<bool> {
        if let Some(source) = get_source(g, ctx)? {
            if php::loose_eq(&card::get(g, source, "rarity")?, &json!(2)) {
                return Ok(false);
            }
        }
        get_arg(self, g, ctx, "effect")?;
        Ok(false)
    }

    // PHP: Actions/SpecialEffect.php:336
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let effect = get_arg(self, g, ctx, "effect")?;
        let a = get_arg(self, g, ctx, "args")?;
        let args = if a.is_null() { php::arr() } else { a };
        let card = get_source(g, ctx)?;
        // `$effect` is compared loosely with the case strings: null and non strings match none
        let effect = match &effect {
            Value::String(s) => s.clone(),
            _ => String::new(),
        };
        let mut s = Se { ctx, effect, args, card };
        if let Some(case) = case_of(&s.effect) {
            if let Out::Done(r) = case(g, &mut s)? {
                return Ok(r);
            }
        }
        s.resolve(g, php::arr())?;
        Ok(None)
    }
}
