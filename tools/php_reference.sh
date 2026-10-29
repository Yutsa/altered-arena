#!/bin/sh
# Extracts the reference PHP engine (AlteredBGA origin/master, without unmerged branches) to
# ../AlteredBGA-ref and restarts the oracle on it (.env sets GAME_DIR for docker compose)
cd "$(dirname "$0")/.."
(cd ../AlteredBGA && git fetch -q origin && git log -1 --format='reference: %h %ad %s' --date=short origin/master)
rm -rf ../AlteredBGA-ref && mkdir ../AlteredBGA-ref
(cd ../AlteredBGA && git archive origin/master) | tar -x -C ../AlteredBGA-ref
grep -q GAME_DIR .env 2>/dev/null || echo "GAME_DIR=../AlteredBGA-ref" >> .env
docker compose up -d server
