"""Per-deck priors from the real players' decisions (phase D): ai/params/DXX.json.

    python3 tools/ai/build_priors.py [PARAMS_DIR]

Reads private/tournament-617607/decisions.json (aitool agree): for each deck, how often each
card was played when it could be (turn decisions) and put in Mana when offered (Mana choices),
shrunk towards the deck's mean rate. The priors are added to the AI's scores (evaluation points)
and break its ties. Other fields of existing parameter files are kept. Writes the anonymised
summary docs/ai/tendencies.md.
"""
import collections
import json
import os
import sys

DIR = 'private/tournament-617607'
OUT = sys.argv[1] if len(sys.argv) > 1 else 'ai/params'
recs = [r for r in json.load(open(f'{DIR}/decisions.json')) if 'error' not in r]
decks = json.load(open(f'{DIR}/decks.json'))
SHRINK = 2.0
PLAY_SCALE = 2.0
MANA_SCALE = 3.0


def rates(pairs):
    offered = collections.Counter()
    chosen = collections.Counter()
    for off, ch in pairs:
        for c in off:
            offered[c] += 1
        for c in ch:
            chosen[c] += 1
    tot_o = sum(offered.values())
    mean = sum(chosen.values()) / tot_o if tot_o else 0
    out = {}
    for c, o in offered.items():
        out[c] = ((chosen[c] + SHRINK * mean) / (o + SHRINK), o, chosen[c])
    return out, mean


os.makedirs(OUT, exist_ok=True)
md = ['# Tendencies of the tournament players', '',
      'From the tournament games the PHP oracle replays exactly. Rate: share of the decisions where the card could be played '
      '(or put in Mana) and was. Only cards seen at least 3 times.', '']
per_deck = collections.defaultdict(lambda: {'play': [], 'mana': [], 'manaPass': collections.Counter(), 'n': 0})
for r in recs:
    d = per_deck[r['deck']]
    d['n'] += 1
    prompt = (r.get('prompt') or '').lower()
    if r['kind'] == 'turn':
        played = r['chosen'] if r['real'].get('type') == 'play' else []
        d['play'].append((sorted(set(r['offered'])), played))
    elif r['kind'] == 'select' and 'mana' in prompt:
        if r['real'].get('type') == 'pass':
            d['manaPass'][r['day']] += 1
        else:
            d['mana'].append((r['offered'], r['chosen']))
for did in sorted(decks):
    path = f'{OUT}/{did}.json'
    params = json.load(open(path)) if os.path.exists(path) else {}
    d = per_deck.get(did)
    if not d:
        continue
    play, pmean = rates(d['play'])
    mana, mmean = rates(d['mana'])
    params['play_prior'] = {c: round(PLAY_SCALE * (r - pmean), 3) for c, (r, o, ch) in play.items() if o >= 2}
    params['mana_prior'] = {c: round(MANA_SCALE * (r - mmean), 3) for c, (r, o, ch) in mana.items() if o >= 2}
    json.dump(params, open(path, 'w'), indent=1, sort_keys=True)
    label = decks[did]['label']
    md.append(f'## {label}')
    md.append('')
    md.append(f'{d["n"]} decisions with a choice in {len(set(r["table"] for r in recs if r["deck"] == did))} games; '
              f'Mana passed {sum(d["manaPass"].values())} times of {sum(d["manaPass"].values()) + len(d["mana"])} Mornings.')
    md.append('')
    top = sorted(((r, c, o, ch) for c, (r, o, ch) in play.items() if o >= 3), reverse=True)
    if top:
        md.append('Played first when possible: ' + ', '.join(f'{c} ({ch}/{o})' for r, c, o, ch in top[:5]) + '.')
        md.append('Held back: ' + ', '.join(f'{c} ({ch}/{o})' for r, c, o, ch in top[-3:]) + '.')
    mtop = sorted(((r, c, o, ch) for c, (r, o, ch) in mana.items() if o >= 3), reverse=True)
    if mtop:
        md.append('Put in Mana: ' + ', '.join(f'{c} ({ch}/{o})' for r, c, o, ch in mtop[:4]) + '.')
    md.append('')
open('docs/ai/tendencies.md', 'w').write('\n'.join(md))
print(len(per_deck), 'decks with decisions;', len(recs), 'decisions')
