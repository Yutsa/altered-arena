# Server performance

Goal: hold 1000 simultaneous games with as little RAM and CPU as possible and low latency.

## Method

- `loadtest/` keeps N games running at once against a fresh release server
  (`tools/perf_run.sh <name> --games N ...`). Half the games are between two simulated humans, half
  against the server's bot. Each simulated human behaves like the web client: it long-polls
  `GET /api/v1/games/{id}?since=` while waiting, answers its decision after a random think time
  (0.75–2.25 s, mean 1.5 s), sends `Accept-Encoding: gzip`, and starts a new game when one ends.
  Real players think 5–20 s per decision, so this load is several times harsher than real play.
- Runs: 90 s, games created over the first 20 s. Server CPU is measured as CPU time used during
  the run divided by the number of answers (`serverCpuMsPerAnswer`, the most stable metric: it
  includes the long-poll responses, bot moves, persistence and HTTP), plus the average CPU % of the
  steady state and the peak RSS. Latency is measured by the client for answers (POST → new view).
- Hardware: MacBook Pro M4 Pro (12 cores), macOS. The load generator runs on the same machine.
  A VPS core is typically 1.5–3× slower than an M4 Pro core: scale CPU numbers accordingly.
  Results vary by ±3 % between identical runs.

## Hill climbing (1000 games, 1500 simulated players, ~590 answers/s)

| Run | Change | CPU ms / answer | CPU (1 core = 100 %) | Peak RSS | Answer p50 / p99 / max |
| --- | --- | --- | --- | --- | --- |
| 0 | Baseline | 0.902 | 58.6 % | 151 MB | 0.88 / 2.27 / 53 ms |
| 1 | Views serialized from typed structs (no intermediate `serde_json::Value` tree) | 0.704 (−22 %) | 45.6 % | 152 MB | 0.83 / 2.50 / 220 ms |
| 2 | Save games every 10 s instead of 3 s; unload idle games (ended: 1 min, others: 30 min), reload on demand | 0.667 (−5 %) | 43.3 % | 145 MB | 0.88 / 2.33 / 25 ms |
| 3 | No full copy of the game per answer (answers are validated before any change), no undo snapshot for bots, log kept to 120 lines | 0.627 (−6 %) | 40.6 % | 133 MB | 0.85 / 2.25 / 22 ms |
| 4 | mimalloc allocator (`MIMALLOC_PURGE_DELAY=0`) | 0.589 (−6 %) | 38.7 % | 147 MB | 0.83 / 2.25 / 33 ms |
| 5 | gzip at the fastest level; views carry the last 40 log lines (clients merge them) | **0.473 (−20 %)** | **30.0 %** | **140 MB** | **0.77 / 2.02 / 16 ms** |

Overall: **−48 % CPU per answer** (0.90 → 0.47 ms), CPU for 1000 games from 59 % to 30 % of one
core, p99 latency 2.3 → 2.0 ms, worst case 53 → 16 ms, peak memory 151 → 140 MB.

Kept/rejected: run 4 (mimalloc) saves ~6 % CPU but costs ~10 MB at this scale; kept because the gain
grows with load (fewer allocator lock contentions) — revert it (one line in `arena-server/src/main.rs`)
if RAM is the scarcer resource on the target VPS. The engine itself got faster with run 3: a whole
bot-vs-bot game now takes 0.31 ms (0.84 ms before).

Profiles (macOS `sample`) guided each step: at the baseline, the active time went to JSON tree
building and serialization, game copies, and file writes; before run 5, gzip at the default level
was about half of the CPU.

## Capacity (final build)

| Games | Simulated players | Answers/s | CPU | Peak RSS | Answer p50 / p99 / max | Errors |
| --- | --- | --- | --- | --- | --- | --- |
| 1000 | 1500 | 590 | 30 % of 1 core | 140 MB | 0.77 / 2.0 / 16 ms | 0 |
| 3000 | 4500 | 1660 | 63 % | 318 MB | 0.59 / 2.0 / 46 ms | 0 |
| 6000 | 9000 | 3100 | 97 % | 564 MB | 0.55 / 2.9 / 71 ms | 0 |

Memory per game in play is about 90–120 KB including its share of connections, buffers and
recently ended games; a fresh game alone is ~16 KB. CPU per answer drops as load grows (batching
in the runtime). The server is multi-threaded (Tokio), so beyond one core it scales across cores.

**Sizing for 1000 simultaneous games:** 1 vCPU and 256 MB of RAM leave ample headroom even with
these aggressive think times (about 0.3–0.9 vCPU at the harsh load measured here, far less with real
players), and answers stay in the low milliseconds. Disk: about 25 KB of JSON per game.

## What's left to try

- WebSocket push instead of long polling: one connection per player, deltas instead of full views
  (fewer requests, fewer bytes, less gzip).
- Binary snapshots for persistence (the state contains JSON effect trees, which need a
  self-describing format today) and a write-ahead log of answers instead of full snapshots.
- Brotli/zstd for clients that support it, or no compression under ~1 KB.
- `SO_REUSEPORT` / several processes behind a balancer if one machine's single process is not enough.

# Translated engine (`rules`, `ARENA_ENGINE=rules`)

The server can run the faithful translation of the PHP engine (`rules/`, see `docs/RUST_PORT.md`).
Same method and load as above (1000 games, 1500 simulated players thinking 0.75–2.25 s, half against
the bot). Every kept run passed the non-regression gate: 6336/6336 golden files (125,565 nodes:
every card on three boards, 400 whole games, 8 real BGA replays), 52/52 unit tests, and an unchanged
checksum of 40 bot-vs-bot games (`rules/examples/bot_bench.rs`). Measurements in `perf/rules/`.

| Run | Change | CPU ms / answer | CPU | Peak RSS | Answer p50 / p99 |
| --- | --- | --- | --- | --- | --- |
| 0 | Literal translation | 7.49 | 425 % | 3332 MB | 4.1 / 28.0 ms |
| 1 | `Globals::fetch` re-reads only the variables written since the last fetch | 7.21 | 394 % | 2979 MB | 3.5 / 28.6 ms |
| 2 | Cheap copies of a game (shared globals, log, dice; a copy from 102 to 10 µs) | 6.05 | 320 % | 2505 MB | 2.5 / 22.4 ms |
| 3 | Cards share their class data until written | 5.35 | 294 % | 1991 MB | 2.3 / 20.6 ms |
| 4 | Card properties = class data + own values; games compacted between requests | 5.40 | 302 % | 973 MB | 2.2 / 22.0 ms |
| 5 | Location filters and string comparisons without copies | 4.06 | 235 % | 963 MB | 1.7 / 18.8 ms |
| 6 | Action args read in place, class defaults built once | 3.24 | 179 % | 978 MB | 1.5 / 10.8 ms |
| 7 | Integer hash for the row tables, quick rejection of non-numeric strings | 2.71 | 152 % | 962 MB | 1.3 / 9.8 ms |
| 8 | Card type filters without temporary arrays | 2.56 | 141 % | 977 MB | 1.3 / 7.7 ms |
| 9 | Many cards deleted in one pass (deck setup) | 2.38 | 140 % | 993 MB | 1.3 / 8.2 ms |
| 10 | Index of the cards by location | 1.89 | 117 % | 972 MB | 1.1 / 6.8 ms |
| **11** | Large global values (deck contents) kept as JSON text between requests | **1.86** | **113 %** | **707 MB** | **1.1 / 6.6 ms** |
| 12, 13 | Fewer copies at engine boot; typed class data with shared effect trees | reverted: within noise | | | |

Bot-vs-bot game: 488 → 126 ms. Game creation: 74 → 12 ms. Golden runner: 151 → ~60 s.
Validation loop per run: build ~16 s, goldens ~60 s, unit tests ~45 s, load test ~2 min (≈ 4 min).

**Plateau.** After run 11 the profile is flat: node evaluation (getChoices, isOptional, isDoable)
~30 %, serde_json value copies ~18 % with no caller above 2 %, effect trees rebuilt ~14 %, views
~13 %, bot game creation ~16 %; malloc/free ~30 % self. Typing one representation at a time gains
nothing measurable. Reaching the old engine (0.47 ms, 140 MB) needs the whole engine on typed data
at once (node infos and action args, card data in conditions and targeting, globals): a rewrite of
most of the 33k translated lines under the golden tests. Recursion is not a cost: the deepest chain
uses under 128 KB of stack (the 256 MB engine stacks are only reserved address space).

## Sizing: OVH VPS-1 (2 vCores, 4 GB RAM, 500 Mbit/s)

Run 11 in a Linux container limited to `--cpus=2 --memory=4g` (`tools/vps_run.sh`, results in
`perf/vps/`). Harsh load: think time 1.5 s. Realistic load: 6.25–18.75 s per decision (12.5 s mean).
No run had errors.

| Cores | Load | Games | CPU | RSS | p50 / p99 | |
| --- | --- | --- | --- | --- | --- | --- |
| 2 × M4 | harsh | 700 | 114 % | 395 MB | 2 / 38 ms | fits |
| 2 × M4 | harsh | 1000 | 148 % | 548 MB | 3 / 74 ms | limit |
| 2 × M4 | harsh | 1500 | 194 % | 763 MB | 120 / 423 ms | saturated |
| 2 × M4 | realistic, saves every 10 s | 6000 | 166 % | 2.0 GB | 4 / 954 ms | saturated |
| 2 × M4 | realistic, saves every 60 s | 6000 | 131 % | 2.0 GB | 2 / 78 ms | fits |
| 1.33 (vCore 1.5× slower) | realistic, saves every 60 s | 3000 | 68 % | 1.0 GB | 4 / 110 ms | limit |
| 1.33 | realistic, saves every 60 s | 4000 | 90 % | 1.4 GB | 8 / 1867 ms | too slow |
| 0.67 (vCore 3× slower) | realistic, saves every 60 s | 1500 | 39 % | 0.5 GB | 3 / 62 ms | fits (not ramped further) |

Capacity of a VPS-1 with this version: about 6000 realistic games if a vCore is as fast as an
M4 Pro core (with `PERSIST_SECS=60`), 2500–3000 if 1.5× slower, around 1500 if 3× slower; 400–1000
games under the harsh load. CPU is the limit. Memory is ~0.35 MB per game early on (more as games
progress), bandwidth 2.2–2.8 KB per request (9–22 Mbit/s at these loads). Saturation means slower
answers, not errors. The biggest remaining lever for realistic loads is persistence: a saved game is
~350 KB because every card's class data is written in full (in the cards and in the undo snapshot);
saving only each card's own values would cut the save cost severalfold.
