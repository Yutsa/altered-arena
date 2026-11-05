//! Managers/Cards.php: card instantiation, queries, moves, listeners and reactions

use crate::db::pieces::{self, Pieces};
use crate::game::{decode_props, CardId, CardRow, ClassData, Game, PId, PropMap, Props};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

// ---------------------------------------------------------------------------
// Card classes (modules/php/Cards/*, exported to rules/data/cards.min.json)
// ---------------------------------------------------------------------------

static CLASSES: OnceLock<HashMap<String, ClassData>> = OnceLock::new();

/// Class name → properties of the class
pub fn classes() -> &'static HashMap<String, ClassData> {
    CLASSES.get_or_init(|| {
        let data: Map<String, Value> = serde_json::from_str(include_str!("../../data/cards.min.json")).expect("cards.min.json");
        data.into_iter()
            .map(|(k, v)| {
                let mut v = v;
                php::normalize(&mut v);
                let m = match v {
                    Value::Object(m) => m.into_iter().collect(),
                    _ => Default::default(),
                };
                (k, Arc::new(m))
            })
            .collect()
    })
}

/// constants.inc.php DYNAMIC_PROPERTIES
pub const DYNAMIC_PROPERTIES: &[&str] = &["tapped", "extraDatas", "setIcon", "flavorText", "asset", "revealed", "subtypes", "typeline", "fullArt", "mainAsset"];

fn translit(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => "A",
        'è' | 'é' | 'ê' | 'ë' | 'ē' => "e",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' => "E",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' => "I",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ø' => "o",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ō' | 'Ø' => "O",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => "U",
        'ñ' => "n",
        'Ñ' => "N",
        'ç' => "c",
        'Ç' => "C",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'ý' | 'ÿ' => "y",
        _ => "",
    }
}

/// `slugify($text)` (Managers/Cards.php:16)
pub fn slugify(text: &str) -> String {
    let t = text.replace(['ō', 'ö'], "o").replace(['ā', 'ä'], "a");
    let t: String = t.chars().filter(|c| c.is_alphabetic() || c.is_ascii_digit()).collect();
    let mut out = String::new();
    for c in t.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            out.push_str(translit(c));
        }
    }
    let out: String = out.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    if out.is_empty() {
        "n-a".into()
    } else {
        out
    }
}

/// The class of a card from its DB properties (`<faction>_<Rarity>_<slug(name)>`)
pub fn class_name(p: &PropMap) -> String {
    let faction = php::strval(p.get("faction").unwrap_or(&Value::Null));
    let rarity = match php::intval(p.get("rarity").unwrap_or(&Value::Null)) {
        0 => "Common",
        1 => "Rare",
        2 => "Unique",
        3 => "Exalted",
        _ => "Common",
    };
    let name = php::strval(p.get("name").unwrap_or(&Value::Null));
    format!("{}_{}_{}", faction, rarity, slugify(&name))
}

static OWN_ID: OnceLock<HashMap<String, Vec<Vec<String>>>> = OnceLock::new();

/// Class name → paths of the properties built from the card's own id (`FT::GAIN/AUGMENT/LOOSE($this)`
/// give `$card->getId() ?? ME`, `'sourceId' => $this->getId()`): the class data was exported from an
/// object without id (`me` or null there), a card loaded from the DB has its id
fn own_id_paths() -> &'static HashMap<String, Vec<Vec<String>>> {
    OWN_ID.get_or_init(|| {
        let data: HashMap<String, Vec<String>> = serde_json::from_str(include_str!("../../data/card_own_id.json")).expect("card_own_id.json");
        data.into_iter().map(|(k, v)| (k, v.iter().map(|p| p.split('/').map(String::from).collect()).collect())).collect()
    })
}

fn set_path(v: &mut Value, path: &[String], x: Value) {
    let Some((first, rest)) = path.split_first() else {
        *v = x;
        return;
    };
    let next = match v {
        Value::Object(m) => m.get_mut(first),
        Value::Array(a) => first.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
        _ => None,
    };
    if let Some(n) = next {
        set_path(n, rest, x);
    }
}

/// `Cards::getCardInstance($id, $data)`: the object's properties (class data built with the card's
/// id + DYNAMIC_PROPERTIES of the DB JSON, or the DB JSON for Uniques)
// PHP: Managers/Cards.php:51
pub fn instantiate_props(db: &Props, id: CardId) -> R<Props> {
    if php::loose_eq(db.get("rarity").unwrap_or(&Value::Null), &json!(2)) {
        return Ok(db.clone());
    }
    let name = class_name(db);
    let (class_key, class) = match classes().get_key_value(&name) {
        Some((k, c)) => (k.clone(), c.clone()),
        None => {
            // slugify transliteration differs from iconv for some names: same faction, rarity and name
            let found = classes().iter().find(|(_, c)| {
                c.get("faction") == db.get("faction")
                    && php::loose_eq(c.get("rarity").unwrap_or(&Value::Null), db.get("rarity").unwrap_or(&Value::Null))
                    && c.get("name") == db.get("name")
            });
            match found {
                Some((k, c)) => (k.clone(), c.clone()),
                None => return Err(PhpError::new(format!("Card class {} does not exist (getCardInstance returns null)", name))),
            }
        }
    };
    let mut props = PropMap::with_base(class);
    // new $className($row): the constructor sees the card's id
    for path in own_id_paths().get(&class_key).into_iter().flatten() {
        if let Some((first, rest)) = path.split_first() {
            if let Some(v) = props.get_mut(first) {
                set_path(v, rest, json!(id));
            }
        }
    }
    for p in DYNAMIC_PROPERTIES {
        if let Some(v) = db.get(p) {
            if !v.is_null() {
                props.insert(p.to_string(), v.clone());
            }
        }
    }
    Ok(Arc::new(props))
}

/// A card row from the DB columns
pub fn card_from_row(id: i64, location: &str, state: i64, pid: i64, properties: &str) -> R<CardRow> {
    let db = Arc::new(PropMap::from_map(decode_props(properties)?));
    let props = instantiate_props(&db, id)?;
    Ok(CardRow { id, location: location.to_string(), state, pid, props, db_props: db, reload: false })
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

pub struct CardsT;

impl Pieces for CardsT {
    fn all_ids(g: &Game) -> Vec<i64> {
        g.cards.keys().copied().collect()
    }
    fn has(g: &Game, id: i64) -> bool {
        g.cards.contains_key(&id)
    }
    fn location_ref(g: &Game, id: i64) -> &str {
        &g.cards[&id].location
    }
    fn ids_at(g: &Game, location: &str) -> Option<Vec<i64>> {
        // An exact location: a string that is not numeric (loose == is then ===), without LIKE
        (!location.contains('%') && !php::is_numeric_str(location)).then(|| g.cards.ids_at(location))
    }
    fn state_value(g: &Game, id: i64) -> Value {
        json!(g.cards.get(&id).or_else(|| g.deleted_cards.get(&id)).map(|c| c.state).unwrap_or(0))
    }
    fn set_location(g: &mut Game, id: i64, loc: &str) -> R<()> {
        card::set_location(g, id, loc)
    }
    fn set_state(g: &mut Game, id: i64, state: i64) -> R<()> {
        card::set_state(g, id, state)
    }
    fn pid_value(g: &Game, id: i64) -> Value {
        json!(g.cards[&id].pid)
    }
    fn remove(g: &mut Game, id: i64) {
        if let Some(c) = g.cards.shift_remove(&id) {
            g.deleted_cards.insert(id, c);
        }
    }
    fn remove_many(g: &mut Game, ids: &[i64]) {
        if ids.len() < 16 {
            for id in ids {
                Self::remove(g, *id);
            }
            return;
        }
        // One pass keeping the order of the others, then the removed rows in the order asked
        let set: std::collections::HashSet<i64> = ids.iter().copied().collect();
        let mut removed: std::collections::HashMap<i64, CardRow> = Default::default();
        let old = std::mem::take(&mut g.cards);
        for (k, c) in old.into_rows() {
            if set.contains(&k) {
                removed.insert(k, c);
            } else {
                g.cards.insert(k, c);
            }
        }
        for id in ids {
            if let Some(c) = removed.remove(id) {
                g.deleted_cards.insert(*id, c);
            }
        }
    }
    fn reshuffle_custom() -> &'static [(&'static str, &'static str)] {
        &[("deck", "discard")]
    }
}

/// What `Cards::get($id)` returns: the card when exactly one matches, else a Collection
#[derive(Debug, Clone)]
pub enum Got {
    One(CardId),
    Many(Vec<CardId>),
}

impl Got {
    pub fn card(&self) -> Option<CardId> {
        match self {
            Got::One(c) => Some(*c),
            Got::Many(_) => None,
        }
    }
}

/// `Cards::get($id, $raise)`
// PHP: Helpers/CachedPieces.php:201
pub fn get_raw(g: &Game, id: &Value, raise: bool) -> R<Got> {
    let ids: Vec<Value> = if php::is_arr(id) { php::vals(id).into_iter().cloned().collect() } else { vec![id.clone()] };
    let r = pieces::get_many::<CardsT>(g, &ids, raise)?;
    if r.len() == 1 {
        Ok(Got::One(r[0]))
    } else {
        Ok(Got::Many(r))
    }
}

/// `Cards::get($id)` used as a card (throws when missing)
pub fn get(g: &Game, id: &Value) -> R<CardId> {
    match get_raw(g, id, true)? {
        Got::One(c) => Ok(c),
        Got::Many(_) => Err(PhpError::new("Error: Call to undefined method ALT\\Helpers\\Collection (Cards::get returned a collection)")),
    }
}

/// `Cards::getSingle($id, $raise)`
pub fn get_single(g: &Game, id: &Value, raise: bool) -> R<Option<CardId>> {
    let r = pieces::get_many::<CardsT>(g, std::slice::from_ref(id), raise)?;
    Ok(if r.len() == 1 { Some(r[0]) } else { None })
}

/// `Cards::getMany($ids, $raise)`
pub fn get_many(g: &Game, ids: &Value, raise: bool) -> R<Vec<CardId>> {
    let ids: Vec<Value> = if php::is_arr(ids) { php::vals(ids).into_iter().cloned().collect() } else { vec![ids.clone()] };
    pieces::get_many::<CardsT>(g, &ids, raise)
}

/// `Cards::getAll()`
pub fn get_all(g: &Game) -> Vec<CardId> {
    CardsT::all_ids(g)
}

/// `Cards::getInLocation($location, $state)`
pub fn get_in_location(g: &Game, location: &str, state: Option<i64>) -> R<Vec<CardId>> {
    pieces::get_in_location::<CardsT>(g, location, state)
}

/// `Cards::getInLocationOrdered($location)`
pub fn get_in_location_ordered(g: &Game, location: &str) -> R<Vec<CardId>> {
    let mut ids = get_in_location(g, location, None)?;
    pieces::order_by_state::<CardsT>(g, &mut ids, true);
    Ok(ids)
}

/// `Cards::countInLocation($location)`
pub fn count_in_location(g: &Game, location: &str) -> R<usize> {
    Ok(get_in_location(g, location, None)?.len())
}

/// `Cards::getTopOf($location, $n)`
pub fn get_top_of(g: &Game, location: &str, n: usize) -> R<Vec<CardId>> {
    pieces::get_top_of::<CardsT>(g, location, n)
}

/// `where_location(get_all(), $value)` through the location index, when the value only matches
/// equal strings: a non-numeric string without `%`, or an array of such strings
fn indexed_location(g: &Game, value: &Value) -> Option<Vec<CardId>> {
    fn exact(v: &Value) -> Option<&str> {
        match v {
            Value::String(s) if !php::is_numeric_str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    match value {
        Value::String(s) if !s.contains('%') => exact(value).map(|l| g.cards.ids_at(l)),
        Value::Array(a) => a.iter().map(exact).collect::<Option<Vec<&str>>>().map(|ls| g.cards.ids_at_any(&ls)),
        Value::Object(o) => o.values().map(exact).collect::<Option<Vec<&str>>>().map(|ls| g.cards.ids_at_any(&ls)),
        _ => None,
    }
}

/// `Collection::where('location', $value)` on cards (array value = in_array, `%` = LIKE)
pub fn where_location(g: &Game, ids: Vec<CardId>, value: &Value) -> Vec<CardId> {
    if value.is_null() {
        return ids;
    }
    let is_arr = php::is_arr(value);
    let like = if is_arr { None } else { Some(php::strval(value)).filter(|v| v.contains('%')) };
    ids.into_iter()
        .filter(|id| {
            let l = g.cards.get(id).or_else(|| g.deleted_cards.get(id)).map(|c| c.location.as_str()).unwrap_or("");
            if is_arr {
                php::in_array_str(l, value)
            } else if let Some(v) = &like {
                php::like_match(v, l)
            } else {
                php::loose_eq_str(l, value)
            }
        })
        .collect()
}

/// `Collection::where($field, $value)` on cards with a property getter
pub fn where_prop(g: &mut Game, ids: Vec<CardId>, field: &str, value: &Value) -> R<Vec<CardId>> {
    if value.is_null() {
        return Ok(ids);
    }
    let mut out = vec![];
    for id in ids {
        let ov = card::get_field(g, id, field)?;
        let keep = if php::is_arr(value) {
            php::in_array(&ov, value)
        } else if php::is_arr(&ov) {
            php::in_array(value, &ov)
        } else if php::strval(value).contains('%') {
            php::like_match(&php::strval(value), &php::strval(&ov))
        } else {
            php::loose_eq(&ov, value)
        };
        if keep {
            out.push(id);
        }
    }
    Ok(out)
}

/// `Collection::whereType($field, $value, $additionalType)`
pub fn where_type(g: &mut Game, ids: Vec<CardId>, field: &str, value: &Value, additional: bool) -> R<Vec<CardId>> {
    if value.is_null() {
        return Ok(ids);
    }
    let mut out = vec![];
    for id in ids {
        let mut ov = card::get_field(g, id, field)?;
        if additional {
            let at = card::get(g, id, "additionalType")?;
            if php::is_arr(value) && php::is_arr(&at) {
                // count(array_intersect($value, array_merge([$ov], $at))) > 0, without the merge
                if php::intersects(value, &std::iter::once(&ov).chain(php::vals(&at)).collect::<Vec<_>>()) {
                    out.push(id);
                }
                continue;
            }
            ov = php::array_merge(&[&json!([ov]), &at]);
        }
        let keep = if php::is_arr(value) {
            if php::is_arr(&ov) {
                php::intersects(value, &php::vals(&ov))
            } else {
                php::in_array(&ov, value)
            }
        } else if php::is_arr(&ov) {
            php::in_array(value, &ov)
        } else if php::strval(value).contains('%') {
            php::like_match(&php::strval(value), &php::strval(&ov))
        } else {
            php::loose_eq(&ov, value)
        };
        if keep {
            out.push(id);
        }
    }
    Ok(out)
}

/// `Cards::getFiltered($pId, $location, $type, $additionalType)`
// PHP: Managers/Cards.php:319
pub fn get_filtered(g: &mut Game, pid: &Value, location: &Value, ty: &Value, additional: bool) -> R<Vec<CardId>> {
    let ids = match indexed_location(g, location) {
        Some(ids) => ids,
        None => where_location(g, get_all(g), location),
    };
    // `where('pId', $pId)`: an array value is `in_array`
    let ids: Vec<CardId> = if pid.is_null() {
        ids
    } else if php::is_arr(pid) {
        ids.into_iter().filter(|id| php::in_array(&json!(g.cards[id].pid), pid)).collect()
    } else {
        ids.into_iter().filter(|id| php::loose_eq(&json!(g.cards[id].pid), pid)).collect()
    };
    where_type(g, ids, "type", ty, additional)
}

/// `orderBy('state', 'ASC')` on cards
pub fn order_by_state(g: &Game, ids: &mut [CardId], asc: bool) {
    pieces::order_by_state::<CardsT>(g, ids, asc);
}

/// IN_PLAY
pub fn in_play() -> Value {
    json!(["stormLeft", "stormRight", "landmark"])
}

/// `Cards::getPlayedCards($pId, $type)`
// PHP: Managers/Cards.php:822
pub fn get_played_cards(g: &mut Game, pid: &Value, ty: &Value) -> R<Vec<CardId>> {
    let mut ids = get_filtered(g, pid, &in_play(), ty, false)?;
    order_by_state(g, &mut ids, true);
    Ok(ids)
}

/// `Cards::getReserveCards($pId, $type)`
pub fn get_reserve_cards(g: &mut Game, pid: &Value, ty: &Value) -> R<Vec<CardId>> {
    get_filtered(g, pid, &json!("reserve"), ty, false)
}

/// `Cards::getStormCards($pId, $type)`
pub fn get_storm_cards(g: &mut Game, pid: &Value, ty: &Value) -> R<Vec<CardId>> {
    get_filtered(g, pid, &json!("storm%"), ty, false)
}

/// `Cards::getHand($pId, $type)`
pub fn get_hand(g: &mut Game, pid: &Value, ty: &Value) -> R<Vec<CardId>> {
    let mut ids = get_filtered(g, pid, &json!("hand"), ty, false)?;
    order_by_state(g, &mut ids, true);
    Ok(ids)
}

/// `Cards::getManaChoice($pId)`
pub fn get_mana_choice(g: &mut Game, pid: &Value) -> R<Vec<CardId>> {
    let mut ids = get_filtered(g, pid, &json!("choice"), &Value::Null, false)?;
    order_by_state(g, &mut ids, true);
    Ok(ids)
}

/// `Cards::hasPlayedCard($pId, $id)`
pub fn has_played_card(g: &mut Game, pid: PId, id: &Value) -> R<bool> {
    let c = get_single(g, id, false)?;
    match c {
        Some(c) => Ok(card::is_played(g, c)? && g.card(c)?.pid == pid),
        None => Ok(false),
    }
}

/// `Cards::getNextPlayedState()`
pub fn get_next_played_state(g: &mut Game) -> R<i64> {
    let mut max = -1;
    for id in get_filtered(g, &Value::Null, &in_play(), &Value::Null, false)? {
        max = max.max(g.card(id)?.state);
    }
    Ok(max)
}

/// `Cards::move($ids, $location, $state)`
pub fn move_cards(g: &mut Game, ids: &Value, location: &str, state: i64) -> R<Vec<CardId>> {
    let ids: Vec<Value> = if php::is_arr(ids) { php::vals(ids).into_iter().cloned().collect() } else { vec![ids.clone()] };
    pieces::move_pieces::<CardsT>(g, &ids, location, state)
}

/// `Cards::discard($cardIds, $discard, $pId)` (a move)
pub fn discard(g: &mut Game, ids: &Value, location: &str, pid: Option<PId>) -> R<Vec<CardId>> {
    let loc = match pid {
        Some(p) => format!("{}_{}", location, p),
        None => location.to_string(),
    };
    move_cards(g, ids, &loc, 0)
}

/// `Cards::insertOnTop($id, $location)`
pub fn insert_on_top(g: &mut Game, id: &Value, location: &str) -> R<()> {
    pieces::insert_on_top::<CardsT>(g, id, location)
}

/// `Cards::insertAtBottom($id, $location)`
pub fn insert_at_bottom(g: &mut Game, id: &Value, location: &str) -> R<()> {
    pieces::insert_at_bottom::<CardsT>(g, id, location)
}

/// `Cards::moveAtPosition($id, $location, $position)`
pub fn move_at_position(g: &mut Game, id: CardId, location: &str, position: i64) -> R<()> {
    pieces::move_at_position::<CardsT>(g, id, location, position)
}

/// `Cards::moveAllInLocation($from, $to, $fromState, $toState)`
pub fn move_all_in_location(g: &mut Game, from: &str, to: &str, from_state: Option<i64>, to_state: Option<i64>) -> R<Vec<CardId>> {
    pieces::move_all_in_location::<CardsT>(g, from, to, from_state, to_state)
}

/// `Cards::pickForLocation($nbr, $from, $to, $state, $deckReform = false)` (Cards overrides the
/// default to false: draws never reshuffle)
// PHP: Managers/Cards.php:814
pub fn pick_for_location(g: &mut Game, nbr: usize, from: &str, to: &str, state: i64, deck_reform: bool) -> R<Vec<CardId>> {
    pieces::pick_for_location::<CardsT>(g, nbr, from, to, state, deck_reform, &|g, loc| shuffle_deck(g, loc))
}

/// `Cards::reformDeckFromDiscard($from)`
pub fn reform_deck_from_discard(g: &mut Game, from: &str) -> R<()> {
    pieces::reform_deck_from_discard::<CardsT>(g, from, &|g, loc| shuffle_deck(g, loc))
}

/// `Cards::shuffle($location)`
pub fn shuffle(g: &mut Game, location: &str) -> R<()> {
    pieces::shuffle::<CardsT>(g, location)
}

/// `Cards::shuffleDeck($location)`: the deck reform listener (checkpoint + notifications)
// PHP: Managers/Cards.php:806
pub fn shuffle_deck(g: &mut Game, location: &str) -> R<()> {
    crate::core::engine::checkpoint(g)?;
    let n = count_in_location(g, location)?;
    g.notify("shuffleDeck", json!({"location": location, "n": n}));
    Ok(())
}

/// `Cards::delete($ids)`
pub fn delete(g: &mut Game, ids: &Value) -> R<Vec<CardId>> {
    let ids: Vec<Value> = if php::is_arr(ids) { php::vals(ids).into_iter().cloned().collect() } else { vec![ids.clone()] };
    pieces::delete::<CardsT>(g, &ids)
}

/// `createDeck($player, $deckContent)`: the hero on the board, every card of the content (the
/// hero again included) in `deck-API`; tokens are not created. Returns the hero's faction.
// PHP: Managers/Cards.php:763
pub fn create_deck(g: &mut Game, pid: PId, deck_content: &Value) -> R<String> {
    let hero_props = php::get(php::get(php::get(deck_content, "hero"), "card"), "properties").clone();
    let mut to_create = vec![json!({"player_id": pid, "location": format!("board-hero-{}", pid), "nbr": 1, "properties": hero_props})];
    let faction = php::strval(php::get(&hero_props, "faction"));
    for info in php::vals(deck_content) {
        // new Card($cardInfo['card']): properties as given, isToken() reads `token`
        let props = php::get(php::get(info, "card"), "properties").clone();
        if php::truthy(php::get(&props, "token")) {
            continue;
        }
        to_create.push(json!({"player_id": pid, "location": "deck-API", "nbr": php::get(info, "n"), "properties": props}));
    }
    create(g, &Value::Array(to_create), None)?;
    Ok(faction)
}

// ---------------------------------------------------------------------------
// Card references (deck lists of the publisher API)
// ---------------------------------------------------------------------------

/// refs.min.json: `refs` (MAP_REFS_CLASSES: reference → "DIR/Class"), `manualAltArt`
/// (MANUAL_ALT_ART), `altArt` (Cards::getAltArt), `serialIllustrations` (the FACTION/UID of the
/// illustrations whose UID ends with _NNN or _XXX)
pub fn refs_data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../../data/refs.min.json")).expect("refs.min.json"))
}

/// A card object built from a reference, not in the database (`Cards::getCardClass`): its class
/// and its properties
#[derive(Clone, Debug)]
pub struct CardObject {
    pub class: String,
    pub props: Map<String, Value>,
}

impl CardObject {
    /// The magic getter of a property: its value, else the default of its type
    fn get_or(&self, name: &str, default: Value) -> Value {
        match self.props.get(name) {
            Some(v) if !v.is_null() => v.clone(),
            _ => default,
        }
    }
    pub fn get_rarity(&self) -> Value {
        self.get_or("rarity", json!(0))
    }
    pub fn get_asset(&self) -> Value {
        self.get_or("asset", json!(""))
    }
    pub fn get_main_asset(&self) -> Value {
        self.get_or("mainAsset", json!(""))
    }
    pub fn get_full_art(&self) -> Value {
        self.get_or("fullArt", json!(false))
    }
    /// `isToken()`
    pub fn is_token(&self) -> bool {
        self.props.get("token").is_some_and(php::truthy)
    }
    /// `setProperty($name, $value)` (the setters of the properties)
    pub fn set(&mut self, name: &str, v: Value) {
        self.props.insert(name.into(), v);
    }
    /// `jsonSerialize()` of a card that is not in the database
    pub fn json(&self) -> Value {
        json!({"id": null, "location": null, "state": null, "pId": null, "properties": Value::Object(self.props.clone())})
    }
}

/// `$expUid[$i] = $v` on the parts of a uid (a missing part is added at the end)
fn set_part(parts: &mut Vec<String>, i: usize, v: &str) {
    match parts.get_mut(i) {
        Some(p) => *p = v.into(),
        None => parts.push(v.into()),
    }
}

/// `Cards::isAlternateArt($uid)`
// PHP: Managers/Cards.php:94
pub fn is_alternate_art(uid: &str) -> bool {
    let p = php::explode("_", uid);
    let part = |i: usize| p.get(i).map(String::as_str);
    part(2) == Some("A")
        || part(1).is_some_and(|s| ["DUSTERTOP", "DUSTERCB", "DUSTEROP", "TCS3", "WCS25", "MUSUBI"].contains(&s))
        || part(2) == Some("P")
        || php::isset(php::get(refs_data(), "manualAltArt"), uid)
}

/// `Cards::getMainUid($uid)`: the reference of the base print
// PHP: Managers/Cards.php:144
pub fn get_main_uid(uid: &str) -> String {
    let manual = php::get(php::get(refs_data(), "manualAltArt"), uid);
    if !manual.is_null() {
        return php::strval(manual);
    }
    let mut p = php::explode("_", uid);
    let set = p.get(1).cloned().unwrap_or_default();
    if ["DUSTEROP", "DUSTERCB", "DUSTERTOP"].contains(&set.as_str()) {
        let number = p.get(4).map(|s| json!(s)).unwrap_or(Value::Null);
        let new = if php::lt(&number, &json!(25)) {
            "CORE"
        } else if php::lt(&number, &json!(45)) {
            "ALIZE"
        } else if php::lt(&number, &json!(85)) {
            "CYCLONE"
        } else {
            "DUSTER"
        };
        set_part(&mut p, 1, new);
    } else if set == "TCS3" {
        set_part(&mut p, 1, "BISE");
    } else if ["WCQ25", "WCS25", "WCF25", "MUSUBI", "JUDGE"].contains(&set.as_str()) {
        set_part(&mut p, 1, "CORE");
    } else if set == "WCS26" {
        set_part(&mut p, 1, "DUSTER");
    } else if ["EOLETOP", "EOLECB", "EOLEOP"].contains(&set.as_str()) {
        set_part(&mut p, 1, "EOLE");
    }
    set_part(&mut p, 2, "B");
    p.join("_")
}

/// `Cards::getAlternateUid($uid)`: the reference without rarity (and serial)
// PHP: Managers/Cards.php:175
pub fn get_alternate_uid(uid: &str) -> String {
    let mut parts: Vec<Option<String>> = php::explode("_", uid).into_iter().map(Some).collect();
    if parts.len() == 7 {
        parts[6] = None;
    }
    if let Some(p) = parts.get_mut(5) {
        *p = None;
    }
    parts.into_iter().flatten().collect::<Vec<_>>().join("_")
}

/// `Cards::getIconSet($uid)`
// PHP: Managers/Cards.php:186
pub fn get_icon_set(uid: &str) -> Option<&'static str> {
    match php::explode("_", uid).get(1).map(String::as_str) {
        Some("COREKS") => Some("ks"),
        Some("ALIZE") => Some("tbf"),
        Some("BISE") => Some("wfm"),
        Some("CYCLONE") => Some("so"),
        Some("DUSTER") => Some("sdu"),
        Some("EOLE") => Some("roc"),
        _ => None,
    }
}

/// `Cards::illustrationExists($uid)`: whether img/illustrations/FACTION/UID.jpg is shipped (only
/// asked for serial prints, whose list is exported)
// PHP: Managers/Cards.php:134
pub fn illustration_exists(uid: &str) -> bool {
    let p = php::explode("_", uid);
    if p.len() < 4 {
        return false;
    }
    let faction = if p[3] == "OR" { "OD" } else { p[3].as_str() };
    let path = format!("{}/{}", faction, uid);
    php::vals(php::get(refs_data(), "serialIllustrations")).iter().any(|f| f.as_str() == Some(path.as_str()))
}

/// The serial suffix of a print (`/_(?:XXX|\d{3})$/`)
fn serial_suffix(uid: &str) -> Option<&str> {
    let i = uid.rfind('_')?;
    let s = &uid[i + 1..];
    (s == "XXX" || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))).then(|| &uid[i..])
}

/// `Cards::getCardClass($uid)`: the card of a reference of the publisher API (alternate arts, KS
/// prints and serial copies resolved to their class, with their own art)
// PHP: Managers/Cards.php:207
pub fn get_card_class(uid: &str) -> R<CardObject> {
    let refs = php::get(refs_data(), "refs");
    let alt_arts = php::get(refs_data(), "altArt");
    // Serialized prints sent by the deck API (numbered copies e.g.
    // ALT_DUSTERCB_P_AX_85_C_001 ... _030, and their ..._XXX placeholder) each
    // carry their own art: strip the serial to resolve the base card identity,
    // then restore it to the asset below so the per-copy image is used.
    let serial = serial_suffix(uid).map(String::from);
    let serial_base_uid = match &serial {
        Some(s) => uid[..uid.len() - s.len()].to_string(),
        None => String::new(),
    };
    let mut uid = if serial.is_some() { serial_base_uid.clone() } else { uid.to_string() };
    let orig_uid = uid.clone();

    let ks = is_ks(&uid);
    let alternate = is_alternate_art(&uid);
    let mut alt_uid = String::new();
    let mut core_uid = String::new();
    if ks {
        core_uid = get_core_uid(&uid);
        alt_uid = get_alt_uid(&uid);
    } else if alternate {
        core_uid = get_main_uid(&uid);
        alt_uid = get_alternate_uid(&uid);
    }

    if !php::isset(refs, &uid) {
        if !php::isset(refs, &core_uid) {
            return Err(PhpError::new(format!("This card is not implemented {} {} {}t", uid, core_uid, alt_uid)));
        } else if ks || alternate {
            uid = core_uid.clone();
        }
    }

    let class_path = php::strval(php::get(refs, &uid));
    let class = php::explode("/", &class_path).get(1).cloned().unwrap_or_default();
    let props = classes().get(&class).ok_or_else(|| PhpError::new(format!("Error: Class \"\\ALT\\Cards\\{}\" not found", class_path.replace('/', "\\"))))?;
    let mut card = CardObject { class, props: props.iter().map(|(k, v)| (k.clone(), v.clone())).collect() };
    if let Some(icon_set) = get_icon_set(&uid) {
        card.set("setIcon", json!(icon_set));
    }

    if ks || alternate {
        let mut alt_art = php::get(alt_arts, &alt_uid).clone();
        // Standard alternate-art prints (the A/B art variants, e.g. ALT_CORE_A_BR_31_C)
        // with no specific override data still carry their own art.
        if alt_art.is_null()
            && alternate
            && (php::explode("_", &alt_uid).get(2).is_some_and(|s| s == "A") || php::isset(php::get(refs_data(), "manualAltArt"), &orig_uid))
        {
            alt_art = php::arr();
        }
        if !alt_art.is_null() {
            if php::isset(&alt_art, "flavorText") {
                card.set("flavorText", php::get(&alt_art, "flavorText").clone());
            }
            let unsuffixed_alt_uid = alt_uid.clone();
            let rarity = card.get_rarity();
            let suffix = if php::loose_eq(&rarity, &json!(1)) {
                "_R"
            } else if php::loose_eq(&rarity, &json!(0)) {
                "_C"
            } else if php::loose_eq(&rarity, &json!(3)) {
                "_E"
            } else {
                "_U"
            };
            card.set("asset", json!(format!("{}{}", alt_uid, suffix)));
            alt_uid.push_str(suffix);

            let info = php::get(alt_arts, &unsuffixed_alt_uid);
            let rest = &alt_uid[unsuffixed_alt_uid.len()..];
            if php::isset(info, "mainAsset") {
                card.set("mainAsset", php::get(info, "mainAsset").clone());
            } else if php::isset(info, "framedBase") {
                card.set("mainAsset", json!(format!("{}{}", php::strval(php::get(info, "framedBase")), rest)));
            } else if php::isset(&alt_art, "fullArt") {
                card.set("mainAsset", json!(format!("{}{}", get_main_uid(&unsuffixed_alt_uid), rest)));
            } else {
                card.set("mainAsset", json!(alt_uid));
            }

            if php::isset(&alt_art, "fullArt") {
                card.set("fullArt", json!(true));
            }
        }
    }
    // Each serialized copy displays its own art (e.g. ALT_DUSTERCB_P_AX_85_C_014).
    if let Some(serial) = serial {
        let numbered = serial != "_XXX";
        let mut asset = format!("{}{}", serial_base_uid, serial);
        // The multi-panel plague art is not shipped anymore, so decklists still
        // referencing a copy number fall back on the single full-art _XXX print.
        if numbered && !illustration_exists(&asset) {
            let full_art_asset = format!("{}_XXX", serial_base_uid);
            if illustration_exists(&full_art_asset) {
                asset = full_art_asset;
            }
        }
        card.set("asset", json!(asset));
        if serial_base_uid.starts_with("ALT_DUSTERCB_P_") {
            card.set("mainAsset", json!(get_main_uid(&serial_base_uid)));
            card.set("fullArt", json!(true));
        }
        if numbered {
            card.set("serial", json!(serial.trim_start_matches('_')));
        }
    }
    Ok(card)
}

// ---------------------------------------------------------------------------
// Unique cards
// ---------------------------------------------------------------------------

/// `constant($name)` for the scalar constants of the engine (rules/data/constants.json); an
/// undefined constant throws
fn constant(name: &Value) -> R<Value> {
    static CONSTANTS: OnceLock<Map<String, Value>> = OnceLock::new();
    let consts = CONSTANTS.get_or_init(|| serde_json::from_str(include_str!("../../data/constants.json")).expect("constants.json"));
    let name = php::strval(name);
    consts.get(&name).cloned().ok_or_else(|| PhpError::new(format!("Undefined constant \"{}\"", name)))
}

/// `Cards::isKS($uid)`
// PHP: Managers/Cards.php:89
pub fn is_ks(uid: &str) -> bool {
    php::explode("_", uid).get(1).is_some_and(|s| s == "COREKS")
}

/// `Cards::getAltUid($uid)`: the KS uid without rarity and unique number
// PHP: Managers/Cards.php:109
pub fn get_alt_uid(uid: &str) -> String {
    let mut parts: Vec<Option<String>> = {
        let mut p = php::explode("_", uid);
        set_part(&mut p, 1, "COREKS");
        p.into_iter().map(Some).collect()
    };
    if parts.len() == 7 {
        parts[6] = None;
    }
    if let Some(p) = parts.get_mut(5) {
        *p = None;
    }
    parts.into_iter().flatten().collect::<Vec<_>>().join("_")
}

/// `Cards::getCoreUid($uid)`
// PHP: Managers/Cards.php:121
pub fn get_core_uid(uid: &str) -> String {
    let mut parts = php::explode("_", uid);
    set_part(&mut parts, 1, "CORE");
    parts.join("_")
}

/// `Utils::convertFaction($faction)`
// PHP: Helpers/Utils.php:315
fn convert_faction(faction: &Value) -> Value {
    match php::strval(faction).as_str() {
        "AX" => json!("AX"),
        "OR" => json!("OD"),
        "BR" => json!("BR"),
        "LY" => json!("LY"),
        "MU" => json!("MU"),
        "YZ" => json!("YZ"),
        _ => Value::Null,
    }
}

/// `Cards::generateUnique($unique)`: the properties of a Unique from its API content (decks-api
/// `/api/bga/decks/{id}`: reference, name, cardType, subTypes, typeline, mainFaction, illustrator,
/// elements, uniqueReduced). Null when an effect does not have its three ids.
// PHP: Managers/Cards.php:415
pub fn generate_unique(unique: &Value) -> R<Value> {
    use crate::helpers::flow_convertor;
    let mut properties = php::arr();
    php::set(&mut properties, "uid", php::get(unique, "reference").clone());
    let uid = php::strval(php::get(&properties, "uid"));
    php::set(&mut properties, "rarity", json!(2));

    let mut asset = php::explode("_", &uid);
    asset.pop();
    php::set(&mut properties, "asset", json!(asset.join("_")));
    if is_ks(&uid) {
        php::set(&mut properties, "setIcon", json!("ks"));

        let alt_uid = get_alt_uid(&uid);
        let alt_art = php::get(refs_data(), "altArt");
        if php::isset(alt_art, &alt_uid) {
            php::set(&mut properties, "flavorText", php::get(php::get(alt_art, &alt_uid), "flavorText").clone());
            php::set(&mut properties, "asset", json!(format!("{}_U", alt_uid)));
        } else {
            php::set(&mut properties, "asset", json!(format!("{}_U", get_core_uid(&alt_uid))));
        }
    }
    php::set(&mut properties, "faction", convert_faction(php::get(php::get(unique, "mainFaction"), "reference")));
    php::set(&mut properties, "name", php::get(unique, "name").clone());
    php::set(&mut properties, "type", constant(php::get(php::get(unique, "cardType"), "reference"))?);

    let mut subtypes = vec![];
    for sub in php::vals(php::get(unique, "subTypes")) {
        subtypes.push(constant(sub)?);
    }
    php::set(&mut properties, "subtypes", Value::Array(subtypes));
    php::set(&mut properties, "typeline", php::get(unique, "typeline").clone());
    php::set(&mut properties, "artist", php::get(unique, "illustrator").clone());

    let elements = php::get(unique, "elements");
    for (prop, element) in
        [("costHand", "MAIN_COST"), ("costReserve", "RECALL_COST"), ("forest", "FOREST_POWER"), ("mountain", "MOUNTAIN_POWER"), ("ocean", "OCEAN_POWER")]
    {
        php::set(&mut properties, prop, json!(php::intval(php::get(elements, element))));
    }

    // add effects
    php::set(&mut properties, "uEffects", php::arr());
    let flow = flow_convertor::data();
    for card_element in php::vals(php::get(unique, "uniqueReduced")) {
        let mut trinity = php::arr();
        for id_gd in php::vals(php::get(card_element, "effects")) {
            if php::in_array(id_gd, php::get(flow, "TRIGGER")) {
                php::set(&mut trinity, "trigger", id_gd.clone());
            } else if php::in_array(id_gd, php::get(flow, "CONDITION")) {
                php::set(&mut trinity, "condition", id_gd.clone());
            } else if php::in_array(id_gd, php::get(flow, "OUTPUT")) {
                php::set(&mut trinity, "output", id_gd.clone());
            }
        }
        if php::empty(&trinity) {
            continue;
        }
        if php::count(&trinity) != 3 {
            return Ok(Value::Null);
        }
        flow_convertor::construct_effect(&trinity, &mut properties)?;
        php::push(php::get_mut(&mut properties, "uEffects"), php::values(&trinity));
    }
    Ok(properties)
}

/// `Cards::create($pieces, $globalLocation, ...)`: new rows (properties stored as JSON), read
/// back from the DB as new objects
// PHP: Helpers/CachedPieces.php:519
pub fn create(g: &mut Game, pieces_v: &Value, global_location: Option<&str>) -> R<Vec<CardId>> {
    let mut pos = match global_location {
        Some(l) => pieces::get_extreme_position::<CardsT>(g, true, l)? + 1,
        None => 0,
    };
    let mut rows = vec![];
    for info in php::vals(pieces_v) {
        let n = php::intval(php::coalesce(php::get(info, "nbr"), &json!(1)));
        let start = php::intval(php::coalesce(php::get(info, "nbrStart"), &json!(0)));
        let location = match php::get(info, "location") {
            Value::Null => global_location.map(String::from),
            v => Some(php::strval(v)),
        };
        let state = php::get(info, "state").clone();
        let state = if state.is_null() {
            if location.as_deref() == global_location {
                let s = pos;
                pos += 1;
                s
            } else {
                0
            }
        } else {
            php::intval(&state)
        };
        let Some(location) = location else {
            return Err(PhpError::new("Class Pieces : create location cannot be null (set per token location or location_global"));
        };
        pieces::check_location(&location, false)?;
        for _ in start..n + start {
            let pid = php::intval(php::get(info, "player_id"));
            let props = match php::get(info, "properties") {
                Value::Null => "null".to_string(),
                v if php::is_arr(v) => serde_json::to_string(v).unwrap(),
                v => php::strval(v),
            };
            rows.push((location.clone(), state, pid, props));
        }
    }
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let mut ids = vec![];
    for (location, state, pid, props) in rows {
        let id = g.next_card_id;
        g.next_card_id += 1;
        let row = card_from_row(id, &location, state, pid, &props)?;
        g.cards.insert(id, row);
        ids.push(id);
    }
    Ok(ids)
}

/// `Cards::singleCreate($token)`
pub fn single_create(g: &mut Game, token: &Value) -> R<Option<CardId>> {
    Ok(create(g, &json!([token]), None)?.first().copied())
}

/// `Cards::untapAll()` (morning)
// PHP: Managers/Cards.php:865
pub fn untap_all(g: &mut Game) -> R<()> {
    let mut untapped = vec![];
    let exhausted_morning = crate::managers::players::is_exhausted_characters_morning(g)?;
    for id in get_all(g) {
        if card::is(g, id, "tapped")? {
            let ty = card::get(g, id, "type")?;
            let in_reserve = g.card(id)?.location == "reserve";
            if !exhausted_morning || (exhausted_morning && (!php::in_list(&ty, &["token", "character"]) || !in_reserve)) {
                card::set_property(g, id, "tapped", json!(false), true)?;
                untapped.push(id);
            }
        }
    }
    g.notify("untap", json!({"cardIds": untapped}));
    Ok(())
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// `Cards::getListeningCardsObject()`: expeditions, landmarks, reserve, heroes
// PHP: Managers/Cards.php:928
pub fn get_listening_cards_object(g: &Game) -> R<Vec<CardId>> {
    let mut out: Vec<CardId> = vec![];
    for loc in ["stormLeft", "stormRight", "landmark", "reserve", "board-hero%"] {
        for id in get_in_location(g, loc, None)? {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    }
    Ok(out)
}

/// `Cards::getListeningCards($event)`
// PHP: Managers/Cards.php:894
pub fn get_listening_cards(g: &mut Game, event: &Value) -> R<Vec<CardId>> {
    let mut cards = vec![];
    for id in get_listening_cards_object(g)? {
        if card::is_listening_to(g, id, event)? {
            cards.push(id);
        }
    }
    for key in ["cardsToListen", "reserveToListen"] {
        if php::isset(event, key) {
            let many = get_many(g, php::get(event, key), false)?;
            let mut add = vec![];
            for id in many {
                if !cards.contains(&id) && card::is_listening_to(g, id, event)? {
                    add.push(id);
                }
            }
            cards.extend(add);
        }
    }
    Ok(cards)
}

/// `Cards::getReaction($event, $returnNullIfEmpty, $ownerPId)`: ACTIVATE_CARD nodes of the
/// listening cards (null when none)
// PHP: Managers/Cards.php:940
pub fn get_reaction(g: &mut Game, event: &Value, return_null_if_empty: bool, owner_pid: bool) -> R<Value> {
    let listening = get_listening_cards(g, event)?;
    if listening.is_empty() && return_null_if_empty {
        return Ok(Value::Null);
    }
    let mut childs = vec![];
    for cid in listening {
        let mut ev = event.clone();
        let loc = g.card(cid)?.location.clone();
        if loc == "reserve" {
            let r = php::get_mut(&mut ev, "reserveToListen");
            php::push(r, json!(cid));
        }
        // The forceListening branch tests in_array($listenCard, STORMS): a Card object against
        // strings, always false (dead code)
        php::set(&mut ev, "sourceLocation", json!(loc));
        let pid = if owner_pid { json!(g.card(cid)?.pid) } else { php::get(&ev, "pId").clone() };
        let mut child = json!({
            "action": "ActivateCard",
            "pId": pid,
            "args": {"cardId": cid, "event": ev},
            "sourceId": cid,
        });
        let ev2 = php::get(php::get(&child, "args"), "event").clone();
        if card::is_immediate_reaction(g, cid, &ev2)? {
            php::set(&mut child, "immediate", json!(true));
        }
        childs.push(child);
    }
    if childs.is_empty() && return_null_if_empty {
        return Ok(Value::Null);
    }
    Ok(Value::Array(childs))
}

/// `Cards::applyEffect($card, $player, $methodName, $args)`: the card's reaction flow to an
/// event (null when it has none)
// PHP: Managers/Cards.php:1281
pub fn apply_effect(g: &mut Game, cid: CardId, _player: PId, _method: &Value, args: &Value) -> R<Value> {
    let mut nd = json!({"type": "seq", "optional": true, "childs": []});
    let (payment, output) = card::get_reactions(g, cid, args)?;
    if output.is_null() || php::empty(&output) {
        return Ok(Value::Null);
    }
    if !payment.is_null() && !php::empty(&payment) {
        // Utils::tagTree on the payment: its result is discarded
        let childs = php::get_mut(&mut nd, "childs");
        php::push(childs, payment.clone());
    }
    let output = crate::helpers::utils::tag_tree(&output, &json!({"sourceId": cid}), false);
    let output = crate::helpers::utils::bind_own_effect_activate_card_id(&output, cid);
    if payment.is_null() || php::empty(&payment) {
        return Ok(output);
    }
    let childs = php::get_mut(&mut nd, "childs");
    php::push(childs, output);
    Ok(nd)
}

/// `Cards::applyEffects($player, $methodName, $args)`: card classes have no `on<Method>`
/// methods, so no card is selected
pub fn apply_effects(_g: &mut Game, _player: PId, _method: &str, _args: &mut Value) -> R<bool> {
    Ok(false)
}

// ---------------------------------------------------------------------------
// Setup (Managers/Cards.php:664 setupPrecoDeck, Cards/cards.inc.php STARTER_DECKS)
// ---------------------------------------------------------------------------

/// `STARTER_DECKS` (Cards/cards.inc.php:90): deck id and faction, in the order of the constant
pub const STARTER_DECKS: &[(&str, &str)] = &[
    ("ax_1_sierra", "AX"),
    ("ax_2_treyst", "AX"),
    ("ax_3_subhash", "AX"),
    ("ax_65_isaree", "AX"),
    ("ax_85_della", "AX"),
    ("br_1_kojo", "BR"),
    ("br_2_atsadi", "BR"),
    ("br_3_basira", "BR"),
    ("br_65_sol", "BR"),
    ("br_105_gretel", "BR"),
    ("ly_1_nevenka", "LY"),
    ("ly_2_auraq", "LY"),
    ("ly_3_fen", "LY"),
    ("ly_65_nadir", "LY"),
    ("ly_105_yeonggi", "LY"),
    ("mu_1_teija", "MU"),
    ("mu_2_arjun", "MU"),
    ("mu_3_rin", "MU"),
    ("mu_65_kauri", "MU"),
    ("mu_85_turuun", "MU"),
    ("or_1_sigismar", "OD"),
    ("or_2_waru", "OD"),
    ("or_3_gulrang", "OD"),
    ("or_65_zhen", "OD"),
    ("or_85_matz", "OD"),
    ("yz_1_akesha", "YZ"),
    ("yz_2_lindiwe", "YZ"),
    ("yz_3_afanas", "YZ"),
    ("yz_65_moyo", "YZ"),
    ("yz_105_sam", "YZ"),
];

type StarterList = Vec<(String, i64)>;
static STARTERS: OnceLock<HashMap<String, StarterList>> = OnceLock::new();

/// The contents of a starter deck (`STARTER_<HERO>` of Cards/StarterDecks/<deck id>.inc.php,
/// exported to rules/data/starters.min.json): class name and number of copies, in order
pub fn starter_contents(deck_id: &str) -> &'static [(String, i64)] {
    let all = STARTERS.get_or_init(|| {
        let data: Map<String, Value> = serde_json::from_str(include_str!("../../data/starters.min.json")).expect("starters.min.json");
        data.into_iter()
            .map(|(k, v)| {
                let list = match v {
                    Value::Object(m) => m.into_iter().map(|(c, n)| (c, php::intval(&n))).collect(),
                    _ => vec![],
                };
                (k, list)
            })
            .collect()
    });
    all.get(deck_id).map(|v| v.as_slice()).unwrap_or(&[])
}

/// `new $className(null)`: the properties of a card class
pub fn class_props(class: &str) -> R<ClassData> {
    classes().get(class).cloned().ok_or_else(|| PhpError::new(format!("Error: Class \"{}\" not found", class)))
}

/// `Cards::setupPrecoDeck($player, $deckNumber, $deckList)`: every starter deck in the player's
/// `deck-<n>` locations (the chosen one is kept at deck setup)
// PHP: Managers/Cards.php:664
pub fn setup_preco_deck(g: &mut Game, pid: PId, deck_number: i64, deck_list: Value) -> R<Value> {
    let mut deck_list = deck_list;
    let mut deck_number = deck_number;
    let mut to_create = vec![];
    for (deck_id, faction) in STARTER_DECKS {
        for (class, n) in starter_contents(deck_id) {
            let card = class_props(class)?;
            let location = format!("deck-{}", deck_number);
            if card.get("type") == Some(&json!("hero")) {
                php::seti(&mut deck_list, deck_number, json!({"deckNumber": deck_number, "deckId": deck_id, "faction": faction}));
            }
            // we do not create token as they will be created on the fly
            if php::truthy(card.get("token").unwrap_or(&Value::Null)) {
                continue;
            }
            to_create.push(json!({
                "player_id": pid,
                "location": location,
                "nbr": n,
                "properties": {
                    "rarity": php::intval(card.get("rarity").unwrap_or(&Value::Null)),
                    "name": card.get("name").cloned().unwrap_or(Value::Null),
                    "faction": card.get("faction").cloned().unwrap_or(Value::Null),
                },
            }));
        }
        deck_number += 1;
    }
    create(g, &Value::Array(to_create), None)?;
    Ok(deck_list)
}
