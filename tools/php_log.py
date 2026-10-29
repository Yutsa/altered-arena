#!/usr/bin/env python3
"""Full log of a game of the PHP oracle: tools/php_log.py GAME_ID [LAST_N] [PLAYER]"""
import json, sys, urllib.request, re
g = sys.argv[1]
n = int(sys.argv[2]) if len(sys.argv) > 2 else 60
player = sys.argv[3] if len(sys.argv) > 3 else '2001'
notifs = json.load(urllib.request.urlopen(f'http://localhost:8080/api/games/{g}/notifs?player={player}&after=0'))['notifications']


def fmt(log, args, depth=0):
    if depth > 6:
        return log
    def sub(m):
        k = m.group(1)
        v = args.get(k, '')
        if isinstance(v, dict):
            return fmt(str(v.get('log', '')), v.get('args') or {}, depth + 1) if 'log' in v else ''
        if isinstance(v, list):
            return ', '.join(str(x) for x in v if not isinstance(x, (dict, list)))
        if k in ('card_name', 'card_name2', 'source', 'cardName', 'eventCardName'):
            return f'[[{v}]]'
        return str(v)
    return re.sub(r'\$\{([a-zA-Z0-9_]+)\}', sub, log)


lines = [f"{x['id']:5} {fmt(x['msg'], dict(x['args'] or {}, you='You'))}" for x in notifs if x.get('msg')]
print('\n'.join(lines[-n:]))
