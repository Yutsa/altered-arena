"""Pairings of the tournament by deck, and the real standings (private/tournament-617607/pairings.json)."""
import json

DIR = 'private/tournament-617607'
t = json.load(open(f'{DIR}/tournament.json'))
td = json.load(open(f'{DIR}/table_decks.json'))
decks = json.load(open(f'{DIR}/decks.json'))
deck_of = {d['player']: did for did, d in decks.items()}

pairings = []
for m in t['matches']:
    if not m['table'] or m['end'] != 'normal_end':
        continue
    a, b = m['players']
    pairings.append({'table': m['table'], 'phase': m['phase'], 'group': m['group'], 'round': m['round'],
                     'a': deck_of[a], 'b': deck_of[b], 'winner': deck_of.get(m['winner'])})

# Real standings: Swiss wins (byes and forfeits count as BGA counted them: a win), then the
# elimination result for the top 8
standing = {did: {'deck': did, 'swissWins': 0, 'swissGames': 0, 'elimination': None} for did in decks}
for m in t['matches']:
    if m['phase'] != 'swiss':
        continue
    for p in m['players']:
        if p in deck_of:
            d = standing[deck_of[p]]
            if m['end'] == 'normal_end':
                d['swissGames'] += 1
            if m['winner'] == p or len(m['players']) == 1:
                d['swissWins'] += 1
for fs in t['final_standings']:
    if fs['player'] in deck_of:
        standing[deck_of[fs['player']]]['elimination'] = fs['result']
order = sorted(standing.values(), key=lambda d: (d['elimination'] or 99, -d['swissWins'], d['deck']))
for i, d in enumerate(order, 1):
    d['rank'] = d['elimination'] if d['elimination'] else i
json.dump({'pairings': pairings, 'standings': order}, open(f'{DIR}/pairings.json', 'w'), indent=1)
print(len(pairings), 'pairings;', len(set((min(p['a'], p['b']), max(p['a'], p['b'])) for p in pairings)), 'distinct deck pairs')
print('top 8:', [(d['deck'], d['rank']) for d in order[:8]])
