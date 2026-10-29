//! Managers/Cards.php: card instantiation, queries, moves, listeners and reactions

use crate::db::pieces::{self, Pieces};
use crate::game::{decode_props, CardId, CardRow, Game, PId, Props};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

// ---------------------------------------------------------------------------
// Card classes (modules/php/Cards/*, exported to engine/data/cards.min.json)
// ---------------------------------------------------------------------------

static CLASSES: OnceLock<HashMap<String, Props>> = OnceLock::new();

/// Class name → properties of the class
pub fn classes() -> &'static HashMap<String, Props> {
    CLASSES.get_or_init(|| {
        let data: Map<String, Value> = serde_json::from_str(include_str!("../../../engine/data/cards.min.json")).expect("cards.min.json");
        data.into_iter()
            .map(|(k, v)| {
                let mut v = v;
                php::normalize(&mut v);
                let m = match v {
                    Value::Object(m) => m,
                    _ => Map::new(),
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
    let t = text.replace('ō', "o").replace('ö', "o").replace('ā', "a").replace('ä', "a");
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
pub fn class_name(p: &Map<String, Value>) -> String {
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
        let data: HashMap<String, Vec<String>> = serde_json::from_str(include_str!("../../../engine/data/card_own_id.json")).expect("card_own_id.json");
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
                c.get("faction") == db.get("faction") && php::loose_eq(c.get("rarity").unwrap_or(&Value::Null), db.get("rarity").unwrap_or(&Value::Null)) && c.get("name") == db.get("name")
            });
            match found {
                Some((k, c)) => (k.clone(), c.clone()),
                None => return Err(PhpError::new(format!("Card class {} does not exist (getCardInstance returns null)", name))),
            }
        }
    };
    let id_paths = own_id_paths().get(&class_key).filter(|p| !p.is_empty());
    if id_paths.is_none() && DYNAMIC_PROPERTIES.iter().all(|p| db.get(*p).is_none_or(Value::is_null)) {
        // The class data as it is: shared (copied on the first write)
        return Ok(class);
    }
    let mut props = (*class).clone();
    // new $className($row): the constructor sees the card's id
    for path in id_paths.into_iter().flatten() {
        if let Some((first, rest)) = path.split_first() {
            if let Some(v) = props.get_mut(first) {
                set_path(v, rest, json!(id));
            }
        }
    }
    for p in DYNAMIC_PROPERTIES {
        if let Some(v) = db.get(*p) {
            if !v.is_null() {
                props.insert(p.to_string(), v.clone());
            }
        }
    }
    Ok(Arc::new(props))
}

/// A card row from the DB columns
pub fn card_from_row(id: i64, location: &str, state: i64, pid: i64, properties: &str) -> R<CardRow> {
    let db = Arc::new(decode_props(properties)?);
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
    fn location(g: &Game, id: i64) -> String {
        g.cards[&id].location.clone()
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

/// `Collection::where('location', $value)` on cards (array value = in_array, `%` = LIKE)
pub fn where_location(g: &Game, ids: Vec<CardId>, value: &Value) -> Vec<CardId> {
    if value.is_null() {
        return ids;
    }
    ids.into_iter()
        .filter(|id| {
            let l = Value::String(g.cards.get(id).or_else(|| g.deleted_cards.get(id)).map(|c| c.location.clone()).unwrap_or_default());
            if php::is_arr(value) {
                php::in_array(&l, value)
            } else {
                let v = php::strval(value);
                if v.contains('%') {
                    php::like_match(&v, &php::strval(&l))
                } else {
                    php::loose_eq(&l, value)
                }
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
            ov = php::array_merge(&[&json!([ov]), &at]);
        }
        let keep = if php::is_arr(value) {
            if php::is_arr(&ov) {
                php::count(&php::array_intersect(value, &ov)) > 0
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
    let ids = where_location(g, get_all(g), location);
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
/// exported to engine/data/starters.min.json): class name and number of copies, in order
pub fn starter_contents(deck_id: &str) -> &'static [(String, i64)] {
    let all = STARTERS.get_or_init(|| {
        let data: Map<String, Value> = serde_json::from_str(include_str!("../../../engine/data/starters.min.json")).expect("starters.min.json");
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
pub fn class_props(class: &str) -> R<Props> {
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
