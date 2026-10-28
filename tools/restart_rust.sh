#!/bin/sh
# Rebuilds and restarts the Rust server on port 8090 (log in $TMPDIR/arena-rust.log)
cd "$(dirname "$0")/.." && cargo build --release -q -p arena-server || exit 1
pkill -f target/release/arena-server; sleep 0.3
ARENA_DEBUG=1 PORT=${PORT:-8090} DATA_DIR=${DATA_DIR:-/tmp/arena-data} nohup ./target/release/arena-server > "${TMPDIR:-/tmp}/arena-rust.log" 2>&1 &
sleep 0.7
