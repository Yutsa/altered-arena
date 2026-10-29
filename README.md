# Altered Arena

A modern, independent implementation of the Altered TCG: a rules engine in Rust, a lightweight
game server, and a web client (desktop and mobile, installable as a PWA) — built to replace the
Board Game Arena version's dated UI and to host games cheaply. The rules engine is a faithful
translation of the BGA engine (PHP): it asks the same questions and reaches the same states, checked
against the PHP engine on every card, 400 whole games and 8 real BGA games.

```
altered-arena/
├── rules/         Rust rules engine: faithful translation of the BGA PHP engine, card data, bot
├── goldens/       Replays the golden decision trees recorded with the PHP engine against rules/
├── arena-server/  Rust HTTP server: protocol v1, matchmaking, persistence, static client + card art
├── client/        Web client: Vite + React + TypeScript, PWA, drag and drop, mobile layouts
├── protocol/      Client/server protocol (protocol.ts) + static data (card catalog, starter decks)
├── loadtest/      Load generator (N concurrent games with simulated players)
├── server/        PHP "oracle": the original BGA engine behind the same protocol (Docker), for tests
└── tools/         Card export, catalog build, golden recording, load and smoke tests
```

## Run it

Requirements: Rust (stable), Node 20+. Card art is read from a checkout of AlteredBGA next to this
folder (`../AlteredBGA/img/illustrations`, override with `ART_DIR`).

```bash
cd client && npm install && npm run build && cd ..
cargo run --release -p arena-server        # http://localhost:8080
```

Open http://localhost:8080, pick a starter deck, play against the bot, find an opponent
(matchmaking), or open a two-player table (one link per seat).

Client development with hot reload: `cd client && ARENA_API=http://localhost:8080 npm run dev`
(http://localhost:5173).

Server settings (environment): `PORT` (8080), `CLIENT_DIR` (client/dist), `ART_DIR`,
`PROTOCOL_DIR` (protocol/data), `DATA_DIR` (data: games are saved there every `PERSIST_SECS` (10)
and reloaded on demand), `ENGINE_THREADS` (threads running the engine, default: one per CPU) and
`ENGINE_STACK_MB` (their stacks, 256: requests recurse like PHP's), `ARENA_DEBUG=1` (enables
`/api/debug/*`, the hidden state of a game — never in production), `MIMALLOC_PURGE_DELAY=0`
(recommended: frees memory sooner).

Deployment: from the parent folder (it holds AlteredBGA, whose card art goes in the image),
`docker build -t altered-arena -f altered-arena/Dockerfile .`, then
`docker run -p 8080:8080 -v arena-data:/data altered-arena`. Performance and sizing:
`docs/PERFORMANCE.md` (OVH VPS-1, 2 vCores and 4 GB: about 1500–6000 simultaneous games at a
realistic pace depending on the vCore speed, with `PERSIST_SECS=60`).

## How it fits together

- **One protocol** (`protocol/protocol.ts`): the client fetches a `GameView` (what the player may
  see + the decision they must take) and answers it. Every decision is typed (`turn`, `select`,
  `choice`, `expedition`, `cleanup`...), so the client renders native interactions (drag a card to
  an expedition, pick targets on the board) instead of generic buttons. Long polling keeps clients
  in sync; each human seat has a secret token.
- **Rules engine** (`rules/`): a translation of the AlteredBGA PHP engine, function by function
  (`docs/PHP_ENGINE.md` maps the PHP code, `docs/RUST_PORT.md` the translation): the BGA framework's
  state machine, the engine's decision tree, the 39 actions, the conditions and special effects, the
  card and player computations, the phases of the day, and the protocol v1 adapter. Card data is the
  PHP card classes exported as JSON (`rules/data/cards.min.json`, 1976 cards). PHP's randomness
  (mt_rand, shuffle, array_rand) is reproduced exactly, seeded per action.
- **Server** (`arena-server/`): games live in memory behind a mutex each, are saved to `DATA_DIR`
  a few seconds after a change, and are reloaded at start. Bots play in-process.

## Rules fidelity: golden tests recorded with the PHP engine

The BGA engine (PHP) is the reference. `server/` runs it unchanged outside BGA (MariaDB in Docker;
the reference version is AlteredBGA `origin/master`, extracted by `tools/php_reference.sh`) and
records golden files (`server/tools/explore.php`, not in git): for each card on three boards, every
branch of the decisions it leads to; whole games played at random; and the real BGA games of the
AlteredBGA test harness, anonymised. `goldens/` replays them against `rules/`: at every step the
player waited for, the decision and the full state must be the same.

```bash
docker compose up -d                                  # PHP oracle on :8080 (needs ../AlteredBGA)
tools/goldens.sh                                      # every card on 3 boards (~1 h 40, once)
tools/goldens_games.sh 400                            # 400 whole random games
tools/goldens_replays.sh                              # the real BGA games of the harness
cargo run --release -p goldens -- server/goldens      # replay them all (~1 min)
```

See `docs/STATUS.md` for where things stand.

## Tests and tools

| Command | What |
| --- | --- |
| `cargo run --release -p goldens -- server/goldens [--only X]` | golden tests (see above); `--create` also checks game creation |
| `cargo test --release -p altered-rules` | unit tests (set `ALTERED_GOLDENS`/`GOLDENS_DIR` to the golden directory) |
| `cargo run --release -p altered-rules --example bot_bench N` | N bot-vs-bot games, time per game and a checksum of the results |
| `tools/rules_iter.sh` | one performance iteration: build, goldens, tests, load test |
| `python3 tools/v1_smoke.py URL` | one game through protocol v1 (any server) |
| `cd client && npm run check:decisions -- --server URL` | games played by the client's own decision logic |
| `./target/release/loadtest --games 1000 --pid PID` | load test, see `docs/PERFORMANCE.md` |
| `php tools/export_cards.php` (in the PHP container) then `tools/build_catalog.py` | refresh card data from AlteredBGA |
