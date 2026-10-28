# Status (first version)

## What works

- **Full games**, desktop and mobile web, installable as a PWA: lobby, play against the bot,
  matchmaking between two players, two-player table with one link per seat, spectators.
- **Rules engine in Rust** covering the whole day structure (setup and mana, Morning draw and
  Expand, Noon, alternating Afternoon turns with quick actions, Dusk movement per region and
  biome, Night rest and Reserve/Landmark cleanup, victory, tiebreaker in the Arena), costs and
  reductions, Fleeting / Anchored / Asleep / boosts / counters, Scout, Tough, Gigantic, Seasoned,
  Eternal, Defender, Cooldown, support abilities (discard from Reserve), exhaust abilities,
  triggered abilities with the original ordering rules, undo within a turn.
- **Cards**: the 30 starter decks are playable; the data of all 1976 cards is loaded. Every trigger
  condition used by a card and every named special effect of the original engine are implemented,
  except `MarkRegion`, `BoostExchange`, and `Gain` with a computed amount (met in ~7 % of
  self-play games; logged as "not implemented" when met).
- Bot vs bot: 1000 games on every pair of starter decks all end (avg 6.2 days), 0.3 ms per game.
- Server: see `PERFORMANCE.md` (1000 simultaneous games ≈ 0.3 core, 140 MB).

## Not there yet

- **Custom decks and Uniques** (the original builds Unique effects from "trinity" codes at runtime,
  `FlowConvertor`): only starter decks can be chosen.
- **Exact parity with the BGA engine.** Measured with `tools/diff_engines.py` (both engines from
  the same position, same random decisions, full state compared after each decision), on 20 games
  (~70 decisions each):

  | | |
  | --- | --- |
  | Games identical from start to end | 1 / 20 |
  | Decisions before the first difference | median 12, mean 21 |

  The first difference of a game is rarely a broken rule in general. It is usually one card's
  effect, or the order of simultaneous triggered abilities. Recent examples:
  - when a Muna hero's "immediate" ability resolves;
  - a Reserve ∞ ability that should trigger and doesn't;
  - the original engine asks to confirm an effect that does nothing, where Rust skips it.

  Each run of the tool lists the first difference of every game with its context. Fixing those one
  by one is the way forward; this session started at 0/8 games past the first 3 decisions.
- Region markers (Nadir & Bubbles), and the original engine's Fen & Crowbar edge cases when the deck
  is almost empty.
- The bot is simple (greedy on statistics, random elsewhere).
- No accounts: seats are protected by per-seat tokens only.

## Next steps (suggested)

1. Keep running `tools/diff_engines.py` and fixing the first divergence until most games are
   identical. Then add the 8 recorded BGA replays (`AlteredBGA/misc/harness/replays`) as a
   regression suite: replay their decisions in the Rust engine.
2. Custom decks (deck codes from altered.gg / the Altered API) and Uniques.
3. WebSocket push (fewer requests, smoother animations), the engine in WebAssembly for offline play
   against the bot, native shells (Capacitor for iOS/Android, Tauri for desktop) around the PWA.
4. Accounts, ranked matchmaking, tournaments.
