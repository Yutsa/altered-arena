"""Extracts the decks of the tournament from its replays (private data, stays in private/).

Each player's confirmed custom deck is in the `updateInitialPrecoDeckSelection` notification
(`_private.API`, the content `actConfirmAPIDeck` received). Writes:
  private/tournament-617607/decks.json   deck id -> player, hero, faction, cards, API content, tables
  server/goldens/tournament_decks.json   deck id -> API content without name/id (input of explore.php deckgame)
  docs/ai/decks.md                       the anonymised list
"""
import base64
import hashlib
import json
import os
import re
import sys

DIR = 'private/tournament-617607'
t = json.load(open(f'{DIR}/tournament.json'))
names = t['players']


def api_player(api):
    s = api['id']
    try:
        return str(json.loads(base64.b64decode(s + '=' * (-len(s) % 4)))['payload']['playerId'])
    except Exception:
        # The hero entry carries its owner
        return str(api['cards']['hero'].get('pId'))


def anonymous(api):
    out = {k: v for k, v in api.items() if k not in ('id', 'deckName', 'apiId')}
    out['deckName'] = 'API deck'
    return out


decks = {}  # (player, hash) -> info
per_table = {}
for m in t['matches']:
    tab = m['table']
    f = f'{DIR}/replays/{tab}.json'
    if not tab or not os.path.exists(f):
        continue
    r = json.load(open(f))
    found = {}
    for log in r['data']['logs']:
        for n in log['data']:
            if n['type'] != 'updateInitialPrecoDeckSelection':
                continue
            p = n['args']['args'].get('_private', {})
            if p.get('selection') == 'API' and p.get('API'):
                pid = api_player(p['API'])
                found[pid] = p['API']
            elif p.get('selection') is not None:
                print(f'table {tab}: selection {p.get("selection")!r} (not an API deck)', file=sys.stderr)
    per_table[tab] = {}
    for pid, api in found.items():
        h = hashlib.sha1(json.dumps(anonymous(api)['cards'], sort_keys=True).encode()).hexdigest()[:10]
        key = (pid, h)
        if key not in decks:
            decks[key] = {'player': pid, 'name': names.get(pid), 'hash': h, 'api': api, 'tables': []}
        decks[key]['tables'].append(tab)
        per_table[tab][pid] = h

# Deck ids: by faction then hero then player id, D01..
def hero_props(api):
    return api['cards']['hero']['card']['properties']

order = sorted(decks.values(), key=lambda d: (hero_props(d['api'])['faction'], hero_props(d['api'])['name'], d['player'], d['hash']))
out = {}
golden_in = {}
for i, d in enumerate(order, 1):
    hp = hero_props(d['api'])
    did = f'D{i:02d}'
    cards = []
    for k, c in d['api']['cards'].items():
        if k == 'hero':
            continue
        p = c['card']['properties']
        cards.append({'uid': p['uid'], 'name': p['name'], 'faction': p['faction'], 'rarity': p.get('rarity'), 'type': p.get('type'), 'n': c['n'], 'unique': bool(re.search(r'_U_\d+$', p['uid']))})
    out[did] = {'id': did, 'label': f'{did} {hp["name"].split(" & ")[0]} {hp["faction"]}', 'player': d['player'], 'playerName': d['name'],
                'hero': hp['uid'], 'heroName': hp['name'], 'faction': hp['faction'], 'cardCount': sum(c['n'] for c in cards),
                'cards': cards, 'tables': d['tables'], 'api': d['api']}
    golden_in[did] = anonymous(d['api'])
json.dump(out, open(f'{DIR}/decks.json', 'w'), indent=1)
os.makedirs('server/goldens', exist_ok=True)
json.dump(golden_in, open('server/goldens/tournament_decks.json', 'w'))

# Table -> deck ids
key2id = {(d['player'], d['hash']): did for did, d in out.items() for d in [d] if False}
key2id = {(o['player'], decks_h): did for did, o in out.items() for decks_h in [hashlib.sha1(json.dumps(anonymous(o['api'])['cards'], sort_keys=True).encode()).hexdigest()[:10]]}
tables = {str(tab): {pid: key2id[(pid, h)] for pid, h in v.items()} for tab, v in per_table.items()}
json.dump(tables, open(f'{DIR}/table_decks.json', 'w'), indent=1)

players_decks = {}
for did, o in out.items():
    players_decks.setdefault(o['player'], []).append(did)
multi = {p: v for p, v in players_decks.items() if len(v) > 1}
print(f'{len(out)} decks from {len(per_table)} tables, {len(players_decks)} players; players with several decks: {multi}')

# Public, anonymised list
lines = ['# Decks of tournament 617607', '', 'Anonymised: decks are named by number, hero and faction. Uniques show their base card.', '',
         '| Deck | Hero | Cards | Uniques | Tables |', '| --- | --- | --- | --- | --- |']
for did, o in out.items():
    lines.append(f'| {o["label"]} | {o["heroName"]} | {o["cardCount"]} | {sum(c["n"] for c in o["cards"] if c["unique"])} | {len(o["tables"])} |')
lines.append('')
for did, o in out.items():
    lines.append(f'## {o["label"]} ({o["heroName"]})')
    lines.append('')
    for c in sorted(o['cards'], key=lambda c: (c['type'] or '', c['name'])):
        lines.append(f'- {c["n"]}x {c["name"]} ({c["faction"]}, {"Unique" if c["unique"] else ["Common", "Rare", "Unique", "Exalted"][c["rarity"] or 0]}, {c["type"]})')
    lines.append('')
open('docs/ai/decks.md', 'w').write('\n'.join(lines))

# Seat order of each table (2001 = first player of the replay) -> deck ids
import os
seats = {}
for f in os.listdir(f"{DIR}/replays"):
    r = json.load(open(f"{DIR}/replays/{f}"))
    ps = [str(p["id"]) for p in r["data"]["players"]]
    seats[f[:-5]] = {"2001": tables[f[:-5]][ps[0]], "2002": tables[f[:-5]][ps[1]]}
json.dump(seats, open(f"{DIR}/table_seats.json", "w"), indent=1)
