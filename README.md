# Altered Arena

A modern, independent implementation of the Altered TCG: a rules engine in Rust, a lightweight
game server, and a web client (desktop and mobile, installable as a PWA) — built to replace the
Board Game Arena version's dated UI and to host games cheaply.

```
altered-arena/
├── engine/        Rust rules engine (no I/O, deterministic): game state, effect interpreter, bot
├── arena-server/  Rust HTTP server: protocol v1, matchmaking, persistence, static client + card art
├── client/        Web client: Vite + React + TypeScript, PWA, drag and drop, mobile layouts
├── protocol/      Client/server protocol (protocol.ts) + static data (card catalog, starter decks)
├── loadtest/      Load generator (N concurrent games with simulated players)
├── server/        PHP "oracle": the original BGA engine behind the same protocol (Docker), for tests
└── tools/         Card export, catalog build, differential tests, smoke tests
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
`PROTOCOL_DIR` (protocol/data), `DATA_DIR` (data: games are saved there and reloaded at start),
`ARENA_DEBUG=1` (enables `/api/debug/*`, for differential tests only — never in production).

## How it fits together

- **One protocol** (`protocol/protocol.ts`): the client fetches a `GameView` (what the player may
  see + the decision they must take) and answers it. Every decision is typed (`turn`, `select`,
  `choice`, `expedition`, `cleanup`...), so the client renders native interactions (drag a card to
  an expedition, pick targets on the board) instead of generic buttons. Long polling keeps clients
  in sync; each human seat has a secret token.
- **Rules engine** (`engine/`): card definitions are data exported from the original engine
  (`engine/data/cards.min.json`, 1976 cards): effects are flow trees (`seq`, `xor`, `parallel`,
  leaf actions such as `Gain`, `Target`, `Discard`, `InvokeToken`) interpreted by `flow.rs` /
  `actions.rs` / `special.rs`, with trigger conditions in `conditions.rs`. Card art and the rest of
  the static data come from the same export (`tools/build_catalog.py`).
  The engine is pure and deterministic (seeded RNG, serializable state), so it can also run in the
  browser (WebAssembly) for offline play — not wired yet.
- **Server** (`arena-server/`): games live in memory behind a mutex each, are saved to `DATA_DIR`
  a few seconds after a change, and are reloaded at start. Bots play in-process.

## Rules fidelity: the original engine as an oracle

The BGA engine (PHP) is the reference. `server/` runs it unchanged outside BGA (MariaDB in Docker,
using the AlteredBGA test harness' framework replacement) and exposes the same protocol, plus a full
state export. `tools/diff_engines.py` starts both engines from the exact same position (the PHP
game's decks, hands and Tumult are loaded into the Rust engine with the same card ids), plays the
same random decisions in both, and compares the full state after each one (every card's zone,
order, boosts, statuses, exhaustion, expedition positions). The first difference of each game is
reported with its context.

```bash
docker compose up -d                       # PHP oracle on :8080 (needs ../AlteredBGA)
tools/restart_rust.sh                      # Rust server on :8090 with ARENA_DEBUG
python3 tools/diff_engines.py 20           # 20 random games, both engines
cargo run --release --example selfplay 1000   # bot vs bot on all starter decks + unsupported effects met
```

See `docs/STATUS.md` for where fidelity stands.

## Tests and tools

| Command | What |
| --- | --- |
| `cargo run --release --example selfplay N` | N bot-vs-bot games on every starter deck pair; lists effects not implemented yet |
| `python3 tools/diff_engines.py N` | differential test against the original engine |
| `python3 tools/v1_smoke.py URL` | one game through protocol v1 (any server) |
| `cd client && npm run check:decisions -- --server URL` | games played by the client's own decision logic |
| `./target/release/loadtest --games 1000 --pid PID` | load test, see `docs/PERFORMANCE.md` |
| `php tools/export_cards.php` (in the PHP container) then `tools/build_catalog.py` | refresh card data from AlteredBGA |
