# AI tournament: progress

Plan: `docs/plans/AI_TOURNAMENT.md`. Private data (replays, decklists, player names): `private/`
(git-ignored).

## Current phase

Phase F after pass 3 (A/B variants, evolution round 3, measure against `private/params_p3`).

## Done

- A.1 Tournament structure from BGA's own JSON (`/tournament/view/getOverview.html?id=617607`):
  stage 1 = Swiss, 2 groups, 4 rounds; stage 2 = single elimination, 8 players, 3 rounds. 38
  players, 82 matches: 74 played (one game, one table each), 6 byes, 2 forfeits
  (`error_tournament_wrongnumber`: the opponent never showed up, no game). Saved in
  `private/tournament-617607/tournament.json` by `tools/ai/build_tournament.py`.
- A.2 Replays fetched one table at a time (4.5 s between requests) through the bridge
  `tools/ai/replay_receiver.py` (adds a `file` message for other JSON).
- A.3 Each replay holds both players' confirmed custom deck (`_private.API` of
  `updateInitialPrecoDeckSelection`), Uniques included with their generated properties: no deck is
  partial. `tools/ai/extract_decks.py` writes `private/.../decks.json`, `table_decks.json`, the
  anonymised `server/goldens/tournament_decks.json` (input of the PHP oracle) and `docs/ai/decks.md`.
- A.4 Every non-Unique card of the decks has a class in `rules/data/cards.min.json`.
- B.1 PHP oracle: `V1::createGame` takes `apiDeck` per player (confirmed with `actConfirmAPIDeck`);
  `explore.php deckgame SEED DECKS_FILE A B` records a whole random game between two custom decks
  in `server/goldens/decks/A-B-SEED.json` (same format as `game`, plus `apiDecks`).
- B.2 Rust: `actConfirmAPIDeck`, `stDeckSetup`'s API branch, `Cards::createDeck`, `NewPlayer.api_deck`.
  Also `Globals::cast`'s quirk (it doubles every backslash of the JSON it reads, so card texts
  with `\"` gain a backslash each time the deck content global is read back): `GlobalSlot` keeps
  the row value when the cached one differs. With it, `--create` tables match PHP's exactly.

- B.3 Goldens of the tournament decks: `tools/goldens_decks.sh` with every real pairing twice
  (seats swapped) = 148 whole random games, 16,048 steps, all pass, also with `--create` (Rust
  creates the game itself: tables identical to PHP's).
- B.5 Tournament replays as goldens: `tools/goldens_tournament.sh` (RUNNER=private/harness_replay,
  a copy of the harness's ReplayRunner whose notification comparison ignores backslashes, which
  `Globals::cast` multiplies in card texts as many times as BGA's request history read them).
  About half the 74 replays replay exactly; the others diverge in the oracle itself (BGA version
  drift: `simpleNote` notifications, state sequences 93/97, a `count(null)`), not in Rust.
  Output `server/goldens/tournament/replays`, linked as `server/goldens/tournament_replays`.
- C. Crate `ai/` (`aitool`): `match`, `mirror`, `pass N AGENT`, `agree N AGENT`; legal answers
  (`legal.rs`), determinisation (`sim.rs`), runner on 256 MB-stack threads with panics recorded
  (`play.rs`), Wilson intervals. Tests: hidden cards do not change the AI's inputs, candidates are
  legal and contain the simple bot's answer, games are deterministic.
- Pass 0 (simple bot, both seats): 7400 games, 0 errors, 3.5 min; the pass's favourite won the real
  game in 36/74 pairings, Spearman 0.49 with the real standings (`docs/ai/passes/PASS_0.md`).
- E. AI v1 (`search.rs`, `eval.rs`): candidates applied on a redealt copy, the action finished by
  Bot.php, GameView evaluated (progress, Dusk outlook per expedition and biome, hand, Reserve,
  concave Mana, landmarks); turn decisions: the 3 best + end of turn rolled out to Night by Bot.php;
  Mana choices by priors (else the most expensive cards) until 10 orbs. Mirror against the simple
  bot (12 decks x 10 games): 82.5 % [74.7, 88.3], 1.55 s CPU per AI per game.

- Pass 1 (AI v1 with priors): 7400 games, 1 error (`too many steps`), favourite = real winner in
  43/74 pairings (pass 0: 36), Spearman 0.56 (0.49), agreement 42.2 % (simple bot 35.5 %).
  Wall time is meaningless (the Mac slept, then background priority). Worst fits: the two Matz OD
  decks (permanent-heavy) lose 87-98 % of their games here but won theirs in the tournament.

- Phase F after pass 1 (A/B, 8 games per pairing, ~592 games each, against the pass-1 parameters):
  - Reserve keep-value at cleanup (`cleanup_keep` 0.5): 52.5 % [48.4, 56.5], kept.
  - End-of-turn bias -1: 50.7 % [46.7, 54.7], kept (passes the rule; evolution tunes it per deck).
  - 2 determinisations per decision: 52.6 % [48.6, 56.6] but +47 % CPU per game: rejected for the
    time budget.
  - 5 rollouts instead of 3: the A/B aborted twice on a stack overflow (job 433, pairing 54,
    seed 754000) in the engine; not measured. Since then a pass logs each game in
    `PASS_N.journal`, resumes after an abort, and records a game that aborted twice as an error.
  - Per-deck evolution (2 candidates x 30 held-out games per deck, keep at +10 points): 9 of 37
    decks changed. The whole set (both kept variants + evolution) against the pass-1 parameters:
    **57.6 % [53.6, 61.5]** of 592 games: kept (`ai/params`).
- Stack overflow of seed 754000: `isGigantic` of a token recursing without end
  (isGigantic -> countUniversalTokenGigantic -> checkAttributeCondition -> isGigantic), the same
  in PHP (a fatal error there). The Rust engine now stops at depth 500 with an error (ee5777b,
  goldens 6526/6526).
- Pass 2: 7400 games in 77 min on 4 threads, 1 error, favourite = real winner 41/74, Spearman
  0.57, agreement 42.7 % (cleanup decisions 20 % -> 38 %). Matz OD still last.
- Phase F after pass 2 (A/B against `private/params_p2`, seed 710000): landmark weight x2 49.3 %
  [45.3, 53.3] rejected; board weight x2 50.3 % [46.3, 54.4] and Mana target 8 50.3 % kept by the
  rule; evolution round 2 changed 4 decks; the package (board x2, Mana target 8, evolution)
  against the pass-2 parameters: 50.0 % [46.0, 54.0] (seed 810000): **rejected** (not > 50 %).
  As games are deterministic per seed, a pass with unchanged parameters would repeat pass 2: more
  variants are tried before pass 3 (seed 720000): hand weight x0.5 50.5 % [46.5, 54.5] kept,
  Reserve weight x1.6 47.8 % rejected, 4 rollouts 51.5 % [47.5, 55.5] kept (+5 % CPU). The
  package (hand x0.5, 4 rollouts) against the pass-2 parameters: 50.3 % [46.3, 54.4] (seed
  820000): kept by the rule, but indistinguishable from no change. Pass 3 uses it.
- Pass 3: 7400 games in 91 min, 12 errors, favourite = real winner 42/74, Spearman 0.55, agreement
  42.1 %. Cards left in hand at the end 2.4 -> 1.7. 11 errors were `too many steps`: at Night
  both players are asked in turn which pending effect resolves first, and each AI picked the
  other's, which sends the question back. The AI now takes the first answer when the same choice
  comes back a third time among its last 8 decisions (the three traced seeds end normally). The
  last error (Mana Flare, a method missing in PHP too) is the engine's, as in pass 2.
  `no answer accepted` errors now carry the engine's first refusal message.

## Decisions

- Forfeited matches (no game played) are not pairings to replay: 74 pairings.
- Deck ids `D01..` sorted by faction, hero, player id; the mapping to players is only in
  `private/tournament-617607/decks.json`.
- Pass seeds are the same at every pass (10000 + 1000 x pairing + game): passes differ only by the
  AI. Optimisation (phase F) uses other seeds (500000+).
- Every player used one deck all tournament: 37 decks (the 38th player only had a forfeit).
- 2026-10-29 21:30, at the user's request (fan noise): runs limited. The running pass 1 was put
  on background priority (`taskpolicy -b`, efficiency cores only); later runs use `--threads 4`
  under `taskpolicy -b`. A pass then takes ~1.5 h instead of 40 min: following the plan's
  fallbacks, the phase F budget (evolution games, A/B sizes) is lowered first, never the 100 games
  per pairing. The Mac slept from ~18:10 to ~21:25 (pass 1 paused, no harm).
- 2026-10-30 02:15: background priority alone makes a pass ~8.6 h (efficiency cores only, 4.2 s
  of wall time per game). Runs from now on: `nice -n 10 ... --threads 4` at normal priority
  (~40 % of the machine, ~1.5-2 h per pass). Phase F per pass: A/B of the variants on 8 games per
  pairing (592 games), evolution with 2 candidates x 30 games per deck, the new parameters measured
  against the old on 8 games per pairing. The AI's simulation budget is unchanged so passes stay
  comparable.
- Shared-code acceptance rule (plan: not worse with 95 % confidence and better on average): the new
  version's win rate over the A/B > 50 % and the lower bound of its 95 % interval > 46 % (with ~600
  games the interval is +-4 points, so "not worse" means not below 50 % by more than the noise).
  Per-deck evolution: a candidate replaces the current parameters when it wins >= 10 points more
  of the same held-out games; the whole new set is then kept only if it passes the same rule
  against the old set.
- Agreement counts decisions with at least 2 candidates, real `undo` answers excluded; an answer
  agrees when it equals the real one or leads to the same state.

## Next steps

1. Phase F3: snapshot `private/params_p3`, variants (`rollout_copies` 2) + `aitool evolve 3`,
   measure (seed 830000).
2. Pass 4, phase F4, pass 5, then `docs/ai/REPORT.md` (`tools/ai/report_tables.py`).
