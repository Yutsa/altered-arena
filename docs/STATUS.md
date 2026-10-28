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
- **Exact parity with the BGA engine** is not reached yet, but it is now measured and improving.
  `tools/diff_engines.py` starts both engines from the same position, plays the same random
  decisions in both, and compares the full state after each one. Results on 30 games of 50–140
  decisions:

  | | First version | Now |
  | --- | --- | --- |
  | Games identical from start to end | 1 / 20 | 4 to 8 / 30 (varies between samples) |
  | First difference (diverging games), median | 12 decisions | 44 decisions |
  | First difference (diverging games), mean | 21 decisions | 44 decisions |

  Games where a die is rolled stop counting at the roll: the dice are random in both engines.

  First differences in the last run (21 diverging games):

  | Category | Games |
  | --- | --- |
  | a card's effect gives a different result (zones, boosts, statuses) | 10 |
  | an answer the BGA engine accepts is refused | 3 |
  | one engine asks something the other doesn't | 2 |
  | different playable cards | 2 |
  | different Dusk movement | 2 |
  | different targets offered | 1 |
  | choice options enabled differently | 1 |

  The tool reports the first difference of each game with the BGA engine's log at that point. It
  also saves a trace (`traces/game-N.json`) that
  `cargo run --release --example replay_trace traces/game-N.json` replays step by step in the Rust
  engine with its full log.

  The tool tolerates differences that are only about how the BGA client asks:
  - confirmations ("switch of player", "card is seen");
  - single-option prompts, and choices of an effect that would do nothing;
  - the order of several simultaneous abilities that Rust resolves on its own.

  Use `--strict-order` to report ordering differences too.
- Not implemented: `BoostExchange` and `Gain` with a computed amount (met in ~5 % of self-play
  games), card biome modifiers (`updateExpeditions`), "behind" Defender exceptions, and Fen &
  Crowbar's choices when the deck is almost empty.
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
