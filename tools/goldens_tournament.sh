#!/bin/sh
# Records the replays of the tournament's games with the PHP oracle (as tools/goldens_replays.sh,
# for replays kept outside the harness): 4 processes. RUNNER: a directory with a copy of the
# harness's Replay.php / ReplayRunner.php to use instead (mounted at /runner).
#   tools/goldens_tournament.sh REPLAY_DIR [TABLE...]
# Output in server/goldens/tournament/replays/<table>.json (players renamed A/B, ids 2001/2002).
# Replay: cargo run --release -p goldens -- server/goldens --only tournament/
cd "${ROOT:-$(dirname "$0")/..}"
dir=$(cd "${1:?usage: tools/goldens_tournament.sh REPLAY_DIR [TABLE...]}" && pwd)
shift
harness=$(cd "${HARNESS:-../AlteredBGA/misc/harness}" && pwd)
tables=${*:-$(ls "$dir" | sed 's/\.json$//')}
runner=$(cd "${RUNNER:-$harness/tests/Replay}" && pwd)
export harness dir runner
for t in $tables; do echo "$t"; done | xargs -P 4 -L 1 sh -c \
  'test -s server/goldens/tournament/replays/$0.json || docker compose run --rm -T --no-deps -e REPLAY_HARNESS=/runner -v "$harness:/harness:ro" -v "$runner:/runner:ro" -v "$dir:/treplays:ro" server php tools/explore.php replay /treplays/$0.json /server/goldens/tournament'

ls server/goldens/tournament/replays/ | wc -l
