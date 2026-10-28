#!/bin/sh
# One load-test run against a fresh release server: tools/perf_run.sh <name> [loadtest args...]
# Result JSON in perf/<name>.json; the server runs on port 8091 with an empty data dir.
set -e
cd "$(dirname "$0")/.."
NAME=$1; shift
cargo build --release -q -p arena-server -p loadtest
DATA=$(mktemp -d)
PORT=8091 DATA_DIR=$DATA ./target/release/arena-server > "$DATA/server.log" 2>&1 &
PID=$!
sleep 1
./target/release/loadtest --url http://localhost:8091 --pid $PID "$@" > "perf/$NAME.json"
du -sh "$DATA/games" | awk '{print "data on disk: " $1}'
ls "$DATA/games" | wc -l | awk '{print "games saved: " $1}'
kill $PID; wait $PID 2>/dev/null || true
rm -rf "$DATA"
cat "perf/$NAME.json"
