"""Copies a parameter directory with some fields changed: variant.py SRC DST key=value ...
(values are JSON; keys may be weights.x). key*=f multiplies the field (its default when absent)."""
import json
import os
import sys

src, dst = sys.argv[1], sys.argv[2]
sets = [a.split('=', 1) for a in sys.argv[3:]]
# Defaults of the multiplied fields (search.rs Params, eval.rs Weights)
DEFAULTS = {'weights.progress': 12.0, 'weights.dusk_win': 5.0, 'weights.margin': 0.6, 'weights.board': 0.25, 'weights.hand': 0.6,
            'weights.reserve': 0.5, 'weights.mana': 1.6, 'weights.landmark': 0.6, 'weights.empty_expedition': 1.0}
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
        if k.endswith('*'):
            last = parts[-1][:-1]
            cur[last] = round(cur.get(last, DEFAULTS[k[:-1]]) * json.loads(v), 4)
        else:
            cur[parts[-1]] = json.loads(v)
    json.dump(params, open(f'{dst}/{d}.json', 'w'), indent=1, sort_keys=True)
print(f'{dst}: {len(decks)} decks')
