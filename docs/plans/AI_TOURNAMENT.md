# Overnight run: AI tournament on BGA tournament 617607

This file is the prompt of a long autonomous session. Start the session with:

> Read `docs/plans/AI_TOURNAMENT.md` and carry it out, from the phase recorded in
> `docs/ai/PROGRESS.md` (phase A if the file does not exist). Work autonomously until the definition of
> done or the end of the time budget. Do not ask me questions: decide, write the decision down in
> `docs/ai/PROGRESS.md`, and continue.

---

## Goal

Replay Board Game Arena tournament https://boardgamearena.com/tournament?id=617607 (about 40
players, Swiss groups then direct elimination) between AIs, one AI per deck played in the tournament.
The AIs are written from the tournament replays (how each player played their deck) and improved
between tournament passes.

## Definition of done

1. Every deck of the tournament is known (full list, Uniques included) and plays in the Rust engine.
2. One AI per deck. An AI decides only from what its player sees, plus the opponent's full decklist
   (open decklists).
3. A tournament pass replays **every pairing that took place in the tournament** (all groups and the
   elimination phase) **100 times** (50 games with each player seated first), and writes a
   per-pass report: the matchup table of those pairings, each deck's win rate, the standings the pass
   gives against the real ones, and the AI's agreement rate with the real players' decisions.
4. **5 passes** in a row. Between two passes, the AIs are improved from what the previous pass showed
   (see phase F), and the change is measured.
5. Final report `docs/ai/REPORT.md`: method, the 5 passes side by side, what changed between passes
   and what it brought, the limits, the next steps.

## Ground rules

- **Fidelity comes first.** The PHP engine (`../AlteredBGA-ref`) is the source of truth and is never
  modified. Any change to `rules/` keeps the golden tests at 6336/6336 and `cargo test -p altered-rules`
  green (`tools/rules_iter.sh --quick` while iterating, the full set before each commit). New engine
  features get their own goldens recorded with the PHP oracle (`server/tools/explore.php`), as for
  the rest of the translation (`docs/RUST_PORT.md`).
- **No cheating.** An AI reads the game through `Game::view(Some(player))` (the GameView a human
  player gets) and the opponent's decklist. It may simulate on a copy of the game only after
  *determinisation*: the cards it cannot see (opponent hand, face-down mana, both decks' order) are
  redrawn at random from what is still unknown, consistent with the open decklist. A test checks
  this: two games that differ only in hidden cards give the AI the same inputs.
- **Private data.** Replays and decklists hold real player names and BGA ids. Keep them in
  `private/` (add it to `.gitignore` first) and never commit them. In committed reports, name decks
  by hero and faction plus a short number (e.g. `D07 Sierra AX`); the mapping to players stays in
  `private/`.
- **BGA etiquette.** Only the tournament's tables, one request at a time, at least 4 s apart, from
  the user's logged-in Chrome tab. Stop on any 4xx/5xx or captcha and continue with what is already
  saved.
- **Git.** Commit locally after each milestone, author `Yutsa <edouard.willisseck@gmail.com>`
  (already the repo's local config), with the usual Co-Authored-By line. Do not push.
- **State that survives context compaction.** `docs/ai/PROGRESS.md` always holds: current phase,
  what is done (with numbers), decisions taken and why, the next 3 steps. Update it at every milestone.
- **Keep the Mac awake** (`mcp__ccd_host__request_keep_awake`) at the start. Long commands run in
  the background; never block on a command over ~10 minutes without a log file to check.
- **Engine threads need big stacks**: the engine recurses deeply (`arena-server` runs it on 256 MB
  stacks). Simulation threads must do the same, and a game that panics or errors is recorded as an
  error (deck, seed, decision) and skipped, never allowed to stop a pass.

## Time budget and fallbacks

Machine: 12 cores (8 performance), 24 GB. Measured on the current engine
(`goldens/examples/bench_search.rs`): 0.84 ms per decision, ~120 decisions and ~100 ms per game with
the simple bot, 11 µs to copy a game state.

A pass is ~100 pairings x 100 games = ~10,000 games. Budget: **a pass under 45 minutes**, so at most
~2.5 s of CPU per game on 10 threads, i.e. ~3,000 engine decisions per game including the AI's
simulations (about 20 simulated decisions per real decision on average). Spend them on the decisions
that matter (which card to play, where, what to keep in reserve), not on forced or trivial ones.

If time runs short, in this order:
1. Never lower the 100 games per pairing of a pass. Lower the optimisation budget of phase F first,
   then the AI's simulation budget (uniformly, for every deck).
2. A deck that still cannot be played after phase C (unknown card, Unique the engine cannot load):
   replace the missing card by its closest regular version (same name, Common/Rare), say so in the
   report, and go on. Exclude the deck only if that is impossible; its pairings then count as missing.
3. Fewer than 5 passes is acceptable only with the reason written in the report.

## Phases

### A. Collect the tournament (browser, ~30 min)

1. With the Chrome tools, open the tournament page. It has two groups (Swiss) and an elimination
   phase; each match has an icon next to "Terminé" leading to the table, whose page links the replay.
   Collect every table id with: phase, group, round, the two players, the winner.
   Prefer reading the DOM or the JSON the page itself loads over clicking through every match.
   Check whether a match is one game or several (several tables).
2. Fetch each replay as `misc/harness/tools/collect.js` does in `../AlteredBGA` (logged-in tab,
   `X-Request-Token`, `/archive/archive/logs.html?table=ID&translated=true`, the bridge
   `tools/replay_receiver.py` to write files, since BGA's CSP forbids calling localhost). Adapt it
   to take a list of table ids. Save to `private/tournament-617607/replays/TABLE.json`, plus
   `private/tournament-617607/tournament.json` (structure, pairings, real results).
3. Extract the decklists. Archive logs hold the private notifications of both players (the existing
   replays show both players' `pDrawCards`). After a player confirms a custom deck,
   `updateInitialPrecoDeckSelection` carries `_private[PID].API`: the full deck content
   (`SetupTrait::argsPrecoDeckSelection`), Uniques included with their generated `properties`.
   Check this on the first replay before fetching the rest. If it is not there, rebuild each list
   from every card the player drew or revealed across all their tournament games (union, max count
   per game), and mark the deck "partial" with the number of unseen slots.
   Write `private/tournament-617607/decks.json` (deck id, player, hero, cards with counts, Unique
   properties) and the public `docs/ai/decks.md` (anonymised).
4. Check every card against the engine's catalogue (`rules/data/cards.min.json`). List what is
   missing (new cards, Uniques).

### B. Custom decks and Uniques in the engine (the largest risk)

The engine only builds starter decks today. Custom decks go through `actConfirmAPIDeck` then
`stDeckSetup` (`SetupTrait.php`, the `API` branch) and `Cards::createDeck`. Unique properties are
generated once by `Cards::generateUnique` / `FlowConvertor::constructEffect`; after that a Unique is
data like any card (its properties are stored with the card row; `rules/src/managers/cards.rs`
already loads Unique rows from their DB JSON). So:

1. PHP oracle (`server/src/Arena.php`, `V1.php`, `explore.php`; ours, not the BGA engine): let
   `createGame` take a deck content per player (the `API` content as extracted in A.3) and select it
   the way `actConfirmAPIDeck` does. Add an `explore.php` mode that plays random games with given
   decks, recorded like `game SEED`.
2. Rust: translate the same path (`Game::create` with deck contents), faithfully, with the PHP line
   references as elsewhere.
3. Goldens: whole random games with the tournament decks (at least 3 seeds per deck, more for decks
   with Uniques), and the per-card goldens of any card the catalogue lacked. All must pass, plus the
   existing 6336.
4. If the replays only give a Unique's `content` and not its `properties`, generate the properties
   with the PHP oracle (a small `server/tools/unique_props.php` calling `Cards::generateUnique`) and
   store them in `decks.json`. Do not translate `FlowConvertor` (6900 lines) unless there is no other way.
5. Bonus, if it comes cheap: the tournament replays become goldens too (`tools/goldens_replays.sh`
   on them), which proves the decks replay exactly as on BGA.

### C. Headless simulation

A new crate `ai/` (workspace member, depends on `altered-rules`):
- `match`: deck A vs deck B, N games, seeds, seats alternated, threads with big stacks, results with
  Wilson 95 % intervals, per-game errors recorded. Deterministic for a given seed and AI version.
- Legal answers: from `Game::decision(player)`, enumerate the candidate answers of every decision
  kind (select with min/max, assignment, cleanup, exchange, choices, mana, mulligan...). Check the
  enumeration against the engine: every enumerated answer is accepted on a copy, and over many games
  the answers the simple bot gives are always in the list. Large combinatorial kinds are generated
  lazily or pruned by heuristics.
- `tournament`: reads `tournament.json` + `decks.json`, plays every pairing 100 times, writes
  `docs/ai/passes/PASS_N.md` (anonymised) and `private/.../PASS_N.json` (raw).
- First check of the whole chain: one pass with the existing simple bot for every deck (the
  baseline, pass 0; it does not count among the 5).

### D. Learn from the replays

Turn each tournament replay into a sequence of (position seen by the player, decision, answer the
player gave): replay it in the Rust engine (phase B makes that possible) and record, at each decision
of each player, the view and the real answer. From that:
- **Agreement rate**: share of real decisions where the AI picks the same answer as the player
  (overall, and per decision kind). Measured on every pass.
- **Per-deck tendencies**: which cards the player plays early or holds, which go to mana, where
  characters go (hero or companion expedition), what is kept in reserve at night, mulligans. These
  become per-deck priors and the starting values of the AI's parameters.

There are few games per deck (4 to 8): use them to set priorities and check agreement, not to train
a model.

### E. The AI

One engine shared by every deck, with per-deck parameters (a JSON file per deck, versioned):
1. **Evaluation of a position from a player's point of view**: region progress (expedition
   positions, who is ahead in each region, tumult/storm state), board strength per region by
   statistic, cards in hand and reserve, mana available and to come, tempo (who plays next, what is
   still exhausted), and the end-of-game race. Weights per deck.
2. **Beliefs about hidden cards**: the opponent's unknown pool = their decklist minus every card seen
   (board, reserve, discard, revealed). Each card's probability of being in their hand follows from
   the hand size and the pool (hypergeometric). Used to weigh threats (e.g. "2 copies left of a
   removal spell, 6 cards in hand, P(at least one) = ...") and to draw determinisations.
3. **Decision**: for each candidate answer, apply it on a copy and evaluate. On important decisions,
   look further: K determinisations of the hidden cards, play the rest of the current day (both
   players use the fast 1-ply policy), evaluate, average. K and the depth are budget parameters.
   Look one day further only if the budget allows it.
4. **Priors from phase D** break ties and order the candidates (cheaper simulations).
5. Unit tests: no hidden information read (see ground rules), legal answers only, deterministic for
   a seed.

Before pass 1, check the AI beats the simple bot clearly (e.g. > 80 % over every deck in mirror
setups) and that its agreement rate is above the simple bot's.

### F. Passes and improvement (5 times)

For pass n = 1..5:
1. Run the tournament pass (all pairings x 100). Report it.
2. Analyse: matchups far from the real results, decks whose AI loses to the simple bot or to weaker
   decks, recurring blunders (look at a few lost games' logs: wasted cards, empty expeditions, mana
   misuse, illegal-answer fallbacks), agreement rate per decision kind. Write the findings in the
   pass report.
3. Improve, two ways, each measured before it is kept:
   - **Shared AI code** (evaluation terms, candidate pruning, lookahead): A/B test the new version
     against the previous one on a fixed sample of pairings (both seats, fixed seeds); keep it only
     if it is not worse with 95 % confidence and better on average.
   - **Per-deck parameters**: a short evolution strategy per deck (perturb the weights, play the
     deck against its tournament opponents and a sample of the field, keep the best), with seeds
     held out from the final measure to limit overfitting. All decks are improved, in parallel,
     within the phase budget (~60 min per pass).
4. Commit, update `PROGRESS.md`, next pass.

### G. Final report

`docs/ai/REPORT.md` (anonymised): the decks, the method, a table of the 5 passes (per-deck win rate,
agreement rate, games played, errors), the final matchup table of the tournament pairings, the
simulated standings next to the real ones, what each improvement brought, the known limits (partial
decks, substituted cards, AI weaknesses seen), and the next steps (full round-robin matchup table,
optimising one given decklist against the field, a faster typed engine if the AI needs more
simulations).
