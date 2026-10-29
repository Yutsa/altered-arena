# PHP engine reference (AlteredBGA)

Architecture reference for the Rust translation of the Altered rules engine written for Board Game Arena.
The goal of the port is to reproduce the PHP code exactly: same structure, same order of operations,
same quirks. Every claim below cites the PHP source.

## Conventions

- Paths without prefix are relative to `AlteredBGA/modules/php/` (e.g. `Core/Engine.php:102`).
- `game:` = `AlteredBGA/altered.game.php`, `action:` = `AlteredBGA/altered.action.php`,
  `states:` = `AlteredBGA/states.inc.php`, `consts:` = `modules/php/constants.inc.php`.
- `FW:` = `altered-arena/server/framework/module/table/table.game.php` (local replacement of the BGA
  framework), `RNG:` = `altered-arena/server/framework/rng.php`, `Arena:` = `altered-arena/server/src/Arena.php`.
- Card classes in `modules/php/Cards/` are data (already exported as JSON) and are not covered.
  `Helpers/FlowConvertor.php` (6.9k lines) builds the effect trees of Unique cards from their
  trigger/condition/output ids; it is only called when a Unique is generated (`Managers/Cards.php:491`,
  `:567`), so its output is data stored in the card's `properties` JSON.
- "Node" = engine tree node, "action" = atomic action class in `Actions/`, "flow" = the array form of a
  tree (`['type'=>..., 'action'=>..., 'args'=>..., 'childs'=>[...]]`).

---

## 1. Request lifecycle and state machine

### 1.1 One request

`Arena:213-231` runs every player request inside one MariaDB transaction and rolls back on any
exception. Before the request, `Arena:191-208` drops every static cache (`Cards`, `Meeples`, `Players`
caches, `Globals::$data/$initialized`, `Notifications::$cachedValues`, the framework's `global` table copy).
So the unit of atomicity is the request: any exception (`BgaUserException`, `BgaVisibleSystemException`,
`feException`, `UserException`) cancels every DB write of the request. BGA behaves the same way.

The game object is built at each request (`game:59-72`): `Engine::boot()` reads the engine tree from the
`engine` global, rebuilds node objects and wraps the root in a SEQ node if needed, saving the tree if it
wrapped it (`Core/Engine.php:21-26`, `:41-47`).

### 1.2 Entry points

`action:` methods only parse arguments and forward to the game object. All engine actions go through
`States/EngineTrait.php`.

| HTTP action (`action:`) | Game method | What it does |
|---|---|---|
| `actTakeAtomicAction` (190-198): `actionName` alphanum, `actionArgs` JSON validated by `validateJSonAlphaNum` (226-243) | `EngineTrait::actTakeAtomicAction` (`States/EngineTrait.php:120-127`) | `Notifications::resetCache()`, `localCheckAction($actionName)`, then `Actions::takeAction(currentNode.action, $actionName, $args, currentNode)` |
| `actChooseAction` (200-206), `id` int | `EngineTrait::actChooseAction` (185-189) | `Engine::chooseNode(activePlayer, id)` |
| `actPassOptionalAction` (208-213) | `EngineTrait::actPassOptionalAction` (132-143) | checks the action is allowed in the state, then `Actions::pass(currentAction, currentNode)` |
| `actConfirmTurn` (161) | `EngineTrait::actConfirmTurn` (230-236) | `Engine::confirm()` |
| `actConfirmPartialTurn` (168) | `EngineTrait::actConfirmPartialTurn` (238-242) | `Engine::confirmPartialTurn()` |
| `actRestart` (175) | `EngineTrait::actRestart` (244-254) | requires `Globals::isUndo()` and `engineChoices >= 1`, then `Engine::restart()` |
| `actUndoToStep` (182) | `EngineTrait::actUndoToStep` (256-263) | requires undo, `Engine::undoToStep($stepId)` |
| `actAnytimeAction` (215) | `EngineTrait::actAnytimeAction` (99-115) | dead in practice: `addArgsAnytimeAction` always builds an empty list (`States/EngineTrait.php:81-96`) |
| `actSelectPrecoDeck`, `actSelectRandomDeck`, `actConfirmAPIDeck`, `actCancelPrecoDeckSelection` | `States/SetupTrait.php:224`, `:144`, `:455`, `:237` | deck choice in state 2 |
| `actFirstDayManaSelection`, `actCancelFirstDayManaSelection` | `States/NewDayTrait.php:70`, `:91` | first-day mana choice in state 11 |
| `actChangePref` | `game:144-147` | `Preferences::set` (not logged, not undone) |
| `actLoadAPIDecks`, `actGetDeckInfos`, `actDisplayAllCards`, `loadBugSQL`, `actGetDeck`, `actNewDayManaSelection`, `actPassNewDayManaSelection` | Setup/Debug traits | BGA/UI only; the last two have no game method |

`localCheckAction` (`game:402-408`) calls `checkAction($name, false)` and throws `feException` if the
action is not in the state's `possibleactions`. In the local framework a state without
`possibleactions` accepts every action name (`FW:322-329`).

### 1.3 Framework semantics (local `Table` / `GameState`)

- Global ids: `1` = current state id, `2` = active player id (`FW:259-262`, `:331-334`, `:498-501`);
  `3` is read by `Log::addEntry` as the move id (`Helpers/Log.php:51`); `10` = game state label
  `logging` (`game:63-65`). Values are cast to int (`FW:439-445`).
- `jumpToState($id)` is synchronous and re-entrant (`FW:286-306`): it sets the state id, records a
  `gameStateChange` notification, calls `getGameProgression()` if `updateGameProgression`, calls the
  state's `action` method **inside the same PHP call stack**, then, if the state did not change during
  that call and the state is `activeplayer`/`multipleactiveplayer`, computes its `args`. Code placed
  after a `jumpToState` call runs after the whole chain triggered by the new state has returned. The
  engine relies on this: a single player request usually runs many states recursively until a state
  waits for input.
- `nextState($t)` (`FW:308-320`): uses transition `$t`; if not found and the state has exactly one
  transition, uses it; otherwise throws.
- `changeActivePlayer($pId)` only writes global 2 (`FW:331-334`).
- `setPlayersMultiactive($ids, $next, $exclusive)` (`FW:341-354`): clears all flags if exclusive, sets
  the given players, and calls `nextState($next)` if nobody is active. `setAllPlayersMultiactive`
  (`FW:336`), `setPlayerNonMultiactive` (`FW:356-364`).
- `getCurrentPlayerId()` returns the request's player, or the active player when none (`FW:508-511`).
- Player order: `getNextPlayerTable()` (`FW:535-543`) returns `[0 => firstSeat, pId => nextPId]`.
  `Globals::firstPlayer` is initialised from index `0` (`States/SetupTrait.php:26`).

### 1.4 States (`states:` + ids in `consts:65-138`)

Types: `manager`, `game` (runs its action, never waits), `activeplayer`, `multipleactiveplayer`.

| id | name | type | action | args | transitions / notes |
|---|---|---|---|---|---|
| 1 `ST_GAME_SETUP` | gameSetup | manager | `stGameSetup` (FW:617, `nextState('')`) | | `'' → 2` |
| 50 | plop | activeplayer | | | debug, unused |
| 97 `ST_GENERIC_NEXT_PLAYER` | genericNextPlayer | game | none | | target of custom turn orders (`game:194`) |
| 2 `ST_PRECO_DECK_SELECTION` | selectPrecoDeck | multipleactiveplayer | | `argsPrecoDeckSelection` | `done → 5`, `zombiePass → 5` |
| 3 `ST_DECK_SELECTION` | selectDeck | multipleactiveplayer | | | unused |
| 5 `ST_SETUP` | deckSetup | game | `stDeckSetup` | | `'' → 10` |
| 10 `ST_FIRST_DAY` | firstDay | game | `stFirstDay` | | `done → 11` (reached with `nextState('')`, single transition) |
| 11 `ST_FIRST_DAY_MULTI` | firstDayManaSelection | multipleactiveplayer | | `argsFirstDayManaSelection` | `done → 18`, `zombiePass → 18`; in practice left through `checkCardListeners('Noon', 18)` |
| 12 `ST_NEW_DAY` | newDay | game | `stNewDay` | | none |
| 18 `ST_BEFORE_ASSIGNMENT` | beforeAssignment | game | `stBeforeAssignment` | | updateGameProgression |
| 19 `ST_ASSIGNMENT` | assigment | game | `stAssignment` | | `done → 83` (unused) |
| 83 `ST_PRE_DUSK_PHASE` | beforeDusk | game | `stBeforeDusk` | | never entered: methods are called directly |
| 84 `ST_DUSK` | dusk | game | `stDusk` | | never entered |
| 87 `ST_AFTER_DUSK` | afterDusk | game | `stAfterDusk` | | never entered |
| 85 `ST_BEFORE_NIGHT` | beforeNight | game | `stBeforeNight` | | never entered |
| 89 `ST_END_OF_NIGHT` | endOfNight | game | `stEndOfNight` | | entered as end callback of `nightPhase` |
| 20 `ST_CHOOSE_ASSIGNMENT` | chooseAssignment | activeplayer | **none** | `argsAtomicAction` | possible: actPlay, actSupport, actTap, actPass, actConfirmTurn, actRestart, actPassOptionalAction |
| 21 `ST_DISCARD` | discard | activeplayer | `stAtomicAction` | `argsAtomicAction` | actDiscard, actConfirmTurn, actRestart, actPassOptionalAction |
| 22 `ST_GAIN` | gain | game | `stAtomicAction` | | actPassOptionalAction |
| 23 `ST_TARGET` | target | activeplayer | `stAtomicAction` | `argsAtomicAction` | actTarget, actPassOptionalAction, actConfirmTurn, actRestart |
| 24 `ST_LOOSE` | loose | game | `stAtomicAction` | | |
| 25 `ST_SPELL_CLEANUP` | spellCleanup | game | `stAtomicAction` | | |
| 26 `ST_INVOKE_TOKEN` | invokeToken | activeplayer | `stAtomicAction` | `argsAtomicAction` | actInvokeToken, actInvokeTokenPass, actConfirmTurn, actRestart, actPassOptionalAction |
| 27 `ST_ACTIVATE_CARD` | activateCard | game | `stAtomicAction` | | actPassOptionalAction |
| 28 `ST_DRAW` | draw | game | `stAtomicAction` | | |
| 29 `ST_SPECIAL_EFFECT` | specialEffect | game | `stAtomicAction` | | |
| 30 `ST_CHECK_CONDITION` | checkCondition | game | `stAtomicAction` | | actPassOptionalAction |
| 31 `ST_AFTER_YOU` | afterYou | game | `stAtomicAction` | | |
| 32 `ST_ROLL_DIE` | rollDie | activeplayer | `stAtomicAction` | `argsAtomicAction` | actRollDie, actDiscardAdd, actConfirmTurn, actRestart, actPassOptionalAction |
| 33 `ST_RESUPPLY` | resupply | game | `stAtomicAction` | | |
| 34 `ST_MOVE_EXPEDITION` | moveExpedition | activeplayer | `stAtomicAction` | `argsAtomicAction` | actMoveExpedition, actConfirmTurn, actRestart, actPassOptionalAction |
| 35 `ST_USE_COUNTER` | useCounter | game | `stAtomicAction` | | |
| 36 `ST_ACTIVATE_EFFECT` | activateEffect | game | `stAtomicAction` | | actPassOptionalAction |
| 37 `ST_TAP` | tap | game | `stAtomicAction` | | actPassOptionalAction |
| 38 `ST_PLAY_CARD` | playCard | activeplayer | `stAtomicAction` | `argsAtomicAction` | actPlayCard, actPassOptionalAction, actConfirmTurn, actRestart |
| 39 `ST_MOVE_CARD` | moveCard | game | `stAtomicAction` | | |
| 40 `ST_PAY` | pay | activeplayer | `stAtomicAction` | `argsAtomicAction` | actPay, actPassOptionalAction, actConfirmTurn, actRestart |
| 41 `ST_DRAW_MANA` | drawMana | game | `stAtomicAction` | | |
| 42 `ST_BLOCK_EXPEDITION` | blockExpedition | activeplayer | `stAtomicAction` | `argsAtomicAction` | actBlockExpedition, actConfirmTurn, actRestart, actPassOptionalAction |
| 43 `ST_TARGET_PLAYER` | targetPlayer | activeplayer | `stAtomicAction` | `argsAtomicAction` | actTargetPlayer, actPassOptionalAction, actConfirmTurn, actRestart |
| 44 `ST_DISCARD_DO` | discardDo | activeplayer | `stAtomicAction` | `argsAtomicAction` | actTarget, actPassOptionalAction, actConfirmTurn, actRestart |
| 45 `ST_TARGET_EXPEDITION` | targetExpedition | activeplayer | `stAtomicAction` | `argsAtomicAction` | actTargetExpedition, actConfirmTurn, actRestart, actPassOptionalAction |
| 46 `ST_EXHAUST` | exhaust | game | `stAtomicAction` | | actPassOptionalAction |
| 47 `ST_READY` | ready | game | `stAtomicAction` | | actPassOptionalAction |
| 48 `ST_EXCHANGE` | exchange | activeplayer | `stAtomicAction` | `argsAtomicAction` | actExchange, actPassOptionalAction, actConfirmTurn, actRestart |
| 49 `ST_SPEND` | spend | activeplayer | `stAtomicAction` | `argsAtomicAction` | actSpend, actConfirmTurn, actRestart, actPassOptionalAction |
| 50 `ST_BOOST_EXCHANGE` | boostExchange | activeplayer | `stAtomicAction` | `argsAtomicAction` | actBoostExchange, ... (the action is automatic, see §3.3). Note: id 50 collides with the debug state `50` above; the later key wins in the PHP array, so `50` is `boostExchange`. |
| 51 `ST_END_AFTERNOON` | endAfternoon | game | `stAtomicAction` | | actPassOptionalAction |
| 52 `ST_MARK_REGION` | markRegion | activeplayer | `stAtomicAction` | `argsAtomicAction` | actMarkRegion, actPassOptionalAction, actConfirmTurn, actRestart |
| 53 `ST_MOVE_REGION_MARKER` | moveRegionMarker | activeplayer | `stAtomicAction` | `argsAtomicAction` | actMoveRegionMarker, ... |
| 54 `ST_INTERRUPT_REVEAL` | interuptReveal | activeplayer | `stAtomicAction` | `argsAtomicAction` | actInteruptReveal, ... |
| 55 `ST_SHUFFLE` | shuffle | game | `stAtomicAction` | | |
| 56 `ST_COMPLETE_FEAT` | completeFeat | game | `stAtomicAction` | | actPassOptionalAction |
| 57 `ST_DISCARD_FROM_DECK` | discardFromDeck | game | `stAtomicAction` | | |
| 88 `ST_NIGHT_CLEANUP` | nightCleanup | activeplayer | `stAtomicAction` | `argsAtomicAction` | only `actNightCleanup` (an auto-pass here would throw, see §3.1) |
| 90 `ST_RESOLVE_STACK` | resolveStack | game | `stResolveStack` (empty, `States/EngineTrait.php:191`) | | used as a neutral state before changing the active player |
| 93 `ST_CONFIRM_TURN` | confirmTurn | activeplayer | `stConfirmTurn` | `argsConfirmTurn` | actConfirmTurn, actRestart |
| 94 `ST_CONFIRM_PARTIAL_TURN` | confirmPartialTurn | activeplayer | none | `argsConfirmTurn` | actConfirmPartialTurn, actRestart |
| 91 `ST_RESOLVE_CHOICE` | resolveChoice | activeplayer | `stResolveChoice` (empty) | `argsResolveChoice` | actChooseAction, actRestart |
| 92 `ST_IMPOSSIBLE_MANDATORY_ACTION` | impossibleAction | activeplayer | | `argsImpossibleAction` | unused (jump commented out, `Managers/Actions.php:140`) |
| 98 `ST_PRE_END_OF_GAME` | preEndOfGame | game | `stPreEndOfGame` | | `'' → 99` |
| 99 `ST_END_GAME` | gameEnd | manager | `stGameEnd` (FW:615, empty) | | |

Constants `ST_NEW_DAY_MULTI` 13, `ST_NEW_DAY_BONUS` 14, `ST_NIGHT` 86 have no state.

All `gamestate` calls in the code: `game:194-196` (custom order), `game:219`, `game:316`, `Core/Engine.php:126,155-156,183,191-192,257,281,627`,
`States/NewDayTrait.php:44-45,116`, `States/TurnTrait.php:233,316,357,384`, `States/SetupTrait.php:31-32,274,278,551`,
`Managers/Players.php:302`, `States/EndGameTrait.php:123`.

### 1.5 Custom turn orders (`game:154-226`)

Stored in global `customTurnOrders` as `key => ['order' => pIds, 'index' => int, 'callback' => int|string,
'args' => array, 'endCallback' => int|string, 'loop' => bool]`.

- `initCustomTurnOrder($key, $order, $callback, $endCallback, $loop=false, $autoNext=true, $args=[])`
  (`game:154-170`): `order ?? Players::getTurnOrder()` (first player then `getNextId`, `Managers/Players.php:132-143`),
  index `-1`, then `nextPlayerCustomOrder($key)` if `$autoNext`.
- `initCustomDefaultTurnOrder` = same with `order = null` (`game:172-175`).
- `nextPlayerCustomOrder($key)` (`game:177-200`): `index+1`; if it equals `count(order)` and `loop`, wrap
  to 0; save. If `index < count`: `jumpToState(97)`, `changeActivePlayer(order[i])`,
  `jumpToOrCall(callback, args)`. Else `endCustomOrder($key)`.
- `endCustomOrder($key)` (`game:202-214`): sets index to `count(order)`, `jumpToOrCall(endCallback)` (no args).
- `jumpToOrCall($mixed, $args)` (`game:216-226`): int that is a state id → `jumpToState`; otherwise
  calls the method by name with one argument `$args` (methods that declare no parameter just ignore it).
  PHP method names are case-insensitive: `'stAfterNewday'` (`States/NewDayTrait.php:272`) calls
  `stAfterNewDay` (`:309`).
- The engine ends a flow by calling `nextPlayerCustomOrder($callback['order'])` (`Core/Engine.php:628-629`),
  which is how each player's engine flow hands over to the next player of the order.

### 1.6 Phases

Phase numbers in `Globals::phase`: 0 morning, 1 noon, 2 afternoon, 3 dusk, 4 night (`States/NewDayTrait.php:171,311`, `States/TurnTrait.php:47,186,360`).

**Setup.**
`setupNewGame` (`States/SetupTrait.php:20-33`): `Globals::setupNewGame` (storm cards, day 0, deck format, undo option; `Core/Globals.php:226-272`),
`Players::setupNewGame`, `firstPlayer = nextPlayerTable[0]`, `Players::initializeDecks`, stats, `logging = 0`,
all players multiactive, `activeNextPlayer()`. Then state 1 → 2. In state 2 each player picks a deck;
`updateActivePlayersPrecoDeckSelection` (`:253-280`) keeps undecided players active (players who chose
`API` without stored content are reset) and calls `nextState('done')` when all have chosen.
`stDeckSetup` (`:482-552`) creates/keeps the chosen deck, deletes the other cards of the player, moves
the hero to `board-hero-<pId>`, renames the deck location to `deck-<pId>`, creates the storm tokens
(`Meeples::setupPlayer`: companion on `storm-7`, hero on `storm-0`, `Managers/Meeples.php:47-53`),
shuffles `deck-<pId>`, then `nextState('')` → 10.

**First day.** `stFirstDay` (`States/NewDayTrait.php:19-46`): `day = 1`, hero terrain markers
(`Meeples::createHeroMarkers`), each player draws 6 (15 with testing option), all players multiactive,
→ state 11. Each player selects 3 cards (10 with testing option) (`:70-89`); when nobody is left
(`:104-134`) the selections go to `mana` (`Cards::discard($ids, MANA)` = move) and
`checkCardListeners('Noon', ST_BEFORE_ASSIGNMENT)`. There is no Morning on day 1.

**Afternoon (assignment).** `stBeforeAssignment` (`States/TurnTrait.php:41-77`): phase 2, resets
(`stormMoves`, `afterRest`, `afterNightCleanup`, `playedCards`, `skippedPlayers`, `costReduction`,
next-character modifiers, ability-activation counters, `smokeThemOutArmed`), `dayPhase = true`, clears
`extraDatas.userPower` on all cards, then `initCustomDefaultTurnOrder('assignment', ST_ASSIGNMENT, 'stPreBeforeDusk', loop=true)`.
`stAssignment` (`:82-171`): if the active player is in `skippedPlayers`, either everyone passed
(`dayPhase = false`, `endCustomOrder`) or `nextPlayerCustomOrder`. Otherwise it resets the per-turn
globals (non-permanent cost reductions, next-character modifiers, `additionalEffect`, `activePId`,
fleeting removers, ability counters for this player…), then
`Engine::setup(['childs' => [['action' => CHOOSE_ASSIGNMENT, 'pId' => $pId]]], ['order' => 'assignment'])`
and `Engine::proceed()`. The loop continues until all players are in `skippedPlayers`.

**Dusk.** `stPreBeforeDusk` (`:181-228`): phase 3, same resets as above, `stormMoves = []`,
`checkCardListeners('BeforeDusk', 'stBeforeDusk')`. `stBeforeDusk` (`:230-237`): `instantWin` →
state 98, else `checkCardListeners('AtDusk', 'stDusk')`. `stDusk` (`:239-242`): one-player order
`advancePhase` → `stAdvanceDusk` (`:245-326`): normally `Engine::setup(empty SEQ, ['order'=>'advancePhase'])`,
`Players::computeStorm(true)` (moves tokens or queues MOVE_EXPEDITION/ROLL_DIE/... nodes on the
after-finishing node, `Managers/Players.php:572-775`), `Engine::proceed()`. In tie-break mode it
computes the winner of the tie-break region directly and either ends the game or calls `stAfterDusk`.
`stAfterDusk` (`:328-336`): `checkCardListeners('AfterDusk', 'stBeforeNight')`.

**Night.** `stBeforeNight` (`:347-366`): `instantWin` → 98; phase 4; one-player order `nightCleanup` →
`stNightCleanup` (`:368-386`): `Engine::setup(empty SEQ, ['order'=>'nightCleanup'])`, for each player in
turn order `Player::nightCleanup()` (fleeting discarded, characters to reserve, ...; `Models/Player.php:626`),
`Meeples::nightCleanup()` (deletes ASCEND markers), `activePId = firstPlayer`,
`Engine::updateParallelChilds(['noIndependent' => true])`, active player = first player, `proceed`.
`stAfterNightCleanup` (`:388-392`): `checkCardListeners('BeforeNight', 'stPreNight', [])`.
`stPreNight` (`:394-398`): `playedCards = 1`, order `nightPhase` → `stNight` (`:400-441`) per player: if
reserve or landmarks exceed their slots (exhausted reserve cards count against
`exhaustedReserveSlots`), `Engine::setup` of one `NIGHT_CLEANUP` node; else next player. End callback
`ST_END_OF_NIGHT` → `stEndOfNight` (`:443-447`): `checkCardListeners('EndOfNight', ST_NEW_DAY)`.

**Morning.** `stNewDay` (`States/NewDayTrait.php:139-142`): one-player order `morning` → `stNewDayInit`
(`:144-262`): `Players::checkVictory()` (may jump to 98 and return); enter tie-break mode if flagged;
`day++`; resets; `Cards::untapAll()`; phase 0; first player = next of current first player; draws:
each player draws 2 cards (hero `ALT_CORE_B_LY_03_C`: 1 card + 1 card to mana, and the player is added
to `skippedPlayers`), except when the deck holds 0–2 cards, in which case DRAW/DRAW_MANA/SHUFFLE nodes
are built and run through the engine (`Engine::setup(FT::SEQ(...), ['order' => 'morning'])`). End
callback `stCheckMorning` → `checkCardListeners('Morning', 'initNewDay')` → `initNewDay` (`:270-273`):
looping order `newDay` → `stNewDayDraw` (`:275-306`): a player not yet in `skippedPlayers` is added and
gets one optional `DISCARD` node (hand → mana, `canPass`); when everyone is in `skippedPlayers` the
order ends → `stAfterNewDay` (`:309-319`): phase 1 (Noon), `checkCardListeners('Noon', ST_BEFORE_ASSIGNMENT)`.

**End of game.** `Players::checkVictory` (`Managers/Players.php:249-307`) sets score 1 on the winner
and calls `jumpToOrCall(ST_PRE_END_OF_GAME)`. `stPreEndOfGame` (`States/EndGameTrait.php:17-125`)
reports the game to an external API (BGA-only) and `nextState('')` → 99.

### 1.7 Phase triggers: `checkCardListeners` (`game:236-286`)

`checkCardListeners($type, $endCallback, $event = [], $order = null)` sets `event.type = event.method = $type`,
`event.phase = true`, and starts a custom order keyed `$type` whose callback is
`genericPlayerCheckListeners($event)`. For each player of the order (default turn order):

1. `event.pId` = active player; `$reaction = Cards::getReaction($event)` (null if none).
2. `BeforeNight`: append `Globals::afterRest[pId]`; `EndOfNight`: append `Globals::afterNightCleanup[pId]`.
   If `$reaction` was null it becomes that list. If it was not null, the code does
   `$reaction = array_push($reaction, ...$list)` (`game:259`, `:271`): `$reaction` becomes an int, and
   `count($reaction)` two lines later throws a `TypeError` in PHP 8. Reproduce as an error.
3. No reaction → `nextPlayerCustomOrder($type)`. Otherwise
   `Engine::setup(['type' => 'parallel', 'childs' => $reaction, 'noIndependent' => count($reaction) > 1], ['order' => $type])`
   and `Engine::proceed()`.

Because `event.phase` is set, `Card::isListeningTo` only accepts cards whose `pId` equals the event's
`pId` (`Models/Card.php:823-827`): each player resolves their own phase triggers on their turn of the order.

### 1.8 Zombie and concede

`zombieTurn` (`game:305-317`) ends the game: next player gets score 1, `zombie = true`, stats, jump to
98. Everything after the `return` at line 317 is dead code. `onConcede` (`game:355-367`) does the same
and calls `stPreEndOfGame(true)`.

---

## 2. The Engine (`Core/Engine.php`, `Core/Engine/*Node.php`)

### 2.1 Storage

The tree lives in `Engine::$tree` (static) and is persisted after every change as the `engine` global
(`Core/Engine.php:32-36`), a JSON object produced by `toArray()` (`Core/Engine/AbstractNode.php:130-137`):
the node's `infos` array merged with `childs` (recursively). `buildTree($t)` (`Core/Engine.php:68-81`):
type = `$t['type'] ?? (childs empty ? 'leaf' : 'seq')`; class `ucfirst(type) . 'Node'`
(`SeqNode`, `OrNode`, `XorNode`, `ParallelNode`, `LeafNode`); `infos` = the array without `childs`.
Each node keeps a pointer to its parent (`attach`, `AbstractNode.php:34-37`). `LeafNode` ignores
children (`LeafNode.php:14-18`). Every node constructor forces `infos['type']`.

`Engine::setup($t, $callback)` (`Core/Engine.php:53-62`): build and save the tree, store the callback in
`callbackEngineResolved` (`['order' => key]` in practice; `['state' => id]` and `['method' => name]`
are supported by `confirm`), reset `engineChoices` and `engineAutomatic` to 0, `Log::enable()`,
`Log::startEngine()` (checkpoint + `engine` log entry). Note that `boot` (not `setup`) wraps a non-SEQ
root into a SEQ (`:41-47`); `setup` stores whatever root it is given (a LEAF in `stNewDayDraw`, a
PARALLEL in `genericPlayerCheckListeners`), and the wrapping happens at the next request's `boot` or at
the next `insertAtRoot`/`getAfterFinishingNode`. Until then `replace()` on that root throws
(`AbstractNode.php:60-67`), so `insertAsChild` on a LEAF root fails in the same request.

### 2.2 Node `infos` keys

| key | meaning | read at |
|---|---|---|
| `type` | `seq`/`or`/`xor`/`parallel`/`leaf` | `AbstractNode.php:245-248` |
| `action`, `args` | atomic action name and its arguments | `:564-567`, `:255-258` |
| `pId` | player id, or `'nextPlayer'`, `'active'`, `'source'` (resolved by `getPId`), `'all'` (multiactive, `Engine.php:154`), `'owner'` (rewritten by `updateCardId`) | `:223-243` |
| `sourceId`, `source`, `cardId`, `event` | provenance; `event` is the listener event (set by `Utils::tagTree` in ActivateCard) | `:260-283` |
| `optional` | node may be passed; `SeqNode::choose` sets it false; `resolveAction` sets it false | `:270-273`, `:402-405` |
| `mandatory` | set by `enforceMandatory` when a parent chose this child manually | `:615-623` |
| `noIndependent` | disables the independent-first rule and makes the node non-independent | `ParallelNode.php:115`, `AbstractNode.php:434` |
| `manualChoice` | every choice under this node is `optionalAction` and `forceManualChoice` (no auto choice) | `:407-410`, `:472`, `:479` |
| `flag` | `afterFinishing` (root PARALLEL collecting reactions) or `preActionDone` (stPre already run) | `:250-253`, `:544-547` |
| `choice`, `countChoices` | index of the chosen child, number of choices made | `:514-529` |
| `resolved`, `resolutionArgs` | node resolution | `:320-347` |
| `actionResolved`, `actionResolutionArgs` | action resolution (see §2.7) | `:569-592` |
| `willBeDoable` | leaf is always doable | `LeafNode.php:94-96` |
| `reusable` | `Engine::resolveAction` does not mark it | `AbstractNode.php:302-305`, `Engine.php:387` |
| `resolveParent` | resolving the action also resolves the parent | `:307-310`, `Engine.php:389-391` |
| `forceConfirmation`, `customDescription` | description only | `:164-208`, `:297-300` |
| `groupGains`, `gainGroupId` | SEQ of a gain distribution (`FT::SEQ_DISTRIBUTE_GAINS`) | `Actions/Gain.php:229-244` |
| `automatic`, `independent` | node-level overrides read by `Resupply::isAutomatic` and `InvokeToken::isAutomatic/isIndependent` | `Actions/Resupply.php:55`, `Actions/InvokeToken.php:116-127` |
| `backupNode` | OR node with `args.canReuse` | `Engine.php:206-219` |
| `duplicate` | set by `duplicateAction` | `Models/Action.php:398` |
| `activePId` | set on the after-finishing node when reused (not read elsewhere) | `Engine.php:449-451` |
| `flagToto` | written by `setFlag` (never read) | `AbstractNode.php:549-552` |

`FT` builders (`Helpers/FT.php`): `SEQ`, `SEQ_DISTRIBUTE_GAINS` (`groupGains`), `SEQ_OPTIONAL`
(`optional`), `SEQ_OPTIONAL_MANUAL` (`optional`, `manualChoice`), `OR`, `XOR`, `XOR_OPTIONAL`, `PAR`,
`ACTION($name, $args, $node)`, `GAIN`, `AUGMENT`, `LOOSE`, `RETURN_TO_HAND`, `DISCARD_TO_RESERVE`, `RUSH*`
(`CHOOSE_ASSIGNMENT` with `actions => ['play']`, `mandatory`), `SABOTAGE`.

### 2.3 `getNextUnresolved`

- Abstract (`AbstractNode.php:330-341`), used by OR, XOR, PARALLEL, LEAF: null if resolved; itself if no
  `choice` or if `childs[choice]` is resolved; otherwise recurse into `childs[choice]`. So after a
  chosen child is resolved, the parent becomes current again and offers its remaining choices.
- SEQ (`SeqNode.php:73-88`): null if resolved; itself if `isOptional(activePlayer)`; otherwise recurse into
  the first unresolved child.

### 2.4 `Engine::proceed($confirmedPartial = false, $isUndo = false)` (`Core/Engine.php:102-263`)

1. `$node = tree->getNextUnresolved()`, `$player = Players::getActive()`.
2. **Tree finished** (`:111-145`): if (`dayPhase === true` and (`playedCards != 0` or `activePId` in
   `skippedPlayers`)) or `dayPhase === false`: auto-confirm (`confirm()`) when `engineChoices == 0` or undo is
   off or the active player's pref 152 is "disabled"; otherwise `jumpToState(ST_CONFIRM_TURN)`. In the
   other case (afternoon turn where nothing was played or passed yet, e.g. after a hero tap or a free
   play), `playedForFree = false`, insert a new `CHOOSE_ASSIGNMENT` for `activePId` at root, and recurse.
3. `$oldPId` = active player; `$pId = $node->getPId()`. `'all'` → state 90, all players multiactive,
   `Log::checkpoint()`, reset both counters, `proceedToState` and return. `'nextPlayer'`, `'active'`,
   `'source'` are resolved here too (`:166-172`).
4. **Partial confirmation** (`:176-185`): if undo is on and (pref 152 enabled or pref 104 enabled) for the
   *previous* active player, `$pId` set and different from `$oldPId`, the node is not independent for
   the new player, `engineChoices != 0`, and not already confirmed → `jumpToState(ST_CONFIRM_PARTIAL_TURN)`.
5. If `$pId` set and different: `jumpToState(ST_RESOLVE_STACK)`, `changeActivePlayer($pId)`. If `$pId` is
   null the active player stays.
6. If confirmed partial or the player changed: `Log::enable()`, `Log::checkpoint()`, `engineChoices = 0`.
7. OR node with `args.canReuse`: rebuild from `backupNode` (`:206-219`).
8. `$choices = getChoices($player)`, `$allChoices = getChoices($player, true)`. If `$allChoices` is not
   empty and the node is not a leaf:
   - auto-choose `$id = first key of $choices` (`chooseNode($player, $id, true)`) when all of:
     `count($choices) == 1`, `count($allChoices) == 1`, same keys, `!forceManualChoice`, and one of
     (undo off or pref 152 disabled) / (`!irreversibleAction && !optionalAction`) /
     (`engineChoices == 0 && !optionalAction`);
   - otherwise `jumpToState(ST_RESOLVE_CHOICE)`.
   Leaves (and non-leaves with empty `$allChoices`) go to `proceedToState($node, $isUndo)`.

### 2.5 `proceedToState($node, $isUndo)` (`Core/Engine.php:265-282`)

Unless (`$isUndo` and the node is irreversible) or `flag == preActionDone`: call
`Actions::stPreAction(action, node)` (`Managers/Actions.php:164-175`; runs `stPre<Class>` if it exists, then
`Engine::checkpoint()` if the node is irreversible for its player), set `flag = preActionDone`
(overwrites any other flag), save; if the stPre method returned `true`, call `proceed()` and return.
Then `jumpToState($node->getState())`. For a leaf the state comes from `infos.state` or the action's
`getState()` (`LeafNode.php:112-124`). The state's action is `stAtomicAction`
(`States/EngineTrait.php:148-153`) → `Actions::stAction` (§3.1), except ST_CHOOSE_ASSIGNMENT which has no
action and just waits.

### 2.6 Choices

`AbstractNode::getChoices($player, $displayAllChoices)` memoizes `computeChoices` (`:441-446`).
`computeChoices` (`:448-512`):

- Candidates: for a SEQ only `[0 => childs[0]]` (even if resolved), otherwise all children with their
  indexes as keys.
- For each child: `$playerTest` = child's pId player, else `$player`, else active player. The child is a
  candidate if (unresolved, or the node is an OR with `args.canReuse` set) and (`$displayAllChoices` or
  `child->isDoable($playerTest)`). The choice entry: `id`, `description` (the SEQ's own description for
  a SEQ node, the child's otherwise), `args`, `optionalAction = (child->isOptional($playerTest) && child's
  pId (or playerTest) == playerTest) || node->isManualChoice()`, `automaticAction`, `independentAction`,
  `irreversibleAction`, `source`, `sourceId`, `player`, `forceManualChoice = node->isManualChoice()`.
  It is added if `description != ''` or the child is doable. The description therefore matters: in
  `allChoices`, a non-doable child with an empty description is left out.
- `$allOptional` is false as soon as one candidate child is not optional; `$hasOneAction` is true if a
  candidate belongs to `$playerTest`.
- **PASS** (key `99`): if (no choices) or (`$hasOneAction` and (node optional for `$player` or
  `$allOptional`)) or `engineChoices > 20`, and also (`count($choices) != 1` or the last candidate
  computed (`$choice`, which may not be in `$choices`) is optional or automatic), add
  `[id => 99, description 'Pass', irreversibleAction false, optionalAction false, args []]`.

**Independent-first rule** (`ParallelNode.php:112-130`): a PARALLEL node without `noIndependent` keeps,
among its choices, those with `independentAction && !optionalAction`; if there is at least one, it
returns only the first of them (`[$id => $choice]`). This applies to both `choices` and `allChoices`, so
§2.4 step 8 auto-chooses it (subject to the undo conditions). Mandatory independent reactions therefore
resolve automatically, in child order, before the player is asked to order the others. Note that
`ParallelNode::getChoices` is not memoized itself (it calls the memoized parent).

### 2.7 `chooseNode($player, $nodeId, $auto = false)` (`Core/Engine.php:295-366`)

1. `$node` = next unresolved; `$nodeId` must be a key of `$node->getChoices($player)` (else exception).
2. Not auto: `engineChoices++`, `Log::step()`.
3. PASS on a PARALLEL: every unresolved child whose pId is the player (or null) is resolved with `[PASS]`
   (an array); if another child is doable for its own player, the PARALLEL gets `pId` = that player (the
   decision moves to them); else the PARALLEL is resolved with `[PASS]`. Save, proceed.
   PASS on any other node: `Engine::resolve(PASS)` (scalar 99) on the next unresolved node, save, proceed.
4. `canReuse` (OR): allows re-choosing a resolved child and un-resolves it and its children.
5. If the chosen child belongs to another player and is optional for that player, the node gets
   `pId` = that player (no choice made); otherwise `$node->choose($nodeId, $auto)`.
6. Save, proceed.

`AbstractNode::choose` (`:514-529`): sets `choice`, `countChoices++`; if the child is not a leaf: an
optional SEQ child is entered (`child->choose(0)` → `optional = false`, `SeqNode.php:93-99`), otherwise if
not auto `child->enforceMandatory()`.

### 2.8 Resolution: node vs action

- `Engine::resolve($args)` (`:372-377`): `resolved = true`, `resolutionArgs = $args` on the next unresolved node.
- `Engine::resolveAction($args, $checkpoint, &$node)` (`:382-404`): on `$node` (default: next unresolved),
  unless `reusable`: `actionResolved = true`, `actionResolutionArgs = $args`, `optional = false`
  (`AbstractNode.php:579-584`); if `resolveParent`, resolve the parent with `[]`. Save. Then
  `engineAutomatic++` if `$args['automatic'] === true`, else `engineChoices++`. Then
  `Engine::checkpoint()` if `$checkpoint` is truthy.
- A LEAF is resolved when `resolved` or `actionResolved` (`LeafNode.php:23-26`). A leaf that inserted
  children has been replaced by a SEQ/PARALLEL (§2.9); that node carries `actionResolved` but is resolved
  only when its children are (`SeqNode.php:62-68`, `ParallelNode.php:101-107`). This is how "the action
  is done, its consequences are pending" is represented.
- `Action::resolveAction($args, $checkpoint)` (`Models/Action.php:145-151`) adds
  `$args['automatic'] = $this->isAutomatic(activePlayer)`, calls `Engine::resolveAction` on its ctx, then
  `Engine::proceed()`.
- `Actions::takeAction` (`Managers/Actions.php:104-131`) always calls, after the act method,
  `Engine::resolveAction(['actionName' => ..., 'args' => ...], $checkpoint, $ctx)` (without `automatic`,
  so `engineChoices++`) and `Engine::proceed()`. The 4th argument `$automatic` is ignored by `resolveAction`.

**Double resolution.** When an act method reached through `takeAction` already calls
`$this->resolveAction(...)` (which proceeds), the request proceeds twice: the first `proceed()` runs the
game until a state waits for input (possibly through a new `Engine::setup` for the next player or
phase); then `takeAction` resolves the old ctx again (harmless on the node, which may no longer be in
the tree, but it increments `engineChoices` of the *current* engine) and calls `proceed()` again, which
re-enters the state where the first chain stopped. Act methods in this situation (§3.3 column "self"):
`Target::actTarget`, `Pay::actPay` (when the player chose X), `RollDie::actRollDie` (also when called
from `stRollDie`), `InvokeToken::actInvokeToken` (player choice), `Spend::actSpend` (player choice),
`MarkRegion::actMarkRegion`, `MoveRegionMarker::actMoveRegionMarker`, `MoveExpedition::actMoveExpedition`
(only its branch at `Actions/MoveExpedition.php:236`). `Discard::actDiscard` only self-resolves when
called with `$automatic = true` from `stDiscard`, i.e. not through `takeAction`.

**Counters.** `engineChoices` counts player-visible decisions since the last checkpoint and drives undo
availability, auto-choice and auto-confirm; `engineAutomatic` counts automatic resolutions. Loop guards:
`isAutomatic`/`isIndependent` return false when `engineChoices > 20` (`AbstractNode.php:426`, `:437`,
`LeafNode.php:34`, `:44`); PASS is always offered past 20 (`AbstractNode.php:497`);
`ActivateCard::isOptional` is true when `engineAutomatic > 20` and `isIndependent` requires `< 20`
(`Actions/ActivateCard.php:110`, `:137`).

### 2.9 Tree mutation API

| Function | Behaviour |
|---|---|
| `insertAsChild($t, &$node)` (`Engine.php:483-502`) | null `$t` → nothing. A LEAF node is replaced in its parent by a SEQ with the same infos (`$node` is rebound to the new node); `$t` is appended as the last child. |
| `insertOrUpdateParallelChilds($childs, &$node)` (`:511-554`) | SEQ node: append to its first PARALLEL child if any, else push a new PARALLEL child with them. Other node: LEAF is replaced by a PARALLEL with the same infos, then each child is appended. Used by `Action::pushParallelChild(s)` (`Models/Action.php:175-183`). |
| `insertAtRoot($t, $last = true)` (`:425-436`) | push (or unshift if `$last = false`) a new root child. `unshift` puts it before every pending branch: it becomes the next unresolved node. |
| `getAfterFinishingNode()` (`:441-462`) | first unresolved root child with `flag == afterFinishing` (sets `activePId` on it), or a new root PARALLEL with that flag appended at the end. |
| `pushAfterFinishingChilds($childs)` (`:467-478`) | append each child to the after-finishing node. Deferred reactions go here: they resolve after the whole current root branch. |
| `updateParallelChilds($attrs, &$node)` (`:557-581`) | sets `$attrs` on the node if PARALLEL; recurses into SEQ children (stops at the first PARALLEL at each level). |
| `updateAfterFinishingChilds($attrs)` (`:584-595`) | sets `$attrs` on the after-finishing node (used for `noIndependent`). |
| `updateBrotherArgs($args)` (`:597-611`) | rebuilds siblings of the current node with merged args (no caller in the engine). |
| `Action::duplicateAction($args, $checkpoint)` (`Models/Action.php:391-409`) | copies the ctx as a LEAF with merged args, `duplicate = true`, without `mandatory`, inserts it right after the ctx (`insertAsBrother`, which wraps the ctx into a new SEQ if the parent is not a SEQ, `AbstractNode.php:75-89`); does not proceed. The copy keeps `flag` (so its stPre does not run again). |
| `forceListeningNode($sourceId)` (`Engine.php:719-739`) | adds `$sourceId` to `args.event.cardsToListen` of every unresolved ACTIVATE_CARD node about that card. |
| `hasPendingGroupedReaction` (`:687-694`, `AbstractNode.php:372-392`) | true if an unresolved ACTIVATE_CARD node for (listener, gaining card, gain group) is already queued. |
| `getResolvedActions`/`getLastResolvedAction` (`:707-716`) | resolved nodes of given action types (no caller outside Engine). |
| `clearZombieNodes` (`:699`) | resolves with `ZOMBIE` every node of a player (zombie path is dead code). |

Because `Action` stores its ctx by reference (`Models/Action.php:22-25`, `getCtx` returns a reference at
`:81-84`) and the Engine functions take `&$node`, the action's ctx follows the node when a leaf is
replaced by a SEQ/PARALLEL. `takeAction` reads it back with `$ctx = $action->getCtx()` before the final
`resolveAction` (`Managers/Actions.php:128`). The Rust port needs a node handle that is updated on replacement.

### 2.10 Node predicates

| | `isDoable($p)` | `isOptional($p)` | `isAutomatic($p)` | `isIndependent($p)` | `isIrreversible` | `isResolved` | `getPId` | `getUndoableMandatoryNode` |
|---|---|---|---|---|---|---|---|---|
| Abstract `AbstractNode.php` | true (285) | `infos.optional ?? (parent && parent.areChildrenOptional())` (402-405) | `count(getChoices($p)) < 2 && engineChoices <= 20` (423-427) | `!noIndependent && isAutomatic && all children independent && engineChoices <= 20` (430-439, memoized) | false (539) | `infos.resolved` (320) | `infos.pId` resolved for `nextPlayer`/`active`/`source` (223-243) | self if unresolved && !doable && (mandatory \|\| !optional) (289-295) |
| LEAF `LeafNode.php` | `willBeDoable` → true; resolved → true; else `player->canTakeAction` = `Action::isDoable` (84-107); no action → exception | false if `mandatory` or `Action::isMandatory()`; false if node pId set and ≠ player; parent rule if it is true or there is no action; else `Action::isOptional \|\| !Action::isDoable` (48-71). A non-doable leaf is optional. | no action → false; `Action::isAutomatic && engineChoices <= 20` | no action → false; `Action::isIndependent && engineChoices <= 20` | `Action::isIrreversible` | resolved or actionResolved | Abstract | Abstract |
| SEQ `SeqNode.php` | first child (`current()`): doable or optional; no child → false (32-42) | Abstract | Abstract | Abstract | false | resolved or all children resolved (62-68) | Abstract | null if optional, else first non-null among children (44-56) |
| OR `OrNode.php` | optional or any child doable (31-37) | Abstract or (`args.n` null and a child resolved with resolutionArgs `!= PASS`) (42-49); `areChildrenOptional` = a child resolved `!= PASS` (54-59) | Abstract | Abstract | false | resolved or (`getRemainingChoices() <= 0` and `childs[choice]` resolved) (68-75) | Abstract | Abstract |
| XOR `XorNode.php` | `infos.optional` or any child doable (31-37) | Abstract or no child doable (42-51) | Abstract | Abstract | false | resolved or any child resolved (57-67) | Abstract | Abstract |
| PARALLEL `ParallelNode.php` | optional or every child doable-or-optional (30-36) | false if every child is unresolved and belongs to another player; else node pId null or equal to player, and (Abstract rule or every child optional-or-resolved) (69-82) | Abstract | Abstract | false | resolved or all children resolved (101-107) | unresolved children grouped by pId: only "null" → null; one pId → that pId; else `infos.pId ?? Globals::activePId` (38-64) | null if optional, else first non-null among children (84-96) |

OR details: `getN()` (`OrNode.php:77-98`) = `args.n`, or `count(childs)`; `'sourceCounter1'` → source
card counter + 1; `'landmarks'` → number of landmarks of the source's player, capped by the number of
children when `canReuse` is set or `canReuse` is falsy (the condition at line 92 is always true), and
writes the computed `n` back into `args` (not saved). `getRemainingChoices = getN() - countChoices`.

`getResolutionArgs() != PASS` compares an array with `99` when the node was resolved with `[PASS]`;
in PHP 8 an array is never loosely equal to an int, so such children count as "resolved not by PASS".
Only `Engine::resolve(PASS)` stores the scalar 99.

### 2.11 Memoization (`Helpers/Evaluation.php`)

`Evaluation::run($key, $compute)` opens a scope at the outermost call and caches results by key until
that call returns. Node predicates use keys `node|spl_object_id|what|pId` (`AbstractNode.php:418-421`);
`Cards::getFiltered` uses `Evaluation::cached` (`Managers/Cards.php:319-331`, returns a copy of the
collection with the same card objects). The harness records DB writes performed inside a scope
(`FW:67-103`). The memo only removes repeated computations; it changes results only if a computation
has side effects (examples: `OrNode::getN` writing `args`, `Gain::getGainGroup` saving the tree).

### 2.12 Checkpoints, undo and confirmation

- `Globals::undo` = game option 151 (`Core/Globals.php:271`). Player prefs (`Core/Preferences.php:25-55`,
  values from `user_preferences`, compared loosely, `null` when missing): 103 `OPTION_CONFIRM`
  (0 disabled default, 2 enabled, 3 timer), 104 `OPTION_CONFIRM_UNDOABLE` (2 enabled, 0 disabled; the
  JSON default is the first key, 2), 152 `OPTION_PLAYER_UNDO` (1 enabled, 2 disabled).
- Logging: every `QueryBuilder` update/delete/insert writes a `log` row with the affected rows when the
  `logging` game-state value is 1 or the manager's `static::$log` is true (`Helpers/DB_Manager.php:24-27`,
  `Helpers/QueryBuilder.php:104-111`, `:172-188`). `Preferences` sets `$log = false`. `Log::enable()` is
  called by `Engine::setup` and when the player changes in `proceed`.
- Log entries: `checkpoint` (`Log::checkpoint`, `Helpers/Log.php:57-61`, also clears `newUndoableStep`
  notifications), `step` (`:64-67`, on each manual choice/action), `engine` (`:70-75`, at setup).
- `Engine::checkpoint()` (`Core/Engine.php:406-420`): throws `UserException` if the tree has an undoable
  mandatory node (`getUndoableMandatoryNode`) belonging to the active player; else `engineChoices = 0`,
  `Log::checkpoint()`. Called when an action resolves with a truthy checkpoint, after an irreversible
  stPre, by `Draw`, `DrawMana`, `DiscardFromDeck`, `Shuffle`, `Resupply` (checkpoint arg, true by default), `Exchange`,
  `DiscardDo`, `TargetExpedition`, `TargetPlayer` (truthy act return value), by
  `Cards::shuffleDeck` (deck reform listener, `Managers/Cards.php:806-812`), and by
  `confirmPartialTurn`.
- `restart()` (`:660-668`): `Log::undoTurn()` reverts to the last `checkpoint`/`engine` entry
  (`Helpers/Log.php:114-118`, `revertTo` at `:140-228` replays the log backwards: update → re-update old
  values, delete → insert, create → delete; then clears caches and sends a UI refresh), `Globals::fetch()`,
  `boot()`, `proceed(false, true)`. `undoToStep($id)` reverts to `$id - 1`.
- `confirm()` (`:616-634`): the tree must be finished; runs the callback.
- `confirmPartialTurn()` (`:636-655`): the next node must belong to another player; `checkpoint()`,
  `proceed(true)`.
- `stConfirmTurn` (`States/EngineTrait.php:221-228`): auto-confirms if pref 103 is 0 (or missing) or pref
  152 is 2.

For the Rust port the undo log can be replaced by snapshots, but the points where a snapshot is taken
(checkpoint, engine start, step) and where `engineChoices` is reset must stay identical, since they
change auto-choice and auto-confirm decisions.

---

## 3. Actions (`Models/Action.php`, `Managers/Actions.php`, `Actions/*.php`)

### 3.1 Dispatch (`Managers/Actions.php`)

- `$classes` (`:15-55`): the 39 action names; `get($id, &$ctx)` instantiates `\ALT\Actions\<id>` with the
  ctx by reference (`:57-64`). A new instance is created for every query (`isDoable`, `isOptional`, ...).
- Method naming (by `getClassName()`): `st<Class>` (state entry), `act<Class>` (default player act),
  `stPre<Class>` (before entering the state), `args<Class>` (state args), `actPass<Class>` (custom pass).
- `getArgs` (`:95-102`): `args<Class>()` merged with `optionalAction`.
- `stAction($id, $ctx)` (`:133-162`): if `Action::isDoable(activePlayer)` is false →
  `actPassOptionalAction(true)` (the pass checks `possibleactions`, so it throws in a state that does
  not list `actPassOptionalAction`, e.g. 88). Else run `st<Class>()`; a non-null return value is the
  argument list for `takeAction($id, 'act<Class>', $result, $ctx, automatic: true)`.
- `takeAction($id, $actName, $args, &$ctx, $automatic)` (`:104-131`): error unless
  `isDoable || isOptional` (active player); not automatic → `localCheckAction`, `Log::step()`,
  `newUndoableStep` notification; automatic → `checkPossibleAction`. Then `$action->$actName(...$args)`.
  `$args` from the client (JSON) or from `st` may be a list (positional) or a string-keyed map (named
  arguments in PHP 8.1); extra positional arguments are dropped silently. The act's return value is
  the checkpoint flag: `null` → false, anything truthy (non-empty array, `'exchange'`) → checkpoint.
  Then `Engine::resolveAction` + `Engine::proceed()` (§2.8).
- `stPreAction` (`:164-175`): see §2.5.
- `pass($id, $ctx)` (`:177-194`): requires `ctx->isOptional(active) || action->canPassAction(active)`;
  calls `actPass<Class>()` if it exists, else `Engine::resolve(PASS)`; then `proceed()`.

### 3.2 Base class `Models/Action.php`

| Member | Lines | Behaviour |
|---|---|---|
| defaults | 27-60 | `isDoable` true, `isOptional` false, `canPassAction` = `isOptional`, `isIndependent`/`isAutomatic`/`isIrreversible`/`isMandatory` false, `getState` null |
| `getPlayer()` | 67-71 | `Players::get(ctx->getPId() ?? activeId)` |
| `getCtxArgs()` / `getCtxArg($k)` | 86-99 | `ctx->getArgs() ?? []`; `[k] ?? null` |
| `$args` / `getArg($k)` | 101-113 | ctx arg if not null, else the class default in `$args`, else exception |
| `getEvent()` | 115-118 | `ctx->getEvent()` (node `infos.event`) |
| `getEventRecursive($ctx)` | 120-132 | first non-null `event` going up the parents |
| `getSourceId()` / `getSource()` | 134-143 | `ctx.sourceId`; the card or null. Several actions read `$this->ctx->getSource()` (the node's `source` info, usually null) and fall back to `Cards::getSingle($sourceId)`. |
| `resolveAction($args, $checkpoint)` | 145-151 | §2.8 |
| `insertAsChild($flow)` | 156-162 | empty → nothing; `Engine::insertAsChild($flow, $this->ctx)` |
| `pushAfterFinishingChilds`, `pushParallelChild(s)`, `updateParallelChilds`, `updateAfterFinishingChilds` | 167-193 | wrappers of §2.9 |
| `insertBonusesFlow` | 198-213 | only handles flows already in node form (FlowConvertor call commented out) |
| `checkListeners($method, $player, $args, $override)` | 237-275 | §4.4 |
| `filterGroupedReactions` | 283-297 | §4.5 |
| `logReactions` | 299-315 | returns `Cards::getReaction(event)` without queuing (used by NightCleanup) |
| `checkAfterListeners($player, $args, $during = true, $override = null)` | 326-334 | `checkListeners(className, ...)` when `$during` |
| `checkImmediateListeners($player, $args)` | 336-364 | §4.4 |
| `checkModifiers`, `checkCostModifiers`, `checkArgsModifiers` | 366-384 | `Cards::applyEffects` over cards that have `on<Method>` methods; no caller, card classes have no such methods |
| `duplicateAction` | 391-409 | §2.9 |
| `updateCardId($node, $cardId, $cardFrom, $sourceId, $ownerId, $preserveEffect)` | 442-514 | binds a picked card into a nested effect flow: sets `args.cardId/cardFrom/ownerId` (and `wasGigantic` for cards in expeditions) unless the node already holds `ME`/`mana` (or `EFFECT` when preserving), skipping TARGET nodes unless they need the prior pick (`targetNeedsPriorSelectionCtx`, 519-531); `pId == 'owner'` → `$ownerId`; sets `sourceId` on every node; recurses into `1-3`, `4+`, `args.effect` (TARGET does not pass its cardId down), `args.oppositeEffect`, `childs`. |

### 3.3 The 39 actions

"State" = state id/type used (`g` = game, `ap` = activeplayer). "self" = act/st method calls
`$this->resolveAction` itself (§2.8). Overrides list the methods besides `getState`/`getDescription`.

| Action | State | What it does | Overrides / notes |
|---|---|---|---|
| ActivateCard | 27 g | Resolves a passive reaction: rebuilds the card's flow with `Cards::applyEffect` (conditions re-checked now), tags it with `sourceId`, `event`, `pId`, and replaces itself with the flow (`stActivateCard`, `Actions/ActivateCard.php:159-188`). See §4.6. | `getCard`, `getFlow`, `getFlowTree`, `isOptional`, `isAutomatic` (true), `isDoable`, `isIrreversible`, `isIndependent`; st does not resolve, it replaces the node and proceeds |
| ActivateEffect | 36 g | Re-activates the `{J}`/`{R}`/`{D}` effect (`effectType` Played/Reserve/Support) of one or several cards; pushes them as parallel child (OR with `n`, XOR, or single); `{D}` also updates ability counters and fires ChooseAssignment listeners (`:164-263`). | `isAutomatic`, `isIndependent` (true), `getCard`; st self |
| AfterYou | 31 g | Pays an optional mana cost and counts as the turn's played card (`playedCards++`). | `isDoable`; st self |
| BlockExpedition | 42 ap | Adds an expedition (or all) to `Globals::blockedExpeditions`. | `argsBlockExpedition`, `isAutomatic` (`expedition == 'all'`), `stBlockExpedition` returns `['all']` |
| BoostExchange | 50 ap | Swaps the boost counts of two cards, firing `Gain` listeners on the card that gained. | `isAutomatic`/`isIndependent` true, `getPlayer`; st self |
| CheckCondition | 30 g | Evaluates `condition(s)` with `Conditions::check`; met → pushes `effect`, not met → pushes `oppositeEffect` or resolves `['notMet']`. Effects that need a deck fail when the deck is empty. | `isIndependent` (false if a card's `blockAutomaticAction[CheckCondition]` lists a condition), `isDoable`; st self |
| ChooseAssignment | 20 ap (no st) | The afternoon turn action: `actPlay` (`playCard`), `actSupport` (discard a reserve card for its `{D}`), `actTap` (exhaust a card for `{T}`), `actPass`. `playCard` (`Actions/ChooseAssignment.php:267-1028`) pays, moves the card, applies next-character modifiers, builds `{J}`/`{H}`/`{R}` effects (spells: `SEQ(PAR(effects), SPELL_CLEANUP)` inserted as child; others pushed on the after-finishing node), fires Immediate and After listeners. | `argsChooseAssignment`, `isOptional`, `isDoable`, `actPassChooseAssignment` (resolves the ctx with `[PASS]`, and the parent SEQ when `mandatory`), `actPlay`, `actSupport`, `actTap`, `actPass`; `playCard` calls `Engine::resolveAction()` itself for non-spells |
| CompleteFeat | 56 g | Puts a `featCompleted` meeple on the target card (once). | `isAutomatic` true; st self |
| Discard | 21 ap | Moves cards to `destination` (discard, reserve, hand, mana, topOfDeck, destroy for tokens) with sacrifice/sabotage/Jinn/protection rules, fires Discard listeners with `cardsToListen`. `stDiscard` auto-runs for forced targets (`cardId`, `special` allHand/allHandReserve/allReserve, `n == ME`). | `isOptional` (`canPass`), `canPassAction`, `isDoable`, `getPlayer`, `argsDiscard`, `stDiscard`, `actDiscard` (self only when automatic) |
| DiscardDo | 44 ap | Player discards up to n cards, then the effect runs with `n = count`. | `isOptional` true, `argsDiscardDo`, `actTarget` returns `[$cardIds]` → checkpoint |
| DiscardFromDeck | 57 g | Mills n cards from the deck of me/opponent/all. | `isAutomatic` false, `isIrreversible` true, `getPlayer`; st self with checkpoint |
| Draw | 28 g | Draws n cards (or 1 to mana) for me/opponent/owner/all, fires Draw listeners. | `isAutomatic`, `isIrreversible` true; st self with checkpoint |
| DrawMana | 41 g | Top n deck cards to mana, tapped by default. | `isAutomatic`, `isIrreversible`, `isDoable` true, `getPlayer`; st self with checkpoint |
| EndAfternoon | 51 g | Passes for the rest of the afternoon (adds to `skippedPlayers`, sets `firstPass`), inserts `EndTurn` reactions as a child PARALLEL. | `isDoable`; st self |
| Exchange | 48 ap | Swap one reserve card with one hand card (two DISCARD nodes as child). | `isOptional`, `isDoable`, `argsExchange`, `actExchange` returns `'exchange'` → checkpoint |
| Exhaust | 46 g | Exhausts a card in play/reserve, fires Exhaust listeners. | `isDoable` (not tapped); st self |
| Gain | 22 g | Gives meeples (boost, fleeting, anchored, asleep) or a counter (`augment`) to a card, respecting `upTo`, block rules and `dynamicGainReplace`; fires Gain listeners with `gainGroup`. | `isAutomatic` true, `isDoable`, `isIndependent`, `getPlayer`; `gain()` is also called directly by ChooseAssignment; st self |
| InteruptReveal | 54 ap | Player confirms a reveal; cards in limbo go back to hand. | `argsMarkRegion` (copy-paste name, so no args are used), `actInteruptReveal` |
| InvokeToken | 26 ap | Creates token card(s) in an expedition/landmark, applies next-token modifiers, fires listeners. | `argsInvokeToken`, `isAutomatic`/`isIndependent` (node override or single location), `getPlayer`; st and act self |
| Loose | 24 g | Removes meeples of a type (or counter via USE_COUNTER), fires Loose listeners. | `isAutomatic`/`isIndependent` (`!upTo`, block rules), `getPlayer`; st self |
| MarkRegion | 52 ap | Moves a terrain marker from the source card to a visible region (`stPreMarkRegion` may create it). | `argsMarkRegion`, `isDoable`, `stPreMarkRegion`; act self |
| MoveCard | 39 g | Moves characters to the opposite expedition (with anchored/boost protection), fires listeners. Line 99 inserts `['action' => BOOST]`, which is not an action name and would throw. | `isAutomatic` true; st self |
| MoveExpedition | 34 ap | Moves a storm token forward/backward n regions, records `expeditionMoves`, fires listeners; forced expedition comes from `forceExpedition`. | `argsMoveExpedition`, `isDoable`, `isAutomatic` (one choice), `stMoveExpedition` returns the expedition; act self in one branch |
| MoveRegionMarker | 53 ap | Moves a terrain marker to the source's storm token region. | `argsMoveRegionMarker`, `isDoable`, `isOptional` true; act self |
| NightCleanup | 88 ap | Player chooses reserve/landmark cards to discard at night; reactions stored in `afterNightCleanup`. | `argsNightCleanup`, `actNightCleanup` |
| Pay | 40 ap | Pays mana; fixed positive `pay` is paid in `stPay`, `X` is chosen by the player. | `isDoable`, `isMandatory` true, `argsPay`, `stPay`, `actPay` self |
| PlayCard | 38 ap | Plays a given card (free or not) through `ChooseAssignment::playCard` with the same ctx. | `getPlayer`, `getCard`, `isDoable`, `argsPlayCard`, `stPlayCard` (forced location) |
| Ready | 47 g | Readies a card (or one tapped mana card). | `isDoable` true; st self |
| Resupply | 33 g | Top deck card to reserve (Ouroboros: draw 2 keep 1), fires Draw listeners. | `isAutomatic` (node override or `!isOptional`), `isIrreversible` true, `isDoable` true, `getPlayer`; st self with checkpoint |
| RollDie | 32 ap | `stPreRollDie` rolls `n + addDice` dice with `bga_rand(1,6)` (5 in studio), adds `+i` variants, checkpoints, fires ImmediateRollDie, returns true (interrupt); the player then picks a result and the matching effect (`1-3`, `4+`, `die` placeholder) is inserted. `actDiscardAdd` discards reserve cards for +2. | `getN`, `isAutomatic`, `isIrreversible` true, `getSource`, `stPreRollDie`, `argsRollDie`, `stRollDie` (single result), `actRollDie` self, `actDiscardAdd` (duplicateAction) |
| Shuffle | 55 g | Reforms the deck from the discard and shuffles it. | `isAutomatic`/`isIndependent` false, `isOptional`/`isIrreversible` true; st self with checkpoint |
| SpecialEffect | 29 g | Big switch of bespoke effects (§6.2). | `isAutomatic` true, `isIndependent` always false, `getCard`; st self |
| SpellCleanup | 25 g | After a spell's effects: fleeting → discard, else reserve (exhausted if `cooldown`); fires Immediate and After listeners as `ChooseAssignment`. | `isAutomatic`/`isIndependent` true, `getCard`; st self |
| Spend | 49 ap | Spends n boosts (priority) or counters on a card, then inserts `effect` bound with `updateCardId`. | `isAutomatic` true, `getSource`, `getCard`, `argsSpend`, `isDoable`, `stSpend` (when `automatic` arg), `actSpend` self |
| Tap | 37 g | Exhausts a card as a cost (optional mana `pay`), records ability activation, fires Tap listeners. | `isDoable`, `getCard`; st self |
| Target | 23 ap | Player picks cards matching many filters (type, location, costs, biomes, statuses...), pays Tough, and each pick gets its own copy of `effect` (bound with `updateCardId`) pushed as parallel child, or one copy with all ids when `allIds`. `stTarget` auto-picks when the choice is forced. | `isDoable`, `isOptional`, `getTargetableCards`, `getTargetCosts`, `argsTarget`, `stTarget`, `actTarget` self |
| TargetExpedition | 45 ap | Picks expedition(s); pushes the effect per expedition with `player`/`expedition`/`forceExpedition` args. | `argsTargetExpedition`, `isDoable`, `stTargetExpedition` (`'all'`), `actTargetExpedition` returns `[$expedition]` → checkpoint |
| TargetPlayer | 43 ap | Picks a player; inserts the effect as child with `pId` = picked player. | `argsTargetPlayer`, `stTargetPlayer` (only me / single opponent), `actTargetPlayer` returns `[$pId]` → checkpoint |
| UseCounter | 35 g | Pays mana and consumes counters of the source card. | `isDoable`; st self |

---

## 4. Reactions and listeners

### 4.1 Card passive data

`effectPassive` (`Models/Card.php:84`) maps an event name to a power:
`['condition' => str|list, 'conditions' => list, 'listeningConditions' => list, 'output' => flow,
'oppositeOutput' => flow|'OPPOSITE', 'payment' => flow, 'n' => 'once'|'eachExpedition', 'pId' => ...,
'immediate' => bool, 'forceListening' => bool, 'cardId', 'wasGigantic', 'childs' => [power, ...]]`.
Event names used by the engine:

- phase events (`checkCardListeners`): `Noon`, `Morning`, `BeforeDusk`, `AtDusk`, `AfterDusk`,
  `BeforeNight`, `EndOfNight`;
- action events: the class name of the action that fires (`Draw`, `Discard`, `Gain`, `Exhaust`, `Tap`,
  `Loose`, `MoveCard`, `MoveExpedition`, `InvokeToken`, `RollDie`, `Resupply`, `SpecialEffect`,
  `ChooseAssignment`...) or the override (`ChooseAssignment` from SpellCleanup/ActivateEffect, `Discard`
  from actSupport, `Exhaust` from actTap, `EndTurn` from actPass/EndAfternoon, `Gain` from BoostExchange,
  `SpecialEffect` from augment, `EatMeEnergyBars`);
- `Immediate<Class>` (`checkImmediateListeners`, e.g. `ImmediateChooseAssignment`, `ImmediateRollDie`);
- leave events from `Card::checkLeaveListener` (`Models/Card.php:675-776`): `LeaveExpedition`,
  `LeaveLandmark`, `LeaveOther`, and `OtherLeaveExpedition`/`OtherLeaveLandmark`/`OtherLeaveOther` for
  the other cards.

### 4.2 `Card::isListeningTo($event)` (`Models/Card.php:803-847`)

1. If the card is not in `event.cardsToListen`, is in Reserve and not tapped: use
   `effectInfinity['effectPassive']`; return false if absent. Otherwise use `getEffectPassive()` (which
   itself falls back to `effectInfinity['effectPassive']` for an untapped reserve card without
   `effectPassive`, see §5.4).
2. `$action = resolvePassiveEventAction($passive, $event)` (`:782-798`): `event.action ?? 'none'`; for
   support events (`isSupport === true`) `Discard` and `ChooseAssignment` alias each other when only the
   other is registered.
3. False unless `event.type` or `$action` is a key of the passive.
4. Phase events: false unless `event.pId == card.pId`.
5. If `event.action` is set and the power has `listeningConditions`: each `"fn:arg1:arg2"` is called as
   `Conditions::fn($card, $event, 'arg1', 'arg2')`; any `=== false` → false.

`listeningConditions` are checked when the reaction is collected; `condition`/`conditions` are checked
when the reaction runs (§4.6).

### 4.3 `Cards::getReaction($event, $returnNullIfEmpty = true, $ownerPId = true)` (`Managers/Cards.php:940-984`)

- Listening cards (`getListeningCards`, `:894-926`): the cards in `stormLeft`, then `stormRight`,
  `landmark`, `reserve`, `board-hero%` (`:928-935`), each group in cache order (card id order for cards
  loaded at request start, then creation order), filtered by `isListeningTo`; then cards of
  `event.cardsToListen` and `event.reserveToListen` not already listed (fetched with `getMany`, so in
  the given id order after de-duplication).
- None → null (when `$returnNullIfEmpty`).
- For each card, from a fresh copy of the event: reserve cards get `reserveToListen[] = id`;
  `sourceLocation` = current location; the node is
  `['action' => 'ActivateCard', 'pId' => card.pId (or event.pId when $ownerPId false), 'args' => ['cardId' => id, 'event' => event], 'sourceId' => id]`,
  plus `'immediate' => true` if `isImmediateReaction` (`Models/Card.php:856-875`: power for
  `event.action ?? event.type` has `immediate === true`).
- The `forceListening` branch (`:956-958`) tests `in_array($listenCard, STORMS)`, a Card object against
  strings, which is always false: dead code.

### 4.4 Queuing

- `Action::checkListeners` (`Models/Action.php:237-275`): event =
  `['pId' => player, 'type' => 'action', 'action' => override ?? className, 'method' => override ?? method] + args`
  (`array_merge`, args win); `getReaction`; `filterGroupedReactions`; split: `immediate` reactions
  (flag removed) → `pushParallelChilds` on the current ctx (they resolve inside the current branch);
  the others → `pushAfterFinishingChilds` (they resolve after the current root branch). Order inside a
  group = order from `getReaction`.
- `Action::checkImmediateListeners` (`:336-364`): event action `Immediate<Class>`, `getReaction($event, true, false)`
  (reaction pId = event pId); more than one → `insertAtRoot(PARALLEL pId=event.pId noIndependent, ..., last: false)`,
  one → `insertAtRoot(SEQ, last: false)`. Unshifting at root makes them the next nodes to resolve.
- `Card::checkLeaveListener` (`Models/Card.php:675-776`), called by `discardTo` before the move: if the
  card listens to its own `Leave*` event (with `cardsToListen = [id]`), queue an ACTIVATE_CARD node with
  the card's pId — on the after-finishing node, or in `Globals::afterNightCleanup[pId]` during night
  cleanup; for a landmark sacrificed at night also a `Discard` event node. Then `Other<Leave*>` reactions of
  other cards go to the after-finishing node or to `afterNightCleanup`.
- Night: `NightCleanup::actNightCleanup` stores reactions in `afterNightCleanup` (`Actions/NightCleanup.php:123-145`);
  they run at `EndOfNight` (§1.7). Note that this action calls `checkLeaveListener` explicitly
  (`:110`, `:116`) and again through `discardTo` (`:120` → `Models/Card.php:630`), so leave reactions can
  be registered twice.
- `Globals::afterRest[pId]` (filled by SpecialEffect `afterRest`/`AfterRest*`) runs at `BeforeNight`.
- Ordering rules for the player: reactions accumulated on the after-finishing PARALLEL are offered as
  choices; mandatory independent ones auto-resolve first (§2.6). `noIndependent` is set when a played
  card has its own effects and other passives were added (`Actions/ChooseAssignment.php:968-974`), for
  Unique cards' effects (`:921-923`), for multi-child phase reactions (`game:281`), for the night
  cleanup parallel (`States/TurnTrait.php:383`), and for multi-output powers of Unique cards
  (`Models/Card.php:916`).

### 4.5 `filterGroupedReactions` (`Models/Action.php:283-297`)

When the event has a `gainGroup` (gains inside a `SEQ_DISTRIBUTE_GAINS`), a reaction is dropped if an
unresolved ACTIVATE_CARD node for the same (listener, gaining card, group) is already in the tree. The
group id is created lazily on the SEQ by `Gain::getGainGroup` (`Actions/Gain.php:229-244`) as
`'gains-' . substr(md5(uniqid('', true)), 0, 12)`; only equality matters.

### 4.6 Running a reaction: `ActivateCard`

- `getCard` (`Actions/ActivateCard.php:20-30`): `Cards::getSingle(cardId)`; null if missing, or in
  `destroy` and not in `cardsToListen`.
- `getFlow` (`:32-94`): only if the card is played (expeditions, landmark, or hero), in Reserve, or in
  `cardsToListen`/`reserveToListen`: `Cards::applyEffect($card, $player, $event['method'], $event)`
  (`Managers/Cards.php:1281-1335`), which calls `Card::getReactions($event)` (`Models/Card.php:877-949`):
  - passive = `effectInfinity['effectPassive']` when the card is not in `cardsToListen` and is in Reserve
    (tapped or not) or in `reserveToListen`; else `effectPassive`;
  - power = `passive[event.action ?? event.type]` (the raw action, not the alias of §4.2);
  - power with `childs`: each child through `checkReaction`, result `PARALLEL` of the outputs,
    `noIndependent` for Unique cards; else `checkReaction($power, $event)` (`:951-999`): copies
    `power.pId` onto `output` (and `oppositeOutput`); `n = 'once'`: `Conditions::check($power, $card, $event)`,
    false → `oppositeOutput` (unless `'OPPOSITE'`) or nothing; `'eachExpedition'`: for each of
    `stormLeft`, `stormRight` with `event.expedition` set, collect output or oppositeOutput, wrap in SEQ;
    `output.sourceId = card id`; returns `[payment ?? [], output]`.
  - `applyEffect`: output empty → null; tag `sourceId` on the whole tree (`Utils::tagTree`), bind
    `ownEffect` support activations to the card (`Utils::bindOwnEffectActivateCardId`); if there is a
    payment: `SEQ optional [payment, output]` (the `tagTree` on the payment at `:1317` discards its
    result); else return the output.
- The flow is then tagged on every node (`Utils::tagTree`, `Helpers/Utils.php:139-166`, overwrites
  existing values) with `sourceId`, `event`, and `pId` = `event.controller ?? event.pId` if the flow's
  root `pId` is `'controller'`, else `event.owner` if set, else `event.cardPId` if the event is about
  this card, else the card's current `pId`. For events coming `from` an expedition, `targetLocation`
  values equal to `['source']` are replaced by `[event.from]`.
- `stActivateCard` replaces the node by `buildTree($flow)` (with `optional = false` if the node was
  `mandatory`), saves and proceeds. `getFlow` is recomputed for each of `isOptional`, `isDoable`,
  `isIrreversible`, `isIndependent`, `getDescription`; conditions are re-evaluated each time.

### 4.7 effectInfinity

`effectInfinity` holds properties that apply while the card sits in Reserve: the magic getter returns
`effectInfinity[name]` for an untapped reserve card when the property itself is not set (§5.4), and
the passive lookups above use `effectInfinity['effectPassive']` for reserve cards. Reserve cards are part
of `getListeningCardsObject`, and `Player::getInfinityCards` (`Models/Player.php:212`) is merged with played
cards for some modifiers (e.g. `getAddRoll`, `:396`).

---

## 5. Data model

### 5.1 Tables (`AlteredBGA/dbmodel.sql`)

- `cards`: `card_id` (auto-increment), `card_location` varchar, `card_state` int (order inside a
  location), `player_id` (controller), `properties` JSON.
- `meeples`: `meeple_id`, `meeple_location`, `meeple_state`, `type`, `player_id`.
- `player`: BGA columns plus `faction`.
- `global_variables`: `name`, `value` JSON.
- `user_preferences`: `player_id`, `pref_id`, `pref_value`.
- `log`: undo log (`move_id`, `table`, `primary`, `type`, `affected` JSON); `gamelog.cancel` added.

**Card locations**: `deck-<pId>` (top = highest state), `reveal-<pId>`, `hand` (one location for all
players, owner = `player_id`), `reserve`, `stormLeft` (hero expedition), `stormRight` (companion
expedition), `landmark`, `mana`, `discard`, `limbo` (spell being resolved, reveals), `destroy` (removed
tokens), `board-hero-<pId>`, `choice`, and setup-only `deck-<n>`/`deck-API`. `IN_PLAY` = expeditions +
landmark (`consts:250`).

**Meeple types and locations**: storm tokens `hero` and `companion` on `storm-<0..7>` (hero starts at 0
and moves up, companion starts at 7 and moves down, `Models/Player.php:578-624`); `boost`, `fleeting`,
`anchored`, `asleep`, `featCompleted` on `card-<cardId>`; terrain markers `ocean`/`forest`/`mountain` on
`card-<heroId>` or `storm-<n>`; `ascend` markers on expeditions (deleted at night,
`Managers/Meeples.php:115-123`).

### 5.2 Globals (`Core/Globals.php:15-98`)

Access through `__callStatic` (`:169-221`): `get<Name>`, `set<Name>` (casts int/bool, writes JSON),
`inc<Name>` (int only), `is<Name>` (bool only). Variables missing in DB are created with the type
default (int 0, obj `[]`, bool false, str `''`). `fetch()` loads all rows; `cast` casts int variables
(`:102-108`). `getStorm($ui)` reads `self::$data` directly (`:274-290`). `isBreak()` (`:161-164`) calls an
undeclared variable and would throw (no caller).

| Name | Type | Meaning |
|---|---|---|
| engine | obj | serialized engine tree |
| engineChoices | int | player decisions since last checkpoint (§2.8) |
| engineAutomatic | int | automatic resolutions since setup |
| callbackEngineResolved | obj | `['order' => key]` run by `Engine::confirm` |
| anytimeRecursion | int | unused |
| customTurnOrders | obj | §1.5 |
| playerDecks | obj | pId → deckNumber → deck info (preco selection) |
| deckOptions | str | set at setup, not read |
| deckContent | obj | pId → chosen deck content (API/random/preview), used for the end report |
| tokenStyles | obj | pId → tokenType → art (InvokeToken) |
| undo | bool | game option 151 |
| statMapping | obj | pId → card id → stat name |
| deckFormat | str | FRONTIER/STANDARD/SINGLETON_NUC/SANDBOX/NO_UNIQUE/TEST/DEMO/SEALED |
| firstPlayer | int | first player of the day |
| skippedPlayers | obj | list of pIds that passed (afternoon) or did their morning step |
| dayPhase | bool | true during the afternoon assignment |
| activePId | int | player whose afternoon turn it is |
| zombie | bool | set by `zombieTurn` |
| storm | obj | 5 tumult cards `[cardId, visible, rotated]` |
| day | int | day number |
| phase | int | 0..4 (§1.6) |
| heroes | obj | unused |
| playedCards | int | cards counted as the turn play (ends the afternoon turn) |
| playedCardsHistory | obj | pId → uid → count (end report) |
| stormMoves | obj | pId → expedition → `{biomes, moves, ascended, hasMovedFromAscension}` (dusk moves) |
| expeditionMoves | obj | pId → expedition → moves made by effects |
| blockedExpeditions | obj | pId → list of blocked expeditions |
| tieBreakerMode, enterTieBreakerMode, instantWin | bool | end-of-game modes |
| diceRolls | obj | current roll values |
| firstDayManaSelection, deckSelection | obj | multiactive selections |
| nightSelection, newDayManaSelection | obj | unused |
| costReduction | obj | pId → type/subtype/ALL → `{reduction, minimum, permanent}` |
| nextCharacterBoost(+Occurence), nextReserveCharacterBoost, nextAnimalBoost(+Occurence), nextCharacterBoostV | int | pending boosts for the next played card |
| nextCharacterCost3Anchored, nextCharacterBaseCost3Anchored, nextCharacterAnchored, nextCharacterFleeting, nextTokenAnchored, nextTokenAsleep | bool | pending statuses for the next card/token |
| nextCharacterAsleep | obj | (declared obj, set to `false` by `stAssignment`) |
| nextCharacterInExpeditionBoost | obj | pId → expedition → boost |
| additionalEffect | obj | list of effects to add to the next character's `{J}`/`{H}`/`{R}` |
| afterRest, afterNightCleanup | obj | pId → list of nodes to run at BeforeNight / EndOfNight |
| nextSpellIsFree, playedForFree | bool | free plays |
| removeFleetingIfSpellPlayedHand, removeFleetingSpellPlayed, removeFleetingCharacterPlayed, removeFleetingCharacterStat0Played, removeFleetingSongArtistPlayed | bool | remove fleeting from the next matching card |
| globalTough | obj | extra Tough (Kauri's intervention) |
| turnCards | obj | pId → cards played this day |
| abilityActivatedThisTurn, ...Count, ...TypeCount | obj | pId → activated ability tracking |
| smokeThemOutArmed | obj | pId → cardId → charges |
| samSpookEverDiscarded | obj | card-specific flag |
| testingOption | bool | 15 cards / 10 mana at day 1 |
| beginner | int | option 150 |
| firstPass | int | first player who passed this day (-1 at morning) |

### 5.3 `CachedPieces` (`Helpers/CachedPieces.php`, used by `Cards` and `Meeples`)

All rows are loaded once per request into `static::$datas`, a `Collection` keyed by id in SQL result
order (`:71-76`); every query filters this in-memory collection (`where`, `:89-102`), and every write goes
through the model setters (DB + object). `Helpers/Pieces.php` (DB-query variant) is not used by any manager.

- `get($id, $raise = true)` (`:201-205`): one result → the object; otherwise the `Collection` (an
  empty `Collection` when missing with `$raise = false`). `getSingle` returns null instead (`:234-238`).
  `getMany` (`:207-232`) de-duplicates ids (as strings) and keeps the requested order.
- `getExtremePosition` (`:243-251`): max/min state, 0 when empty. `getTopOf($loc, $n)` (`:256-263`):
  `orderBy('state', 'DESC')` then the first `$n`; `orderBy` is a stable `uasort` so ties keep cache order.
- `move($ids, $loc, $state = 0)` (`:316-331`), `moveAllInLocation` (`:338-351`, state reset, filtered by
  `pId` when the target is `<x>-<pId>`).
- `pickForLocation($n, $from, $to, $state, $deckReform)` (`:367-393`): moves the top `$n`; if fewer
  than `$n` and the location (or its prefix before `-`) is in `autoreshuffleCustom` (`deck → discard`
  for cards) and `$deckReform`, reforms the deck and picks the rest. `Cards` overrides the default
  `$deckReform` to **false** (`Managers/Cards.php:814-817`), so draws never reshuffle automatically;
  reshuffle only happens through the SHUFFLE action.
- `reformDeckFromDiscard($from)` (`:403-421`): moves the `discard` cards whose `pId` matches the
  `-<pId>` suffix of `$from` into `$from` (`moveAllInLocation` filter, `:346-348`), shuffles, calls
  `Cards::shuffleDeck` (checkpoint + notifications).
- `shuffle($loc)` (`:426-434`): ids of the location in cache order, PHP `shuffle`, then state = index.
- `insertOnTop` (`:436-440`): state = max + 1; `insertAtBottom`: min − 1 (no renumbering);
  `moveAtPosition` (`:453-474`) renumbers the location so the card is at position N from the top.
- `create` (`:519-596`): multi-insert, new objects appended to `$datas`; `delete` (`:479-499`) removes them.

### 5.4 Card model (`Models/Card.php`)

- DB attributes (`:26-34`): `id` int, `location` string, `state` int, `pId` int, `properties` obj.
- Instantiation (`Managers/Cards.php:51-87`): Unique cards (`rarity == 2`) → `new Card($row)` with all
  properties from the DB JSON. Other cards → the data class `\ALT\Cards\<faction>\<faction>_<Rarity>_<slug>`
  (properties from the class), then the `DYNAMIC_PROPERTIES` (`consts:415`: tapped, extraDatas, setIcon,
  flavorText, asset, revealed, subtypes, typeline, fullArt, mainAsset) are overlaid from the DB JSON.
  If the class does not exist the function returns null. Writing any property (`setProperty`, `:293-299`)
  stores the **whole** properties array in the DB.
- Typed properties (`:42-204`) are read through `__call` (`:231-286`), with method names parsed by
  `DB_Model::parseMethod` (`get|set|inc|is` + capitalised name, `Helpers/DB_Model.php:72-80`):
  - `get<Prop>()`: the property if set (with one argument and an `obj` type, returns
    `properties[name][arg]`); else, if `name != 'effectInfinity'`, the card is in `reserve`, not tapped,
    and `effectInfinity[name]` is set: that value (the **effectInfinity fallback**); else the default for the
    type (int 0, bool false, str `''`, obj `[]`).
  - `is<Prop>()`: `(bool) properties[name]`, false if unset (no effectInfinity fallback).
  - `set<Prop>($v)`: `setProperty` (DB write).
  - Other names go to `DB_Model::__call` (`Helpers/DB_Model.php:82-165`): DB attributes (`setLocation`,
    `setState`, `setPId` write the row; int/bool setters skip the write when unchanged); unknown names throw.
- Explicit methods override the magic getters for computed values: `getCost` (1001), `getBiomes` (1102),
  `getTough` (1214), `isSeasoned` (1330), `isGigantic` (1382), `getReserveSlots` (1451),
  `isOppositeDefender` (1467), `isIgnoreDefender` (1556), `isBlockingPower` (1574), `isEternal` (1589),
  `isDefender` (1602), plus `canBePlayed` (340), `getPlayableLocation` (449), `getScoutableLocations` (553).
- Tokens: `hasToken`, `countToken`, `hasCounters`, `countCounters`, `getOfType` (`:585-615`); numeric
  counters live in `extraDatas['counter']` (+ `counterName`), the original owner of a stolen card in
  `extraDatas['pId']`, the used flag in `extraDatas['userPower']`.
- `isPlayed()` (`:317-323`): expeditions, landmark, or hero type. `isMana()` (`:325-328`) assigns
  `location = 'mana'` and returns a truthy string (bug; no caller found outside data).
- `discardTo($loc, $seasoned, $afterNight, $gigantic)` (`:627-656`): `checkLeaveListener` first, then
  location, restore owner from `extraDatas.pId`, untap, delete meeples (seasoned cards going to reserve
  keep boosts), clear named counters when going to discard/hand/topOfDeck. `getOwner()` (`:666-673`).

### 5.5 Players and Player

`Players` (`Managers/Players.php`) extends `CachedDB_Manager` (all rows loaded once; `get($id)` filters
by id and returns null when missing; `get(null)` = active player).

| Theme | Functions |
|---|---|
| identity / order | `getActiveId` 62, `getCurrentId` 67, `get` 75, `getActive` 80, `getCurrent` 85, `getNextId` 90 (`is_int` check: a string id is treated as a Player object), `getNext` 97, `getPrevious` 102, `count` 112, `getTurnOrder` 132 |
| opponent powers | `getPowersBlockedExpeditions` 144, `hasOpponentBlockingPower` 156, `hasOpponentBlockMoveExpedition` 170, `hasOppositeDefender` 184, `getOpponentAdditionalCost` 195, `getOpponentMinimumCost` 207, `getMinimumReserveCost` 219, `hasBlockOpponentReserveGain` 228, `hasBlockGainNewCounters` 239 |
| storm / victory | `checkVictory` 249, `getWinningPlayerByStorms` 380, `getBlockedExpeditions` 459, `getActionInsteadOfAdvance` 496, `getBiomesInStorm` 513, `filterBiomes` 531, `excludeBiomes` 554, `computeStorm` 572 (pure unless `$advance`), `biomesModifier` 777, `updateBiomesModifier` 857, `getRegionsInfo` 876 |
| slots and totals | `getBiomeTotals` 316, `getDefenders` 326, `getReserveSlots` 353, `getLandmarkSlots` 362, `getAllReserveSlots` 371, `getIgnoreDefenders` 407, `getIncreaseReserveCost` 431, `getReduceReserveCost` 440, `isExhaustedCharactersMorning` 449 |
| setup / UI / stats | `setupNewGame` 29, `getUiData` 120, `initializeDecks` 309, `setStats` 911 |

`Player` (`Models/Player.php`, attributes `:28-38`: `id`, `no`, `score`, `scoreAux` int; `name`,
`color`, `eliminated`, `zombie`, `faction` strings):

| Theme | Functions |
|---|---|
| deck / hand | `getDeckCount` 57 (deck + reveal), `getDiscardCount` 62, `hasDeckCards` 67, `draw` 100 (takes a revealed card first; picked cards come first in the result; never reshuffles), `getHand` 195, `getDeck` 389, `initializeDecks` 1134 |
| mana | `payMana` 176 (taps the first `n` untapped mana cards in cache order), `getManaCards` 344, `getMana` 351 (untapped), `getTotalMana` 356 |
| board | `getHero` 72, `getHeroCollection` 78, `getPlayedCards` 207, `getInfinityCards` 212, `countCardsInLocation` 219, `hasPlayedCard` 226, `getReserveCards` 231, `getPermanents` 292, `getLandmarks` 297, `getFeats` 302, `getCompletedFeat` 310 |
| slots | `getAllReserveSlots` 236, `getReserveSlots` 247, `getLandmarkSlots` 265, `getExhaustedReserveSlots` 474 |
| storm | `getStormToken` 361, `getCompanionToken` 370, `getHeroToken` 375, `isAscended` 380, `getRegionDifference` 492, `checkVictory` 502, `getBiomeInStorms` 512, `isInBiome` 555, `getBiomes` 560, `advanceStorm` 578, `getBiomeStrength` 771, `isInContact` 1296 |
| passive queries | `getAddRoll` 396, `getAddDice` 405, `getReserveAdd` 414, `getResupply2` 423, `getReduceCostType` 433, `getMinimumReserveCost` 483, `has*` 822-1022, `canPlayTappedCards` 1022, `getOpponentAdditionalCost` 1070, `getOpponentMinimumCost` 1101, `countUniversal*` 1158-1270, `hasOverrideInContact` 1331, `hasOverrideBehind` 1341 |
| engine / prefs | `canTakeAction` 95 (= `Actions::isDoable`), `getPref` 84, `getStat` 89 |
| night | `nightCleanup` 626 (iterates `getPlayedCards()->sortBy('type')`, see §8) |

---

## 6. Conditions and SpecialEffect

### 6.1 Conditions (`Helpers/Conditions.php`)

An abstract class of ~217 static predicates `fn($card, $event, ...$stringArgs)` returning bool (a few
return counts and are called directly, e.g. `countSourceAscended`). Dispatch: `Conditions::check($power, $card, $event)`
(`:15-49`):

1. conditions = `power.conditions` (array) merged with `power.condition` (string or array);
2. `power.cardId` and `power.wasGigantic` are copied into the event;
3. each condition string `"name:a:b"` is split on `:` and called as `self::name($card, $event, 'a', 'b')`;
   the first `=== false` returns false (a `null` or `0` result does not fail the check).

Callers: `Card::checkReaction` (reaction time), `Card::isListeningTo` (`listeningConditions`, collection
time, same calling convention), `Card::canBePlayed` (`playCondition`, `Models/Card.php:348`, `:355`, event null),
`CheckCondition::checkCondition` (ctx args as power, event `['pId' => ...]` or the node event,
`Actions/CheckCondition.php:143-168`), `Gain::isCantGainBoostRuleActive` (`Actions/Gain.php:326`),
`Resupply` (`:262`), `Utils::checkAttributeCondition` (`Helpers/Utils.php:339-353`), plus direct calls of
specific predicates from SpecialEffect, Target, TurnTrait (`disarmSmokeThemOut`). Some predicates write
state (`armSmokeThemOut`, `consumeSmokeThemOutCharge`, `disarmSmokeThemOut`). Arguments are always
strings; comparisons inside are mostly loose.

### 6.2 SpecialEffect (`Actions/SpecialEffect.php`)

- Args: `effect` (string key), `args` (array) (`:332`).
- `getDescription` (`:24-272`): switch on `effect` (UI text).
- `isAutomatic` true (`:275-278`); `isIndependent` is always false (every case falls through to
  `default: return false`, `:280-314`).
- `getCard` (`:316-330`): `args.cardId` (`ME` → source).
- `stSpecialEffect` (`:334-2547`): `$card = getSource()`, then one `switch ($effect)` with ~270 cases
  (`:347`); each case mutates state directly and/or inserts flows with `insertAsChild`,
  `pushParallelChild`, `pushAfterFinishingChilds`, `insertAtRoot`, sets Globals, or fires
  `checkAfterListeners`. Unknown effects do nothing (`default: break`). The method always ends with
  `$this->resolveAction([])` (`:2545`). Each case is a separate rule and must be ported one by one.

---

## 7. Randomness and determinism

All randomness in engine code (card data excluded):

| Where | Call | When |
|---|---|---|
| `Core/Globals.php:232` | `shuffle($stormKeys)` (keys 1,2,3) | game setup: hidden tumult cards order |
| `Core/Globals.php:235` | `bga_rand(0, 1)` ×3 (after the shuffle) | game setup: tumult card rotation |
| `Helpers/CachedPieces.php:430` | `shuffle($ids)` | `Cards::shuffle` at deck setup (`States/SetupTrait.php:543`) and `reformDeckFromDiscard` (SHUFFLE action) |
| `Actions/RollDie.php:183` | `bga_rand(1, 6)` per die | dice; in environment `studio` the value is replaced by **5** (`:184-186`) after calling `bga_rand` |
| `Actions/SpecialEffect.php:2142`, `:2175` | `Collection::rand()` = `array_rand` | random card of the opponent's hand |
| `Managers/Cards.php:344,355,371,403,524,621-631` | `array_rand`, `shuffle` | random deck and random Unique generation (setup only) |
| `States/SetupTrait.php:176` | `array_rand(FACTIONS)` | random faction choice (setup only) |
| `Helpers/Utils.php:44-56` | `array_rand` + `shuffle` | `Utils::rand`, no caller |
| `Actions/Gain.php:235` | `md5(uniqid('', true))` | gain group id (only compared for equality) |

Harness hooks (`RNG:9-46`): `bga_rand` goes through `Harness\Rng::randInt` (default `mt_rand`), and
`shuffle` called unqualified from namespaces `ALT\Helpers`, `ALT\Core`, `ALT\Managers` goes through
`Harness\Rng::shuffle`, which can force the resulting order. `array_rand` is not hooked. The arena server
feeds dice from a per-game list and switches the environment to `prod` when a list is configured, or
uses `studio` (all dice = 5) otherwise (`Arena:155-174`).

`shuffle`, `array_rand` and `mt_rand` all draw from the same Mersenne Twister. The arena seeds it at the
start of every request that changes the game (`Arena::seedRng`, called by `request(..., act: true)` for
game creation, player and bot actions): `mt_srand(($gameId * 1000003 + $acts) & 0xFFFFFFFF)`, where
`arena_rng` keeps `game` and `acts` (incremented in the request's transaction, so rolled back with it).
Views and bot decisions are not seeded; no view draws a value (random calls are only in setup, deck
selection and actions).

For the port: deck order after a shuffle is "state = index in the shuffled array", top = last element
(highest state). `rules/src/php_rand.rs` reproduces PHP 8.2's generator, `shuffle` and `array_rand`
exactly, so the Rust engine seeded the same way draws the same values.

---

## 8. Translation notes

### 8.1 PHP semantics to preserve

**Ordered maps.** PHP arrays keep insertion order. Collections are keyed by id and iterate in SQL
result order at load time (no `ORDER BY` in `CachedPieces::fetchIfNeeded`; in practice primary-key
order), then creation order. Reaction order (§4.3), `payMana` (which mana cards get tapped), night
cleanup order, and `getTopOf` ties all depend on it.
- `Collection::merge` uses `+` (`Helpers/Collection.php:50-53`): keys of the right side that already exist
  are dropped. `Discard::actDiscard` builds `$validIds = cards + reserveCards + landmarkCards` with `+` on
  lists (`Actions/Discard.php:289`), so only the first list's indexes count.
- `array_merge` renumbers integer keys; `array_filter`/`array_diff` keep keys; `array_values` renumbers.
- JSON round trip of globals/engine: a PHP list stays a list; an array with non-sequential int keys is
  stored as an object and decoded back with int keys.

**Comparisons and coercion.**
- DB values are strings unless the model casts them: `Card` casts id/state/pId to int; `Meeple` only
  id (its `state`, `type`, `pId` are strings); `Player` casts id/no/score/scoreAux; preferences are
  strings or null; Globals int variables are cast on load.
- `==`, `!=`, `in_array`, `array_search`, `switch` are loose. PHP 8 rules: `"5" == 5` true, `"abc" == 0`
  false, `null == 0`, `null == ''`, `null == false`, `[] == false` true; an array is never `==` to an int
  (§2.10 PASS). Missing preference values (`null`) compare equal to `0` (`OPTION_CONFIRM_DISABLED`).
- `Collection::where` (`:106-124`): null value = no filter; array value = `in_array`; array field =
  `in_array(value, field)`; `%` in value = case-insensitive LIKE; else `==`.
- `orderBy` uses `$a - $b` with `uasort` (stable since PHP 8). `sortBy` (`:185-191`) returns a bool from
  the comparator; PHP 8 accepts it with a deprecation and handles `false` by calling the comparator with
  swapped arguments, so equal keys compare as "greater" and the result depends on PHP's `zend_sort`
  (insertion sort under 16 elements). Used by `Player::nightCleanup` (`Models/Player.php:649`) and the
  tie-break markers (`States/TurnTrait.php:266`). Verify the exact order against PHP before porting.

**Operators.**
- `??` only tests null/undefined; `?:` tests falsiness. `??` has lower precedence than comparison and
  `&&`: `$x ?? false == true` (`ParallelNode.php:115`) is `$x ?? (false == true)`;
  `$locExploded[1] ?? '' == 'scout'` (`Actions/ChooseAssignment.php:260`) is true whenever the location
  has any `_suffix`; `infos['optional'] ?? (parent != null && parent->areChildrenOptional())` (`AbstractNode.php:404`).
- `(A && B) || C` in `optionalAction` (`AbstractNode.php:472`).
- `array_push` returns the new count (`game:259`, `:271`, §1.7).
- Spread `...$args` with string keys = named arguments; extra positional arguments to user functions are
  ignored (§3.1).
- Method names are case-insensitive (`'stAfterNewday'`).

**Null and missing values.**
- `$x[] = v` on an undefined variable creates an array (`States/NewDayTrait.php:219`). Reading an
  undefined index yields null with a warning. Calling a method on null is a fatal `Error`; `count(int)`
  and `count(null)` throw `TypeError`.
- `Cards::get($id, false)` returns an empty `Collection` when missing (truthy object), not null;
  `getSingle` returns null. `Collection::first()` returns null on empty.
- Several `getCard()` implementations throw `VisibleSystemException` when `cardId` is missing.

**References and identity.**
- Action ctx and Engine functions use references (§2.9). Objects in caches are shared: a card fetched by
  two filters is the same object, and a setter updates every holder.
- `foreach (... as &$x)` blocks leave references alive (`States/TurnTrait.php:106-118` unsets them).

**Static state per request.** `Engine::$tree`, `Globals::$data/$initialized`, `Cards::$datas`,
`Meeples::$datas`, `Players::$datas`, `Notifications::$cachedValues`, `DB_Model::$parsedMethods`,
`Evaluation::$results`, framework globals cache. All are rebuilt at each request (`Arena:191-208`);
`Log::revertTo` and `Engine::restart` clear them mid-request.

**Synchronous re-entrancy.** State actions run inside `jumpToState` (§1.3), act methods proceed inside
`takeAction` and then proceed again (§2.8). The port must keep the same call order, including code that
runs after a nested chain returns (e.g. `takeAction`'s final `resolveAction`, `jumpToState` computing
args only if the state did not change).

**Known defects to reproduce, not fix** (each changes observable behaviour):
`array_push` in phase listeners (§1.7); `forceListening` dead branch (§4.3); payment tag discarded
(§4.6); double proceed (§2.8); `isMana` assignment (§5.4); `MoveCard` inserting action `boost`
(`Actions/MoveCard.php:99`, also uses `$player` undefined unless `cards == ALL`, `:100`); `location == DISCARD`
compares with the action name `'Discard'` (`Actions/ChooseAssignment.php:501`); double leave listeners in
`NightCleanup` (§4.4); power lookup without the support alias (§4.6); `isListeningTo`/`getReactions`
disagree on tapped reserve cards (§4.2 vs §4.6); `ActivateEffect` pushes `$node` (the last raw effect) when
there is one node (`Actions/ActivateEffect.php:254-257`); `getUndoableMandatoryNode` is not recursive on
OR/XOR nodes.

### 8.2 BGA-only (not rules)

- Notification text and arguments (`Core/Notifications.php`); `updateIfNeeded` (`:52-67`) recomputes
  `getBiomeTotals`, `computeStorm(false)`, `getBlockedExpeditions`, `getPowersBlockedExpeditions`,
  `getDefenders`, slots after each `notifyAll` and sends differences. These calls are read-only
  (`computeStorm` only writes when `$advance` is true), but they may create missing globals.
- State args (`args*` methods, `argsAtomicAction`, `argsResolveChoice`, `descSuffix`, `_private`), the
  `description` of choices (except the empty/non-empty distinction of §2.6), `getGameProgression`,
  `localGetAllDatas`/`getUiData`, preferences UI.
- Stats (`Core/Stats.php`, `ChooseAssignment::statPlay`), except `Stats::getWinner` read by the end report.
- Undo notification management (`Log::clearUndoableStepNotifications`, `gamelog` cancel), `refreshUI`.
- Zombie/concede handling (game ends immediately), `stPreEndOfGame` API report,
  `giveExtraTime`, deck API loading, `DebugTrait`, studio-only messages.
- Rules-relevant despite looking like UI: `Notifications::resetCache()` only refreshes the notification
  cache; the `engineChoices`/`Log` bookkeeping behind undo affects auto-choice and auto-confirm
  decisions (§2.4, §2.12) and must be ported.
