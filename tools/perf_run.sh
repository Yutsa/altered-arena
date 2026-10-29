#!/bin/sh
# One load-test run against a fresh release server: tools/perf_run.sh <name> [loadtest args...]
# Result JSON in perf/<name>.json; the server runs on PERF_PORT (8091) with an empty data dir and
# the engine ARENA_ENGINE (legacy by default). PERF_SAMPLE=1 also records a `sample` profile of
# the server during the run (perf/<name>.sample.txt).
set -e
cd "$(dirname "$0")/.."
NAME=$1; shift
PERF_PORT=${PERF_PORT:-8091}
cargo build --release -q -p arena-server -p loadtest
DATA=$(mktemp -d)
PORT=$PERF_PORT DATA_DIR=$DATA ARENA_ENGINE=${ARENA_ENGINE:-legacy} MIMALLOC_PURGE_DELAY=0 ./target/release/arena-server > "$DATA/server.log" 2>&1 &
PID=$!
sleep 1
if [ -n "$PERF_SAMPLE" ]; then
  (sleep 40; sample $PID 20 -file "perf/$NAME.sample.txt" > /dev/null 2>&1) &
fi
./target/release/loadtest --url http://localhost:$PERF_PORT --pid $PID "$@" > "perf/$NAME.json"
du -sh "$DATA/games" | awk '{print "data on disk: " $1}'
ls "$DATA/games" | wc -l | awk '{print "games saved: " $1}'
grep -c . "$DATA/server.log" | awk '{print "server log lines: " $1}'
tail -5 "$DATA/server.log"
kill $PID; wait $PID 2>/dev/null || true
rm -rf "$DATA"
cat "perf/$NAME.json"
