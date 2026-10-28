//! Static card data. Card definitions are exported from the original engine
//! (`server/tools/export_cards.php`): base characteristics are typed, effects are kept as the
//! original flow trees (JSON) and interpreted by `flow`.

use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardType {
    Hero,
    Character,
    Spell,
    Permanent,
    Token,
}

#[derive(Debug)]
pub struct CardDef {
    pub index: u16,
    pub reference: String,
    pub name: String,
    pub faction: String,
    pub card_type: CardType,
    pub subtypes: Vec<String>,
    pub rarity: u8,
    pub cost_hand: i32,
    pub cost_reserve: i32,
    /// forest, mountain, ocean
    pub stats: [i32; 3],
    pub is_token: bool,
    pub reserve_slots: i32,
    pub landmark_slots: i32,
    /// Every property of the original definition (effects, keywords and special flags)
    pub props: Map<String, Value>,
}

impl CardDef {
    pub fn prop(&self, key: &str) -> Option<&Value> {
        self.props.get(key).filter(|v| !v.is_null())
    }

    /// An effect tree ({J} effectPlayed, effectHand, effectReserve, effectSupport, effectTap...)
    pub fn effect(&self, key: &str) -> Option<&Value> {
        self.prop(key).filter(|v| match v {
            Value::Object(m) => !m.is_empty(),
            Value::Array(a) => !a.is_empty(),
            _ => false,
        })
    }

    pub fn flag(&self, key: &str) -> bool {
        match self.prop(key) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => true,
            None => false,
        }
    }

    pub fn int(&self, key: &str) -> i32 {
        self.prop(key).and_then(Value::as_i64).unwrap_or(0) as i32
    }

    pub fn has_subtype(&self, s: &str) -> bool {
        self.subtypes.iter().any(|t| t == s)
    }

    pub fn is_landmark_permanent(&self) -> bool {
        self.card_type == CardType::Permanent && !self.has_subtype("expedition")
    }
}

pub struct Catalog {
    pub cards: Vec<CardDef>,
    pub by_ref: HashMap<String, u16>,
    /// starter deck id => (card ref, count)
    pub starters: Vec<(String, Vec<(String, u32)>)>,
}

static CATALOG: OnceLock<Catalog> = OnceLock::new();

pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        Catalog::parse(
            include_str!("../data/cards.min.json"),
            include_str!("../data/starters.min.json"),
        )
    })
}

impl Catalog {
    pub fn parse(cards_json: &str, starters_json: &str) -> Catalog {
        let raw: Map<String, Value> = serde_json::from_str(cards_json).expect("cards.min.json");
        let mut cards = Vec::with_capacity(raw.len());
        let mut by_ref = HashMap::new();
        for (i, (reference, v)) in raw.into_iter().enumerate() {
            let props = match v {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            let s = |k: &str| props.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let n = |k: &str| props.get(k).and_then(Value::as_i64).unwrap_or(0) as i32;
            let is_token = props.get("token").and_then(Value::as_bool).unwrap_or(false);
            let card_type = match s("type").as_str() {
                "hero" => CardType::Hero,
                "spell" => CardType::Spell,
                "permanent" => CardType::Permanent,
                "token" => CardType::Token,
                _ => CardType::Character,
            };
            let subtypes = props
                .get("subtypes")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let def = CardDef {
                index: i as u16,
                name: s("name"),
                faction: s("faction"),
                card_type,
                subtypes,
                rarity: n("rarity") as u8,
                cost_hand: n("costHand"),
                cost_reserve: n("costReserve"),
                stats: [n("forest"), n("mountain"), n("ocean")],
                is_token,
                reserve_slots: if props.contains_key("reserveSlots") { n("reserveSlots") } else { 2 },
                landmark_slots: if props.contains_key("landmarkSlots") { n("landmarkSlots") } else { 2 },
                reference: reference.clone(),
                props,
            };
            by_ref.insert(reference, i as u16);
            cards.push(def);
        }
        let raw: Map<String, Value> = serde_json::from_str(starters_json).expect("starters.min.json");
        let starters = raw
            .into_iter()
            .map(|(id, list)| {
                let cards = list
                    .as_object()
                    .map(|m| m.iter().map(|(r, n)| (r.clone(), n.as_u64().unwrap_or(1) as u32)).collect())
                    .unwrap_or_default();
                (id, cards)
            })
            .collect();
        Catalog { cards, by_ref, starters }
    }

    pub fn get(&self, reference: &str) -> Option<&CardDef> {
        self.by_ref.get(reference).map(|&i| &self.cards[i as usize])
    }

    pub fn starter(&self, id: &str) -> Option<&Vec<(String, u32)>> {
        self.starters.iter().find(|(d, _)| d == id).map(|(_, l)| l)
    }
}
