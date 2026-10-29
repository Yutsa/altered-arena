# Status

## What works

- **Full games**, desktop and mobile web, installable as a PWA: lobby, play against the bot,
  matchmaking between two players, two-player table with one link per seat, spectators.
- **Rules engine** (`rules/`): a faithful translation of the AlteredBGA PHP engine (reference:
  AlteredBGA `origin/master`). It asks the same questions, with the same options, and reaches the
  same states as the BGA engine. Checked by golden tests recorded with the PHP engine:

  | Golden set | Files | Steps compared |
  | --- | --- | --- |
  | Every card (1976) played on three boards, every branch of its decisions | 5928 | 81,065 |
  | Whole games played at random, setup to victory | 400 | 43,344 |
  | Real games recorded on Board Game Arena (anonymised) | 8 | 1,156 |
  | **Total** | **6336/6336** | **125,565/125,565** |

  Game creation is checked too: for the 400 games, the tables Rust creates are identical to PHP's.
  PHP's randomness (mt_rand, shuffle, array_rand) is reproduced bit for bit, seeded per action.
- **Server** (`arena-server/`) on the translated engine: game creation, the GameView the client
  shows, the bot (translated from the oracle's PHP bot), persistence, long polling, tokens.
- **Performance**: see `docs/PERFORMANCE.md`. 1.86 ms of CPU per answer under the harsh load test;
  an OVH VPS-1 (2 vCores, 4 GB) holds about 1500 to 6000 simultaneous games at a realistic pace,
  depending on the vCore speed (with `PERSIST_SECS=60`).

## Known differences and limits

- The PHP engine's quirks are reproduced on purpose (e.g. the `array_push` bug in the phase
  listeners, MoveCard's undefined action): the reference is what BGA does.
- The GameView follows the PHP adapter (`server/src/V1.php`): play options have no cost of their
  own (the client shows the card's cost), card stats are printed stats plus boosts, and the log is
  built from the PHP notification templates, shorter than BGA's.
- Decks: starter decks only. Custom decks (deck codes), random and API decks, and the DEMO format
  raise "not translated".
- The bot is the oracle's simple bot (first legal choices). When both of its answers are refused,
  it tries the other legal answers instead of stopping the game (PHP would wait forever).
- Speed: the translated engine is about 4 times slower in CPU and 5 times larger in memory than the
  first, from-scratch Rust engine (removed since: it was fast but did not match the BGA rules).
  Closing that gap needs the whole engine on typed data (see the plateau analysis in
  `docs/PERFORMANCE.md`). The cheapest next win is persistence: a saved game is ~350 KB because
  every card's class data is written in full.
- No accounts: seats are protected by per-seat tokens only.

## Next steps (suggested)

1. Compact persistence (save only each card's own values), then measure the VPS-1 capacity again.
2. Custom decks (deck codes) and Unique cards (`Helpers/FlowConvertor.php` builds their effects).
3. WebSocket push, the engine in WebAssembly for offline play, native shells (Capacitor, Tauri).
4. Accounts, ranked matchmaking, tournaments.
