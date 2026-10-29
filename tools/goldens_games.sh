#!/bin/sh
# Records whole random games with the PHP oracle (6 processes):
#   tools/goldens_games.sh N [FIRST_SEED]   — seeds FIRST_SEED .. FIRST_SEED+N-1 (default 1)
# Output in server/goldens/games/<seed>.json (seeds already recorded are skipped).
# KEEP=1 keeps each game's database (read its log with tools/php_log.py <phpGame>).
# Replay: cargo run --release -p goldens -- server/goldens --only games/
# ROOT: the checkout whose server/ is mounted in the container (default: this one)
cd "${ROOT:-$(dirname "$0")/..}"
n=${1:?usage: tools/goldens_games.sh N [FIRST_SEED]}
first=${2:-1}
keep=${KEEP:+--keep}
seq "$first" $((first + n - 1)) | xargs -P 6 -I {} sh -c \
  "test -s server/goldens/games/{}.json || docker compose exec -T server php tools/explore.php game {} $keep 2>>/tmp/goldens-games-errors.txt || echo 'FAILED game {}' >> /tmp/goldens-games-errors.txt"
ls server/goldens/games/ | wc -l
