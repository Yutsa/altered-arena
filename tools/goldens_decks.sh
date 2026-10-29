#!/bin/sh
# Records whole random games between custom decks with the PHP oracle (6 processes):
#   tools/goldens_decks.sh PAIRS_FILE   — one line per game: SEED DECK_A DECK_B
# Decks: server/goldens/tournament_decks.json (deck id -> API deck content, tools/ai/extract_decks.py).
# Output in server/goldens/decks/A-B-SEED.json (games already recorded are skipped).
# Replay: cargo run --release -p goldens -- server/goldens --only decks/ [--create]
cd "${ROOT:-$(dirname "$0")/..}"
xargs -P 6 -L 1 sh -c \
  'test -s server/goldens/decks/$1-$2-$0.json || docker compose exec -T server php tools/explore.php deckgame $0 /server/goldens/tournament_decks.json $1 $2 2>>/tmp/goldens-decks-errors.txt || echo "FAILED $0 $1 $2" >> /tmp/goldens-decks-errors.txt' < "${1:?usage: tools/goldens_decks.sh PAIRS_FILE}"
ls server/goldens/decks/ | wc -l
