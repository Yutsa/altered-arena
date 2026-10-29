#!/bin/sh
# Runs the differential test in parallel: tools/diff_many.sh GAMES_PER_RUN SEED1 SEED2 ...
# Output of each run in $OUT/seed-N.txt (default /tmp/diff-runs), then a summary.
cd "$(dirname "$0")/.."
OUT=${OUT:-/tmp/diff-runs}; mkdir -p "$OUT"
n=$1; shift
for s in "$@"; do python3 tools/diff_engines.py "$n" --seed "$s" > "$OUT/seed-$s.txt" 2>&1 & done
wait
for s in "$@"; do tail -2 "$OUT/seed-$s.txt" | head -1; done
python3 - "$OUT" "$@" <<'PY'
import sys, re
out, seeds = sys.argv[1], sys.argv[2:]
same = div = tot = 0
for s in seeds:
    t = open(f'{out}/seed-{s}.txt').read()
    for m in re.finditer(r'^game \d+: (\w+)', t, re.M):
        tot += 1
        same += m.group(1) == 'identical'
        div += m.group(1) == 'DIVERGES'
print(f'TOTAL {same}/{tot} identical, {div} diverge')
PY
