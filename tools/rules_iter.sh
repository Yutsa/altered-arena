#!/bin/sh
# One timed iteration of the performance loop of the translated engine (rules):
#   tools/rules_iter.sh <name> [--quick]
# 1. release build, 2. golden runner (whole set; --quick: a subset, see QUICK below), 3. the rules
# tests, 4. the 1000-game load test (tools/perf_run.sh, 90 s). Stops at the first failure.
# Wall-clock time of each step in perf/rules/<name>.timings.json, load test result in
# perf/rules/<name>.json. GOLDENS: the golden files (not in git), PERF_PORT: the server's port.
set -e
cd "$(dirname "$0")/.."
NAME=$1
QUICK=$2
GOLDENS=${GOLDENS:-/Users/edouardwillisseck/dev/perso/altered/altered-arena/server/goldens}
now() { perl -MTime::HiRes=time -e 'printf "%.1f", time'; }
d() { perl -e "printf '%.1f', $1 - $2"; }
T0=$(now)
cargo build --release -q -p arena-server -p loadtest -p goldens
T1=$(now)
if [ "$QUICK" = "--quick" ]; then
  # Whole games and real replays only (408 files, 44k of the 126k nodes, ~40 s): the most
  # varied positions; the per-card explorations are the rest
  OUT=$(cargo run --release -q -p goldens -- "$GOLDENS" --only /games/ 2>&1 | tail -1)
  echo "goldens (games): $OUT"
  echo "$OUT" | grep -qE '^([0-9]+)/\1 files pass' || { echo "GOLDEN FAILURE"; exit 1; }
  OUT=$(cargo run --release -q -p goldens -- "$GOLDENS" --only /replays/ 2>&1 | tail -1)
else
  OUT=$(cargo run --release -q -p goldens -- "$GOLDENS" 2>&1 | tail -1)
fi
echo "goldens: $OUT"
echo "$OUT" | grep -qE '^([0-9]+)/\1 files pass' || { echo "GOLDEN FAILURE"; exit 1; }
T2=$(now)
TESTS=$(ALTERED_GOLDENS=$GOLDENS GOLDENS_DIR=$GOLDENS cargo test --release -q -p altered-rules 2>&1 | grep -E "^test result" || true)
echo "$TESTS" | grep -q "FAILED" && { echo "TEST FAILURE"; echo "$TESTS"; exit 1; }
PASSED=$(echo "$TESTS" | awk '{s += $4} END {print s}')
echo "tests: $PASSED passed"
T3=$(now)
PERF_PORT=${PERF_PORT:-8095} tools/perf_run.sh "rules/$NAME" --games 1000 --duration 90 --ramp 20 > /dev/null 2>&1
T4=$(now)
cat > "perf/rules/$NAME.timings.json" <<EOF
{"build": $(d $T1 $T0), "goldens": $(d $T2 $T1), "goldensQuick": $([ "$QUICK" = "--quick" ] && echo true || echo false), "goldenSummary": "$OUT", "tests": $(d $T3 $T2), "testsPassed": $PASSED, "loadtest": $(d $T4 $T3), "total": $(d $T4 $T0)}
EOF
cat "perf/rules/$NAME.timings.json"
python3 -c "
import json; d = json.load(open('perf/rules/$NAME.json'))
print({k: d[k] for k in ['answers', 'gamesFinished', 'errors', 'refused', 'serverCpuMsPerAnswer', 'serverCpuPercentAvg']}, d['answerLatencyMs'], 'create p50', d['createLatencyMs']['p50'], d['serverRssMB'])"
