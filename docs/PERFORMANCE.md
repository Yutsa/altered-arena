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
