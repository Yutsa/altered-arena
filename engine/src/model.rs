//! Game state. Everything is plain data (serializable): a game can be saved, restored for
//! undo, sent to another process, or run in WebAssembly.

use crate::catalog::{catalog, CardDef, CardType};
use serde::{Deserialize, Serialize};

pub type CardId = u32;
/// Index of a player in `Game::players` (0 or 1)
pub type Seat = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Zone {
    Hero,
    Hand,
    Deck,
    Reserve,
    Landmark,
    HeroExpedition,
    CompanionExpedition,
    Discard,
    Mana,
    Limbo,
    /// Tokens that left play, cards removed from the game
    Gone,
}

impl Zone {
    pub fn is_expedition(self) -> bool {
        matches!(self, Zone::HeroExpedition | Zone::CompanionExpedition)
    }
    /// Expeditions and landmarks
    pub fn in_play(self) -> bool {
        self.is_expedition() || self == Zone::Landmark
    }
    pub fn side(self) -> Option<Side> {
        match self {
            Zone::HeroExpedition => Some(Side::Hero),
            Zone::CompanionExpedition => Some(Side::Companion),
            _ => None,
        }
    }
    /// Name used by the original engine's card data (targetLocation...)
    pub fn from_engine(s: &str) -> Option<Zone> {
        Some(match s {
            "hand" => Zone::Hand,
            "reserve" => Zone::Reserve,
            "landmark" => Zone::Landmark,
            "stormLeft" => Zone::HeroExpedition,
            "stormRight" => Zone::CompanionExpedition,
            "discard" => Zone::Discard,
            "mana" => Zone::Mana,
            "limbo" => Zone::Limbo,
            "deck" | "topOfDeck" => Zone::Deck,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Hero,
    Companion,
}

impl Side {
    pub fn zone(self) -> Zone {
        match self {
            Side::Hero => Zone::HeroExpedition,
            Side::Companion => Zone::CompanionExpedition,
        }
    }
    pub fn index(self) -> usize {
        match self {
            Side::Hero => 0,
            Side::Companion => 1,
        }
    }
    pub fn other(self) -> Side {
        match self {
            Side::Hero => Side::Companion,
            Side::Companion => Side::Hero,
        }
    }
    pub const BOTH: [Side; 2] = [Side::Hero, Side::Companion];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Biome {
    Forest,
    Mountain,
    Ocean,
}

impl Biome {
    pub const ALL: [Biome; 3] = [Biome::Forest, Biome::Mountain, Biome::Ocean];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_engine(s: &str) -> Option<Biome> {
        match s {
            "forest" => Some(Biome::Forest),
            "mountain" => Some(Biome::Mountain),
            "ocean" => Some(Biome::Ocean),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Setup,
    Morning,
    Noon,
    Afternoon,
    Dusk,
    Night,
    Ended,
}

/// Status counters a card can gain (the original engine stores them as tokens on the card)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Fleeting,
    Anchored,
    Asleep,
}

impl Status {
    pub fn from_engine(s: &str) -> Option<Status> {
        match s {
            "fleeting" => Some(Status::Fleeting),
            "anchored" => Some(Status::Anchored),
            "asleep" => Some(Status::Asleep),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Status::Fleeting => "fleeting",
            Status::Anchored => "anchored",
            Status::Asleep => "asleep",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub def: u16,
    pub owner: Seat,
    /// Player who controls it (differs from owner after some effects)
    pub controller: Seat,
    pub zone: Zone,
    pub exhausted: bool,
    pub boosts: i32,
    pub statuses: Vec<Status>,
    /// Named counters (Scrap counter...), from effects such as incCounter
    pub counter: i32,
    /// Stat modifiers until the card leaves play
    pub stat_mods: [i32; 3],
    /// Zone it was played from (hand, reserve), while in play
    pub played_from: Option<Zone>,
    /// Feat completed (cards with effectCompleted)
    pub completed: bool,
}

impl Card {
    pub fn def(&self) -> &'static CardDef {
        &catalog().cards[self.def as usize]
    }
    pub fn has(&self, s: Status) -> bool {
        self.statuses.contains(&s)
    }
    pub fn add_status(&mut self, s: Status) {
        if !self.has(s) {
            self.statuses.push(s);
        }
    }
    pub fn remove_status(&mut self, s: Status) {
        self.statuses.retain(|x| *x != s);
    }
    pub fn card_type(&self) -> CardType {
        let d = self.def();
        if d.is_token {
            CardType::Token
        } else {
            d.card_type
        }
    }
    pub fn is_character_like(&self) -> bool {
        matches!(self.def().card_type, CardType::Character | CardType::Token) || self.def().is_token
    }
    /// Statistics with boosts and modifiers (never below 0)
    pub fn stats(&self) -> [i32; 3] {
        let base = self.def().stats;
        let mut out = [0; 3];
        for i in 0..3 {
            out[i] = (base[i] + self.boosts + self.stat_mods[i]).max(0);
        }
        out
    }
    pub fn name(&self) -> &'static str {
        &self.def().name
    }
    pub fn is_gigantic(&self) -> bool {
        self.def().flag("gigantic")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Player {
    pub id: u32,
    pub name: String,
    pub bot: bool,
    pub deck_id: Option<String>,
    pub hero: Option<CardId>,
    /// Ordered card ids per zone; the top of the deck is the last element
    pub hand: Vec<CardId>,
    pub deck: Vec<CardId>,
    pub reserve: Vec<CardId>,
    pub landmarks: Vec<CardId>,
    pub expeditions: [Vec<CardId>; 2],
    pub discard: Vec<CardId>,
    pub mana: Vec<CardId>,
    pub limbo: Vec<CardId>,
    /// Region index of the hero (0 → 7) and companion (7 → 0) expeditions
    pub position: [i32; 2],
    pub passed: bool,
    pub blocked: [bool; 2],
    /// Expedition moved forward this Dusk
    pub moved: [bool; 2],
}

impl Player {
    pub fn zone(&self, z: Zone) -> &Vec<CardId> {
        match z {
            Zone::Hand => &self.hand,
            Zone::Deck => &self.deck,
            Zone::Reserve => &self.reserve,
            Zone::Landmark => &self.landmarks,
            Zone::HeroExpedition => &self.expeditions[0],
            Zone::CompanionExpedition => &self.expeditions[1],
            Zone::Discard => &self.discard,
            Zone::Mana => &self.mana,
            Zone::Limbo | Zone::Hero | Zone::Gone => &self.limbo,
        }
    }
    pub fn zone_mut(&mut self, z: Zone) -> &mut Vec<CardId> {
        match z {
            Zone::Hand => &mut self.hand,
            Zone::Deck => &mut self.deck,
            Zone::Reserve => &mut self.reserve,
            Zone::Landmark => &mut self.landmarks,
            Zone::HeroExpedition => &mut self.expeditions[0],
            Zone::CompanionExpedition => &mut self.expeditions[1],
            Zone::Discard => &mut self.discard,
            Zone::Mana => &mut self.mana,
            Zone::Limbo | Zone::Hero | Zone::Gone => &mut self.limbo,
        }
    }
    /// Distance still to cover before the two expeditions meet (0 = they met)
    pub fn gap(&self) -> i32 {
        (self.position[1] - self.position[0]).max(0)
    }
}

/// One of the 5 cards of the Tumult track: 0 = hero region, 1..3 = tumult cards (2 regions each), 4 = companion region
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StormCard {
    pub card: u8,
    pub rotated: bool,
    pub revealed: bool,
}

/// Biomes of each region side of the storm cards (index = card id)
pub const STORM_CARDS: [[&[Biome]; 2]; 5] = [
    [&[], &[Biome::Forest, Biome::Mountain, Biome::Ocean]],
    [&[Biome::Forest], &[Biome::Mountain, Biome::Ocean]],
    [&[Biome::Mountain], &[Biome::Forest, Biome::Ocean]],
    [&[Biome::Ocean], &[Biome::Mountain, Biome::Forest]],
    [&[Biome::Forest, Biome::Mountain, Biome::Ocean], &[]],
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogLine {
    pub seq: u64,
    pub text: String,
    pub player: Option<Seat>,
}
