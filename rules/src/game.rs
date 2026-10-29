//! The game: the tables of the PHP engine (cards, meeples, global_variables, player,
//! user_preferences), the framework state (BGA `global` table), the engine tree, the undo log,
//! the notifications of the current request and the sources of randomness.

use crate::core::node::Tree;
use crate::php;
use crate::php_rand::{self, PhpArrayShape, PhpMt};
use crate::{PhpError, R};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

pub type CardId = i64;
pub type PId = i64;
pub type Props = Arc<Map<String, Value>>;

/// A row of `cards` and its `Card` object (Models/Card.php)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardRow {
    pub id: CardId,
    pub location: String,
    pub state: i64,
    pub pid: i64,
    /// `$this->properties` of the object (class data + DB dynamic properties, or the DB JSON
    /// for Uniques)
    pub props: Props,
    /// The `properties` column as stored in the DB
    pub db_props: Props,
    /// The object must be rebuilt from the DB at the next request (a property was written)
    pub reload: bool,
}

/// A row of `meeples` (Models/Meeple.php keeps state, type and player id as strings)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meeple {
    pub id: i64,
    pub location: String,
    pub state: String,
    pub kind: String,
    pub pid: String,
}

/// A row of `player` (Models/Player.php) plus the framework multiactive flag
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerRow {
    pub id: PId,
    pub no: i64,
    pub name: String,
    pub color: String,
    pub eliminated: String,
    pub score: i64,
    pub score_aux: i64,
    pub zombie: String,
    pub faction: String,
    pub is_multiactive: bool,
}

/// A row of `user_preferences`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrefRow {
    pub id: i64,
    pub player_id: PId,
    pub pref_id: i64,
    pub value: Option<String>,
}

/// The tables reverted by the undo log (Helpers/Log.php logs cards, meeples, global_variables
/// and player; not user_preferences nor the framework tables)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tables {
    pub cards: IndexMap<CardId, CardRow>,
    pub meeples: IndexMap<i64, Meeple>,
    pub players: IndexMap<PId, PlayerRow>,
    pub globals: Map<String, Value>,
}

/// An entry of the `log` table: `checkpoint`, `step` or `engine`, with the state of the logged
/// tables at that point (the undo log replays changes backwards; a snapshot gives the same state)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: i64,
    pub kind: String,
    pub move_id: i64,
    pub snapshot: Option<Arc<Tables>>,
}

/// Where shuffles take their order from
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ShuffleHook {
    /// PHP's shuffle on the seeded Mersenne Twister (`Game::mt`)
    Seeded,
    /// The orders PHP produced, in call order (replays); each is the list of ids after shuffle
    /// (when the ids differ, the shuffle draws from the seeded generator)
    Injected(VecDeque<Vec<i64>>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub cards: IndexMap<CardId, CardRow>,
    /// Cards deleted during the current request: PHP objects outlive their rows
    pub deleted_cards: IndexMap<CardId, CardRow>,
    pub meeples: IndexMap<i64, Meeple>,
    pub deleted_meeples: IndexMap<i64, Meeple>,
    pub players: IndexMap<PId, PlayerRow>,
    pub prefs: Vec<PrefRow>,
    /// `Globals::$data`
    pub globals: Map<String, Value>,
    /// The framework `global` table (1 = state, 2 = active player, 3 = move id, 10 = logging)
    pub fw: BTreeMap<i64, i64>,
    /// AUTO_INCREMENT of `cards` / `meeples`
    pub next_card_id: i64,
    pub next_meeple_id: i64,
    /// `Engine::$tree`
    pub tree: Tree,
    pub log: Vec<LogEntry>,
    pub next_log_id: i64,
    /// Notifications of the current request (debugging only)
    pub notifications: Vec<Value>,
    /// Number of notifications sent since the game was loaded (decision ids)
    pub notif_seq: u64,
    /// Die results to use in order (the arena's per-game list) and how many were used
    pub dice: Vec<i64>,
    pub dice_used: usize,
    /// PHP's Mersenne Twister (mt_rand, shuffle, array_rand), seeded at every act from the game id
    /// and the number of acts before it (Arena::seedRng, both kept in the `arena_rng` table)
    #[serde(default)]
    pub rng_game: i64,
    #[serde(default)]
    pub rng_acts: i64,
    #[serde(default)]
    pub mt: Option<PhpMt>,
    pub shuffle_hook: ShuffleHook,
    /// `Table::$environment`: 'prod' with a dice list, 'studio' otherwise
    pub environment: String,
    /// `Table::$currentPlayerId`
    pub current_player: Option<PId>,
    /// `Table::$seatOrder`
    pub seat_order: Vec<PId>,
    /// `GameState::$lastArgs`
    pub last_args: Option<Value>,
    /// `Helpers/Evaluation.php` scope
    #[serde(skip)]
    pub memo: Option<HashMap<String, Value>>,
}

impl Game {
    pub fn empty() -> Game {
        Game {
            cards: IndexMap::new(),
            deleted_cards: IndexMap::new(),
            meeples: IndexMap::new(),
            deleted_meeples: IndexMap::new(),
            players: IndexMap::new(),
            prefs: vec![],
            globals: Map::new(),
            fw: BTreeMap::new(),
            next_card_id: 1,
            next_meeple_id: 1,
            tree: Tree::default(),
            log: vec![],
            next_log_id: 1,
            notifications: vec![],
            notif_seq: 0,
            dice: vec![],
            dice_used: 0,
            rng_game: 0,
            rng_acts: 0,
            mt: None,
            shuffle_hook: ShuffleHook::Seeded,
            environment: "studio".into(),
            current_player: None,
            seat_order: vec![],
            last_args: None,
            memo: None,
        }
    }

    // -----------------------------------------------------------------------
    // Rows
    // -----------------------------------------------------------------------

    /// The card object (also a card deleted during this request)
    pub fn card(&self, id: CardId) -> R<&CardRow> {
        self.cards
            .get(&id)
            .or_else(|| self.deleted_cards.get(&id))
            .ok_or_else(|| PhpError::new(format!("Error: call to a member function on null (card {})", id)))
    }

    pub fn card_mut(&mut self, id: CardId) -> R<&mut CardRow> {
        if self.cards.contains_key(&id) {
            return Ok(self.cards.get_mut(&id).unwrap());
        }
        self.deleted_cards
            .get_mut(&id)
            .ok_or_else(|| PhpError::new(format!("Error: call to a member function on null (card {})", id)))
    }

    pub fn meeple(&self, id: i64) -> R<&Meeple> {
        self.meeples
            .get(&id)
            .or_else(|| self.deleted_meeples.get(&id))
            .ok_or_else(|| PhpError::new(format!("Error: call to a member function on null (meeple {})", id)))
    }

    pub fn meeple_mut(&mut self, id: i64) -> R<&mut Meeple> {
        if self.meeples.contains_key(&id) {
            return Ok(self.meeples.get_mut(&id).unwrap());
        }
        self.deleted_meeples
            .get_mut(&id)
            .ok_or_else(|| PhpError::new(format!("Error: call to a member function on null (meeple {})", id)))
    }

    pub fn player(&self, id: PId) -> R<&PlayerRow> {
        self.players.get(&id).ok_or_else(|| PhpError::new(format!("Error: call to a member function on null (player {})", id)))
    }

    // -----------------------------------------------------------------------
    // Framework globals (`global` table)
    // -----------------------------------------------------------------------

    pub fn fw_get(&self, id: i64) -> Option<i64> {
        self.fw.get(&id).copied()
    }

    pub fn fw_set(&mut self, id: i64, v: i64) {
        self.fw.insert(id, v);
    }

    /// `Table::getActivePlayerId()` (global 2)
    pub fn active_player_id(&self) -> i64 {
        self.fw_get(2).unwrap_or(0)
    }

    /// `Table::getCurrentPlayerId()`: the request's player, or the active player
    pub fn current_player_id(&self) -> i64 {
        self.current_player.unwrap_or_else(|| self.active_player_id())
    }

    // -----------------------------------------------------------------------
    // Notifications (Table::notifyAllPlayers / notifyPlayer)
    // -----------------------------------------------------------------------

    pub fn notify(&mut self, name: &str, args: Value) {
        self.notif_seq += 1;
        self.notifications.push(serde_json::json!({ "name": name, "args": args }));
    }

    // -----------------------------------------------------------------------
    // Randomness
    // -----------------------------------------------------------------------

    /// `Arena::seedRng()` at the start of an act: `mt_srand` with the seed of this act, then one
    /// more act (rolled back with the request, as the `arena_rng` row is)
    pub fn seed_rng(&mut self) {
        self.mt = Some(PhpMt::new(php_rand::seed_of(self.rng_game, self.rng_acts)));
        self.rng_acts += 1;
    }

    /// The generator; before any act (a position loaded and driven directly), as seeded by the
    /// next act
    pub fn mt(&mut self) -> &mut PhpMt {
        let seed = php_rand::seed_of(self.rng_game, self.rng_acts);
        self.mt.get_or_insert_with(|| PhpMt::new(seed))
    }

    /// `mt_rand($min, $max)`
    pub fn mt_rand(&mut self, min: i64, max: i64) -> i64 {
        self.mt().range(min, max)
    }

    /// `bga_rand($min, $max)` through Harness\Rng: dice come from the game's list (Arena::useDice)
    pub fn bga_rand(&mut self, min: i64, max: i64) -> i64 {
        if !self.dice.is_empty() && min == 1 && max == 6 {
            let v = self.dice[self.dice_used % self.dice.len()];
            self.dice_used += 1;
            return v;
        }
        self.mt_rand(min, max)
    }

    /// PHP `shuffle($ids)` through Harness\Rng::shuffle
    pub fn php_shuffle(&mut self, ids: &mut Vec<i64>) {
        if let ShuffleHook::Injected(q) = &mut self.shuffle_hook {
            if let Some(order) = q.pop_front() {
                let mut a = order.clone();
                let mut b = ids.clone();
                a.sort();
                b.sort();
                if a == b {
                    *ids = order;
                    return;
                }
            }
        }
        self.mt().shuffle(ids);
    }

    /// `array_rand($array, 1)` on a new array of these distinct integer keys, inserted in this
    /// order (array_filter, a Collection's copy): the position of the key drawn
    pub fn php_array_rand_int_keys(&mut self, keys: &[i64]) -> usize {
        let shape = PhpArrayShape::from_int_keys(keys);
        self.mt().array_rand(&shape, 1)[0]
    }

    // -----------------------------------------------------------------------
    // Evaluation scope (Helpers/Evaluation.php)
    // -----------------------------------------------------------------------

    /// `Evaluation::run($key, $compute)`
    pub fn eval_run(&mut self, key: String, compute: impl FnOnce(&mut Game) -> R<Value>) -> R<Value> {
        let outermost = self.memo.is_none();
        if outermost {
            self.memo = Some(HashMap::new());
        }
        let res = (|| {
            if let Some(v) = self.memo.as_ref().and_then(|m| m.get(&key)) {
                return Ok(v.clone());
            }
            let v = compute(self)?;
            if let Some(m) = self.memo.as_mut() {
                m.insert(key, v.clone());
            }
            Ok(v)
        })();
        if outermost {
            self.memo = None;
        }
        res
    }

    // -----------------------------------------------------------------------
    // Requests (Arena::freshState + new \altered())
    // -----------------------------------------------------------------------

    /// Drops every per-request cache, as a new BGA request would: cards are rebuilt from their DB
    /// row, globals are read back from their JSON, notifications are cleared
    pub fn fresh_state(&mut self, current_player: Option<PId>) -> R<()> {
        let ids: Vec<CardId> = self.cards.iter().filter(|(_, c)| c.reload).map(|(id, _)| *id).collect();
        for id in ids {
            let db = self.cards[&id].db_props.clone();
            let props = crate::managers::cards::instantiate_props(&db, id)?;
            let c = self.cards.get_mut(&id).unwrap();
            c.props = props;
            c.reload = false;
        }
        self.deleted_cards.clear();
        self.deleted_meeples.clear();
        crate::core::globals::fetch(self);
        self.notifications.clear();
        self.current_player = current_player;
        self.memo = None;
        self.last_args = None;
        Ok(())
    }

    /// Starts a request of `player` (None = a server request): fresh state, then `new altered()`
    /// (Engine::boot)
    pub fn begin_request(&mut self, player: Option<PId>) -> R<()> {
        self.fresh_state(player)?;
        crate::core::engine::boot(self)
    }

    // -----------------------------------------------------------------------
    // Undo log (Helpers/Log.php)
    // -----------------------------------------------------------------------

    pub fn snapshot_tables(&self) -> Tables {
        Tables { cards: self.cards.clone(), meeples: self.meeples.clone(), players: self.players.clone(), globals: self.globals.clone() }
    }

    /// Puts back the logged tables as they were at a log entry (Log::revertTo), then drops the
    /// caches as revertTo does (Globals::fetch, Cards/Players/Meeples::invalidate)
    pub fn restore_tables(&mut self, t: &Tables) -> R<()> {
        let multiactive: Vec<(PId, bool)> = self.players.iter().map(|(id, p)| (*id, p.is_multiactive)).collect();
        self.cards = t.cards.clone();
        for c in self.cards.values_mut() {
            c.reload = true;
        }
        self.meeples = t.meeples.clone();
        self.players = t.players.clone();
        for (id, m) in multiactive {
            if let Some(p) = self.players.get_mut(&id) {
                p.is_multiactive = m;
            }
        }
        self.globals = t.globals.clone();
        let ids: Vec<CardId> = self.cards.keys().copied().collect();
        for id in ids {
            let db = self.cards[&id].db_props.clone();
            let props = crate::managers::cards::instantiate_props(&db, id)?;
            let c = self.cards.get_mut(&id).unwrap();
            c.props = props;
            c.reload = false;
        }
        crate::core::globals::fetch(self);
        Ok(())
    }
}

/// `json_decode` of a properties column
pub fn decode_props(s: &str) -> R<Map<String, Value>> {
    let mut v: Value = serde_json::from_str(s).map_err(|e| PhpError::new(format!("bad properties JSON: {}", e)))?;
    php::normalize(&mut v);
    match v {
        Value::Object(m) => Ok(m),
        Value::Array(a) if a.is_empty() => Ok(Map::new()),
        _ => Err(PhpError::new("properties is not an object")),
    }
}
