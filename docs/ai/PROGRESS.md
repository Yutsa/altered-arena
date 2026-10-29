# AI tournament: progress

Plan: `docs/plans/AI_TOURNAMENT.md`. Private data (replays, decklists, player names): `private/`
(git-ignored).

## Current phase

A (collection) finishing, B (custom decks) in progress.

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

## Decisions

- Forfeited matches (no game played) are not pairings to replay: 74 pairings.
- Deck ids `D01..` sorted by faction, hero, player id; the mapping to players is only in
  `private/tournament-617607/decks.json`.

## Next steps

1. Finish the replay download, re-run `extract_decks.py`, record deck goldens (3+ seeds per deck).
2. Full golden run (6336 + new), commit.
3. Phase C: `ai/` crate (match runner, legal answers, tournament pass 0 with the simple bot).
