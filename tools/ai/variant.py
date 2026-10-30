"""Copies a parameter directory with some fields changed: variant.py SRC DST key=value ...
(values are JSON; keys may be weights.x)."""
import json
import os
import sys

src, dst = sys.argv[1], sys.argv[2]
sets = [a.split('=', 1) for a in sys.argv[3:]]
os.makedirs(dst, exist_ok=True)
decks = json.load(open('server/goldens/tournament_decks.json'))
for d in decks:
    p = f'{src}/{d}.json'
    params = json.load(open(p)) if os.path.exists(p) else {}
    for k, v in sets:
        cur = params
        parts = k.split('.')
        for part in parts[:-1]:
            cur = cur.setdefault(part, {})
        cur[parts[-1]] = json.loads(v)
    json.dump(params, open(f'{dst}/{d}.json', 'w'), indent=1, sort_keys=True)
print(f'{dst}: {len(decks)} decks')
