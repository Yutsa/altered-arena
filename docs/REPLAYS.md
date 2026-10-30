# Replays of BGA games

Crate `replay/` (`altered-replay`). Board Game Arena keeps, for a finished table, the notifications
of every request (`/archive/archive/logs.html?table=ID`), not the requests themselves. The replay
engine rebuilds each request on the translated engine, then writes the game in a native format that
replays without the BGA log and without any search.

## Converting a BGA replay

```
cargo run --release -p altered-replay --bin replay -- --native DIR BGA_REPLAY.json...
```

For each request of the log, every answer the position allows is tried on a copy of the game, and
the one whose effect matches the request is kept (`sig::diff`):

- the same game states, in order;
- every card the request shows at the same place, on the same side, exhausted or not; nothing else
  moved;
- meeples by place, type and state (their ids drift from BGA's after an undo);
- the same targeted cards, the die selected, each player's Mana total;
- the values BGA sends through `updateInformations` (region statistics, movements, blocked
  expeditions, defenders, Reserve and Landmark slots).

What the log does not record is rebuilt: the players' turn-confirmation preferences (from the
states), the order cards leave the decks (from the order the log shows them; within a request, every
interleaving of the channels), the ids BGA consumed in undone requests, first-day Mana choices taken
back. When several answers match, the next requests decide; when a request cannot be played, the
earlier uncertain choices are tried again (backtracking, up to 40 requests back). An answer that only
comes close is accepted last, if the next requests then replay.

## Native format

`TABLE.replay.json` (`native::NativeReplay`): the two custom decks, the dice, the Tumult cards, the
order of each deck shuffle, the preferences, and the steps (BGA move id, player, protocol v1
answer, plus the deck order, counters and preferences a step needs). The last line (`end`) is the
final state and a digest of every card's and meeple's place: `native::replay` checks the game ends
there. About 50 KB a game (the decks are most of it), against 1 MB of BGA log.

## Viewing

The game server lists the native replays of `ARENA_REPLAYS` (`GET /api/v1/replays`) and returns the
GameView of a seat after each step (`GET /api/v1/replays/TABLE?seat=2001`). The client shows them at
`?replay` (list) and `?replay=TABLE&seat=2001`: automatic play (speed ×0.25 to ×4), step by step
(arrows), slider.

## Tournament 617607

70 of the 74 games convert and replay to the end (about 2 minutes for all 74; a native replay plays
in under a second). The 4 others:

- 938023923, 938044321: a Unique "if you control a Fleeting Character, draw a card, otherwise
  Resupply". On BGA the draw resolves at once; the reference engine (PHP and Rust alike) offers the
  remaining effects as a choice first.
- 938051946: requests 52 and 53 are missing from the log; one put an unseen card from the deck in
  Mana, which the replay cannot tell.
- 938079521: a first-day Mana choice erased by an undo is found by backtracking; the game then stops
  at move 88 (not analysed).

The BGA logs and the native replays name the players: they stay in `private/` (git-ignored).
