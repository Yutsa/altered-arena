"""Builds private/tournament-617607/tournament.json from the BGA overview JSON saved by the bridge."""
import json
import os
import sys

DIR = sys.argv[1] if len(sys.argv) > 1 else 'private/tournament-617607'
d = json.load(open(os.path.join(DIR, 'overview_main_raw.json')))['data']
players = {str(p['id']): p['name'] for p in d['players'].values()}
matches = []


def add(phase, group, rnd, m):
    ps = m['players']
    rec = {'phase': phase, 'group': group, 'round': rnd, 'match': m['id'], 'table': m.get('tableId'),
           'status': m['status'], 'end': m.get('endgameReason'),
           'players': [str(p['player_id']) for p in sorted(ps, key=lambda p: p['position'])]}
    win = [str(p['player_id']) for p in ps if p.get('result') == 1]
    rec['winner'] = win[0] if len(ps) == 2 and len(win) == 1 else None
    matches.append(rec)


for g in d['stages']['1']['groups']:
    for s in g['steps']:
        for m in s['matches']:
            add('swiss', g['index'], s['id'], m)
for s in d['stages']['2']['steps']:
    for m in s['matches']:
        add('elimination', None, s['id'], m)
for p in d['stages']['2']['players']:
    players.setdefault(str(p['id']), p['name'])
out = {'tournament': 617607, 'players': players,
       'final_standings': [{'player': str(p['id']), 'result': p['result']} for p in d['stages']['2']['players']],
       'matches': matches}
json.dump(out, open(os.path.join(DIR, 'tournament.json'), 'w'), indent=1)
played = [m for m in matches if len(m['players']) == 2 and m['table']]
print(len(players), 'players', len(matches), 'matches', len(played), 'played with a table')
for m in matches:
    if m['end'] != 'normal_end':
        print('  odd:', m['phase'], m['group'], m['round'], m['status'], m['end'], m['table'], m['winner'])
