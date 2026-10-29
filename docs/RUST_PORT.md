# Rust port of the PHP engine: design

The `rules` crate is a faithful translation of the AlteredBGA PHP engine (`docs/PHP_ENGINE.md` is the
map of the PHP code). Correctness first: same structure, same order of operations, same quirks. Speed
comes later, by hill climbing with the golden tests as a safety net. The old `engine` crate keeps
serving games until `rules` passes the tests.

## Rules for translators

- Translate the PHP, don't reinterpret it. One Rust function per PHP method, same name in snake_case,
  same order of statements, same branches (including dead or buggy ones listed in PHP_ENGINE.md §8.1).
  Put a `// PHP: path:line` comment on each function.
- PHP values that are dynamic arrays (action args, node infos, card properties, globals, events) stay
  `serde_json::Value` (with `preserve_order`: PHP arrays keep insertion order). Use the helpers of
  `rules::php` for PHP semantics: truthiness, loose `==`, `in_array`, int/string coercion, `??`.
- Errors: PHP exceptions become `Err(PhpError)`; they propagate with `?` up to the request, which rolls
  back the whole game state (the game is cloned before each request).
- No text matters except where it changes behaviour (empty vs non-empty descriptions in choices,
  PHP_ENGINE.md §2.6). Notifications are kept as a plain log for debugging.
- Don't optimize. Clone when PHP copies. Recompute when PHP recomputes.

## Crate layout (mirrors `modules/php`)

```
rules/src/
  lib.rs            public API (below)
  php.rs            PHP semantics helpers on serde_json::Value
  game.rs           Game: the tables + framework state + engine tree (one struct, Clone + Serialize)
  framework.rs      local BGA framework: gamestate (states.inc.php table, jumpToState re-entrant,
                    nextState, changeActivePlayer, multiactive), custom turn orders, notifications log
  db/pieces.rs      CachedPieces semantics over an ordered map (cards, meeples): get, getMany,
                    getInLocation, getTopOf, move, insertOnTop/AtBottom, moveAtPosition, pickForLocation,
                    shuffle (injectable order), create, delete
  core/globals.rs   Globals: typed defaults per name (Core/Globals.php), get/set/inc/is
  core/engine.rs    Engine (Core/Engine.php): setup, boot, proceed, chooseNode, resolve, resolveAction,
                    checkpoint, confirm, restart, insert* functions
  core/node.rs      node arena + AbstractNode/Seq/Or/Xor/Parallel/Leaf predicates and getChoices
  managers/{cards,meeples,players,actions}.rs
  models/{card,player,action}.rs
  actions/<name>.rs one file per PHP action (39)
  helpers/{conditions,utils,ft,collection}.rs
  states/{setup,new_day,turn,engine,end_game}.rs   the *Trait.php files
  game_methods.rs   altered.game.php (checkCardListeners, genericPlayerCheckListeners, custom orders)
  v1.rs             server/src/V1.php: GameView and decisions of protocol v1, answers → PHP actions
  load.rs           Game::from_bga_tables (rows of cards, meeples, global_variables, player, user_preferences)
```

## Key representations

- **Cards**: ordered map id → `CardRow { id, location: String, state: i64, pid: u32, props: Map }`.
  Properties = the class data (`engine/data/cards.min.json`, keyed by class name
  `<faction>_<Rarity>_<slug(name)>`) overlaid with the DB JSON's `DYNAMIC_PROPERTIES`; Unique cards
  (rarity 2) take everything from the DB JSON (Managers/Cards.php:51-87). Iteration order = PHP cache
  order (id order at load, then creation order).
- **Meeples**: ordered map id → `Meeple { id, location, state: String, kind: String, pid: String }`
  (Meeple keeps strings, as in PHP).
- **Globals**: `Map<String, Value>` with the PHP defaults per declared type.
- **Engine tree**: arena `Vec<Node { infos: Map<String, Value>, childs: Vec<NodeId>, parent: Option<NodeId> }>`.
  Replacing a leaf by a SEQ/PARALLEL changes the node in place, so a `NodeId` held by an action stays
  valid (PHP passes the ctx by reference, PHP_ENGINE.md §2.9). The tree is the canonical state (no JSON
  round trip per request), but `to_array()` / `build_tree()` exist for `infos` copies.
- **Actions**: `managers/actions.rs` dispatches by action name to each module's functions
  (`is_doable`, `is_optional`, `can_pass_action`, `is_automatic`, `is_independent`, `is_irreversible`,
  `is_mandatory`, `get_state`, `st`, `st_pre`, `args`, `act(method, args)`, `act_pass`), with the base
  class defaults of `Models/Action.php`. An action function receives `(&mut Game, NodeId)`.
- **Randomness**: dice from `Game.dice` (the arena's per-game list, cursor in the `arena_rng` table → a
  field). Everything else draws from PHP 8.2's Mersenne Twister, ported bit for bit in `php_rand.rs`
  (`mt_srand`, `mt_rand`, `mt_rand($min, $max)`, `shuffle`, `array_rand` with the packed/hash layout
  that decides its draws; checked against PHP by `tests/php_rand.rs`): `bga_rand` without a dice
  list, `CachedPieces::shuffle`, `Collection::rand`. The arena seeds it at every act
  (`Arena::seedRng`: `mt_srand` of the game id and the number of acts before, both in `arena_rng`), and
  `v1::act` does the same (`Game::seed_rng`), so a position loaded from a dump draws what PHP drew.
  `Game::shuffle_hook` can still inject orders recorded elsewhere.
- **Requests**: `answer()` clones the game, runs the PHP action, and restores the clone on `Err`
  (MariaDB transaction rollback).

## Public API (used by the golden runner and later by the server)

```rust
impl Game {
    pub fn from_bga_tables(tables: &Value, dice: &[i64]) -> Result<Game, PhpError>;
    pub fn waiting_for(&self) -> Vec<u32>;                  // player ids, as V1 `waitingFor`
    pub fn decision(&self, player: u32) -> Option<Value>;   // V1 decision of that player (with its id)
    pub fn answer(&mut self, player: u32, answer: &Value) -> Result<(), PhpError>; // V1 answer
    pub fn oracle_state(&self) -> Value;                    // same format as Arena::oracleState
}
```

## Golden tests

`server/tools/explore.php` (run by `tools/goldens.sh`) records, for every card and three boards, the
tree of decisions PHP asks and the state after each answer. `goldens/` runs them against `rules`:
load the `db` tables, then at each node compare `waiting_for`, the decision (without `id`) and the
compact state, then apply each recorded answer to a clone. Everything is compared, `confirm` decisions
included: they come from the engine's undo/confirmation rules (PHP_ENGINE.md §2.12), which are ported.
