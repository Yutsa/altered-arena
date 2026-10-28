#!/usr/bin/env python3
"""Plays a game through protocol v1 with a random player against the server's bot.
Usage: tools/v1_smoke.py [base_url] [max_decisions]"""
import json, random, sys, urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else 'http://localhost:8080'
MAX = int(sys.argv[2]) if len(sys.argv) > 2 else 400

def call(method, path, body=None):
    req = urllib.request.Request(BASE + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                                 headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(req) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        return json.load(e)

decks = call('GET', '/api/v1/decks')
g = call('POST', '/api/v1/games', {'players': [{'name': 'Smoke', 'deck': random.choice(decks)['id']}, {'name': 'Bot', 'bot': True}]})
gid, me = g['gameId'], g['players'][0]['id']
view = call('GET', f'/api/v1/games/{gid}?player={me}')
kinds = {}
for step in range(MAX):
    if view.get('error'):
        print('ERROR', view); break
    d = view['decision']
    if view['phase'] == 'ended' or d is None:
        if view['phase'] == 'ended':
            break
        view = call('GET', f'/api/v1/games/{gid}?player={me}&since={view["seq"]}'); continue
    kinds[d['kind']] = kinds.get(d['kind'], 0) + 1
    k = d['kind']
    a = {'decisionId': d['id']}
    if k == 'deck': a |= {'type': 'deck', 'deck': d['decks'][0]['id']}
    elif k == 'select': a |= {'type': 'select', 'cardIds': random.sample(d['cardIds'], d['max'])}
    elif k == 'turn':
        if d['play'] and random.random() < 0.85:
            p = random.choice(d['play']); a |= {'type': 'play', 'cardId': p['cardId'], 'destination': random.choice(p['destinations'])}
        elif d['canEndTurn']: a |= {'type': 'endTurn'}
        else: a |= {'type': 'pass'}
    elif k == 'confirm': a |= {'type': 'confirm'}
    elif k == 'choice':
        en = [c for c in d['choices'] if c['enabled']]
        a |= {'type': 'choice', 'id': random.choice(en)['id']} if en else {'type': 'pass'}
    elif k == 'expedition': a |= {'type': 'expedition', 'keys': [e['key'] for e in d['expeditions'][:max(1, d['min'])]]}
    elif k == 'player': a |= {'type': 'player', 'player': d['players'][-1]}
    elif k == 'number': a |= {'type': 'number', 'value': d['values'][-1]}
    elif k == 'location': a |= {'type': 'location', 'key': d['locations'][0]['key']}
    elif k == 'exchange': a |= {'type': 'exchange', 'hand': d['hand'][0], 'reserve': d['reserve'][0]} if d['hand'] and d['reserve'] else {'type': 'pass'}
    elif k == 'cleanup': a |= {'type': 'cleanup', 'reserve': d['reserve']['cardIds'][:d['reserve']['n']], 'landmarks': d['landmarks']['cardIds'][:d['landmarks']['n']]}
    elif k == 'buttons': a |= {'type': 'button', 'key': d['buttons'][0]['key']} if d['buttons'] else {'type': 'pass'}
    new = call('POST', f'/api/v1/games/{gid}/answer?player={me}', a)
    if new.get('error'):
        print('refused', d['kind'], d['prompt'], a, new)
        if d.get('canPass'): new = call('POST', f'/api/v1/games/{gid}/answer?player={me}', {'decisionId': d['id'], 'type': 'pass'})
        if new.get('error'): print('STUCK'); break
    view = new
print('game', gid, 'phase', view.get('phase'), 'day', view.get('day'), 'winner', view.get('winner'), 'steps', step, kinds)
