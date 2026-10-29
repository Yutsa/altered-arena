#!/bin/sh
# Records the replays of real BGA games with the PHP oracle (one process per replay):
#   tools/goldens_replays.sh [TABLE]   — every HARNESS/replays/*.json, or one table
# HARNESS: the AlteredBGA test harness (misc/harness), which holds the replays and the
# ReplayRunner that rebuilds their requests (default: ../AlteredBGA/misc/harness).
# Output in server/goldens/replays/<table>.json (players renamed A/B, ids 2001/2002).
# Replay: cargo run --release -p goldens -- server/goldens --only replays/
# ROOT: the checkout whose server/ is mounted in the container (default: this one)
cd "${ROOT:-$(dirname "$0")/..}"
harness=$(cd "${HARNESS:-../AlteredBGA/misc/harness}" && pwd)
for f in "$harness"/replays/${1:-*}.json; do
  docker compose run --rm -T --no-deps -v "$harness:/harness:ro" server \
    php tools/explore.php replay "/harness/replays/$(basename "$f")" &
done
wait
ls server/goldens/replays/ | wc -l
