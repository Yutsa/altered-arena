#!/usr/bin/env python3
"""Differential test: the Rust engine against the original BGA engine (PHP oracle).

Both engines start from the same position (the oracle's state after the opening draw is loaded
into the Rust server), then the same random decisions are sent to both; after each decision the
full states (every card's zone, order, boosts, statuses, exhaustion, expedition positions) are
compared. The first difference of each game is reported.

Usage: tools/diff_engines.py [games] [--php URL] [--rust URL] [--seed N]
Requires the PHP oracle (docker compose up) and the Rust server with ARENA_DEBUG=1 (tools/restart_rust.sh).
"""
import argparse, json, random, sys, urllib.request

ap = argparse.ArgumentParser()
ap.add_argument('games', nargs='?', type=int, default=5)
ap.add_argument('--php', default='http://localhost:8080')
ap.add_argument('--rust', default='http://localhost:8090')
ap.add_argument('--seed', type=int, default=1)
ap.add_argument('--max', type=int, default=600, help='decisions per game')
ap.add_argument('-v', action='store_true')
opt = ap.parse_args()
rnd = random.Random(opt.seed)


def call(base, method, path, body=None):
    req = urllib.request.Request(base + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                                 headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return json.load(r)
    except urllib.error.HTTPError as e:
        try:
            return json.load(e)
        except Exception:
            return {'error': f'HTTP {e.code}'}


ZONES_COMPARED = ['hand', 'deck', 'reserve', 'landmark', 'heroExpedition', 'companionExpedition', 'discard', 'mana', 'hero']


def normalize(state):
    """{(owner, zone): [(id, ref, exhausted, boosts, statuses)]} + positions"""
    zones = {}
    for c in state['cards']:
        z = c['zone']
        if z not in ZONES_COMPARED:
            continue
        key = (c['owner'], z)
        exhausted = c['exhausted'] if z not in ('deck', 'hand', 'discard') else False
        item = (c['id'], c['ref'], exhausted, c['boosts'] if z not in ('hand', 'deck', 'discard') else 0,
                tuple(sorted(c['statuses'])) if z not in ('hand', 'deck', 'discard') else ())
        if z == 'mana':
            # which orb is exhausted doesn't matter, how many does
            item = (0, c['ref'], False, 0, ())
            zones.setdefault((c['owner'], 'mana exhausted'), []).append((0, (-1, 'exhausted orbs', int(c['exhausted']), 0, ())))
        zones.setdefault(key, []).append((c.get('order', 0), item))
    out = {}
    for k in [k for k in zones if k[1] == 'mana exhausted']:
        zones[k] = [(0, (-1, 'exhausted orbs', sum(x[1][2] for x in zones[k]), 0, ()))]
    for k, items in zones.items():
        items.sort(key=lambda x: x[0])
        # Only the deck's order matters (draws); other zones are compared as sets
        out[k] = [i for _, i in items] if k[1] == 'deck' else sorted(i for _, i in items)
    pos = {p['id']: tuple(p['position']) for p in state['players']}
    return out, pos


def diff(php, rust):
    (pz, pp), (rz, rp) = normalize(php), normalize(rust)
    problems = []
    if pp != rp:
        problems.append(f'expedition positions: php {pp} rust {rp}')
    for k in sorted(set(pz) | set(rz), key=str):
        a, b = pz.get(k, []), rz.get(k, [])
        if a != b:
            ids_a, ids_b = {x[0]: x for x in a}, {x[0]: x for x in b}
            only_a = [ids_a[i][1] for i in ids_a if i not in ids_b]
            only_b = [ids_b[i][1] for i in ids_b if i not in ids_a]
            changed = [(ids_a[i], ids_b[i]) for i in ids_a if i in ids_b and ids_a[i] != ids_b[i]]
            msg = f'{k}: '
            if only_a: msg += f'only php {only_a} '
            if only_b: msg += f'only rust {only_b} '
            if changed: msg += f'differ {changed[:3]} '
            if not (only_a or only_b or changed): msg += 'order differs'
            problems.append(msg)
    return problems


def pick_answer(d):
    k = d['kind']
    a = {'decisionId': d['id']}
    if k == 'select':
        n = rnd.randint(d['min'], d['max']) if d['max'] > d['min'] else d['max']
        return a | {'type': 'select', 'cardIds': rnd.sample(d['cardIds'], n)}
    if k == 'turn':
        if d['play'] and rnd.random() < 0.8:
            p = rnd.choice(d['play'])
            dests = [x for x in p['destinations'] if 'Scout' not in x] or p['destinations']
            return a | {'type': 'play', 'cardId': p['cardId'], 'destination': rnd.choice(dests)}
        if d.get('tap') and rnd.random() < 0.3:
            return a | {'type': 'tap', 'cardId': rnd.choice(d['tap'])}
        if d['canEndTurn']: return a | {'type': 'endTurn'}
        return a | {'type': 'pass'}
    if k == 'confirm': return a | {'type': 'confirm'}
    if k == 'choice':
        en = [c for c in d['choices'] if c['enabled']]
        return a | ({'type': 'choice', 'id': rnd.choice(en)['id']} if en else {'type': 'pass'})
    if k == 'expedition': return a | {'type': 'expedition', 'keys': [e['key'] for e in rnd.sample(d['expeditions'], max(1, d['min']))]}
    if k == 'player': return a | {'type': 'player', 'player': rnd.choice(d['players'])}
    if k == 'number': return a | {'type': 'number', 'value': rnd.choice(d['values'])}
    if k == 'location': return a | {'type': 'location', 'key': rnd.choice(d['locations'])['key']}
    if k == 'exchange': return a | {'type': 'exchange', 'hand': rnd.choice(d['hand']), 'reserve': rnd.choice(d['reserve'])}
    if k == 'cleanup': return a | {'type': 'cleanup', 'reserve': rnd.sample(d['reserve']['cardIds'], d['reserve']['n']),
                                   'landmarks': rnd.sample(d['landmarks']['cardIds'], d['landmarks']['n'])}
    if k == 'buttons' and d['buttons']: return a | {'type': 'button', 'key': d['buttons'][0]['key']}
    return a | {'type': 'pass'}


def translate(ans, pd, rd):
    """PHP answer -> Rust answer (same card ids; choice ids and keys differ)"""
    out = dict(ans, decisionId=rd['id'])
    t = ans['type']
    if t == 'choice':
        label = next((c['label'] for c in pd['choices'] if c['id'] == ans['id']), '')
        if ans['id'] == 99 or label.lower() in ('pass', 'skip'):
            skip = [c for c in rd.get('choices', []) if c['label'] == 'Skip']
            return {'decisionId': rd['id'], 'type': 'choice', 'id': skip[0]['id']} if skip else {'decisionId': rd['id'], 'type': 'pass'}
        # same source card (triggered abilities), else same position among the choices
        src = next((c.get('sourceCardId') for c in pd['choices'] if c['id'] == ans['id']), None)
        same = [c for c in rd.get('choices', []) if src is not None and c.get('sourceCardId') == src]
        if same:
            out['id'] = same[0]['id']
            return out
        idx = [c['id'] for c in pd['choices']].index(ans['id'])
        ch = rd.get('choices', [])
        if idx < len(ch): out['id'] = ch[idx]['id']
    elif t == 'expedition':
        m = {e['key']: (e['player'], e['side']) for e in pd['expeditions']}
        rk = {(e['player'], e['side']): e['key'] for e in rd.get('expeditions', [])}
        out['keys'] = [rk.get(m[k], k) for k in ans['keys']]
    elif t == 'location':
        lab = next(l for l in pd['locations'] if l['key'] == ans['key'])
        cand = [l for l in rd.get('locations', []) if l.get('player') == lab.get('player') and l.get('zone') == lab.get('zone')]
        if cand: out['key'] = cand[0]['key']
    elif t == 'pass' and not rd.get('canPass'):
        skip = [c for c in rd.get('choices', []) if c['label'] == 'Skip']
        if skip: return {'decisionId': rd['id'], 'type': 'choice', 'id': skip[0]['id']}
    return out


def play_game(n, decks):
    d0, d1 = rnd.choice(decks)['id'], rnd.choice(decks)['id']
    g = call(opt.php, 'POST', '/api/v1/games', {'players': [{'name': 'A', 'deck': d0}, {'name': 'B', 'deck': d1}]})
    pid = g['gameId']
    snap = call(opt.php, 'GET', f'/api/oracle/games/{pid}/state')
    r = call(opt.rust, 'POST', '/api/debug/games', snap)
    if 'error' in r:
        return f'cannot load snapshot in rust: {r["error"]}', 0
    rid = r['gameId']
    tokens = {p['id']: p.get('token', '') for p in r['players']}
    rq = lambda player: f'player={player}&token={tokens[player]}'
    history = []
    for step in range(opt.max):
        ps = call(opt.php, 'GET', f'/api/oracle/games/{pid}/state')
        rs = call(opt.rust, 'GET', f'/api/debug/games/{rid}/state')
        problems = diff(ps, rs)
        if problems:
            return 'state differs after ' + (history[-1] if history else 'setup') + ':\n      ' + '\n      '.join(problems[:6]), step
        pv = call(opt.php, 'GET', f'/api/v1/games/{pid}?player=2001')
        if pv.get('phase') == 'ended':
            rv = call(opt.rust, 'GET', f'/api/v1/games/{rid}?{rq(2001)}')
            if rv.get('winner') != pv.get('winner'):
                return f'winner differs: php {pv.get("winner")} rust {rv.get("winner")}', step
            return None, step
        waiting = pv['waitingFor']
        if not waiting:
            return 'php waits for nobody', step
        player = waiting[0]
        pv = call(opt.php, 'GET', f'/api/v1/games/{pid}?player={player}')
        pd = pv['decision']
        if pd is None:
            return f'php has no decision for {player}', step
        ans = pick_answer(pd)
        if pd['kind'] == 'confirm':
            call(opt.php, 'POST', f'/api/v1/games/{pid}/answer?player={player}', ans)
            continue
        rv = call(opt.rust, 'GET', f'/api/v1/games/{rid}?{rq(player)}')
        rd = rv.get('decision')
        if rd is None:
            extra = [c['label'] for c in pd.get('choices', [])]
            return f'rust has no decision for {player}; php asks "{pd["prompt"]}" ({pd["kind"]}) {extra}, rust waits for {rv.get("waitingFor")} after {history[-1] if history else ""}', step
        if rd['kind'] != pd['kind']:
            return f'decision kind differs: php {pd["kind"]} "{pd["prompt"]}" {[c["label"] for c in pd.get("choices", [])]} / rust {rd["kind"]} "{rd["prompt"]}" after {history[-1] if history else ""}', step
        if pd['kind'] == 'turn':
            pp = sorted((p['cardId'], tuple(sorted(p['destinations']))) for p in pd['play'])
            rp = sorted((p['cardId'], tuple(sorted(p['destinations']))) for p in rd['play'])
            if pp != rp:
                return f'playable cards differ: php {pp} / rust {rp}', step
        if pd['kind'] == 'select' and sorted(pd['cardIds']) != sorted(rd['cardIds']):
            return f'selectable cards differ for "{pd["prompt"]}": php {sorted(pd["cardIds"])} / rust {sorted(rd["cardIds"])}', step
        pres = call(opt.php, 'POST', f'/api/v1/games/{pid}/answer?player={player}', ans)
        if 'error' in pres:
            if opt.v: print('   php refused', ans, pres['error'])
            continue
        rans = translate(ans, pd, rd)
        rres = call(opt.rust, 'POST', f'/api/v1/games/{rid}/answer?{rq(player)}', rans)
        refs = {c['id']: c['ref'] for c in ps['cards']}
        shown = {k: v for k, v in ans.items() if k != 'decisionId'}
        if 'cardId' in shown: shown['card'] = refs.get(shown['cardId'])
        if 'cardIds' in shown: shown['cards'] = [refs.get(i) for i in shown['cardIds']]
        desc = f'{player} {pd["kind"]} "{pd["prompt"][:70]}" -> {json.dumps(shown)}'
        history.append(desc)
        if opt.v: print('  ', desc)
        if 'error' in rres:
            return f'rust refused {rans}: {rres["error"]} (php accepted) at {desc}\n      php choices {[(c["id"], c["label"], c.get("sourceCardId")) for c in pd.get("choices", [])]}\n      rust choices {[(c["id"], c["label"], c.get("sourceCardId")) for c in rd.get("choices", [])]}', step
    return f'not finished after {opt.max} decisions', opt.max


decks = call(opt.php, 'GET', '/api/v1/decks')
results = []
for n in range(opt.games):
    problem, steps = play_game(n, decks)
    results.append(problem)
    print(f'game {n + 1}: {"identical until the end" if problem is None else "DIVERGES"} ({steps} decisions)' + ('' if problem is None else '\n    ' + problem), flush=True)
same = sum(1 for r in results if r is None)
print(f'\n{same}/{len(results)} games identical from start to end')
