"""Tables of the final report from the passes' summaries: printed as markdown.

    python3 tools/ai/report_tables.py [LAST_PASS]

Reads private/tournament-617607/passes/SUMMARY_N.json (report_pass.py), AGREE_N.json,
PASS_N.json, decks.json and pairings.json. Anonymised output (deck labels only).
"""
import json
import math
import os
import sys

DIR = 'private/tournament-617607'
last = int(sys.argv[1]) if len(sys.argv) > 1 else 5
decks = json.load(open(f'{DIR}/decks.json'))
real = json.load(open(f'{DIR}/pairings.json'))
label = {d: decks[d]['label'] for d in decks}
passes = [n for n in range(0, last + 1) if os.path.exists(f'{DIR}/passes/SUMMARY_{n}.json')]
S = {n: json.load(open(f'{DIR}/passes/SUMMARY_{n}.json')) for n in passes}
A = {n: json.load(open(f'{DIR}/passes/AGREE_{n}.json')) for n in passes if os.path.exists(f'{DIR}/passes/AGREE_{n}.json')}
P = {n: json.load(open(f'{DIR}/passes/PASS_{n}.json')) for n in passes}


def pct(x):
    return f'{100 * x:.0f}'


out = []
out += ['| Pass | Agent | Games | Errors | Minutes | CPU s/game | Favourite = real winner | Spearman | Agreement |',
        '| --- | --- | --- | --- | --- | --- | --- | --- | --- |']
for n in passes:
    s = S[n]
    games = [g for pr in P[n]['pairings'] for g in pr['games']]
    cpu = sum(g['ms'] for g in games) / max(1, len(games)) / 1000
    ag = A.get(n)
    agree = f'{pct(ag["overall"]["same"] / ag["overall"]["n"])} %' if ag else '-'
    out.append(f'| {n} | {s["agent"]} | {s["games"]} | {s["errors"]} | {s["minutes"]:.0f} | {cpu:.2f} | {s["favouriteAgrees"]}/{s["pairings"]} | {s["spearman"]:.2f} | {agree} |')
out.append('')

# Per-deck win rate across passes
st = {x['deck']: x for x in real['standings']}
order = sorted(decks, key=lambda d: (st[d]['rank'], d))
out += ['| Real rank | Deck | ' + ' | '.join(f'Pass {n}' for n in passes) + ' |', '| --- | --- |' + ' --- |' * len(passes)]
for d in order:
    out.append(f'| {st[d]["rank"]} | {label[d]} | ' + ' | '.join(f'{pct(S[n]["winRate"][d])} %' for n in passes) + ' |')
out.append('')

# Final matchup table of the last pass
n = passes[-1]
out += [f'Pairings, pass {n} (A win rate):', '', '| Phase | Deck A | Deck B | Real winner | A wins |', '| --- | --- | --- | --- | --- |']
for pr in P[n]['pairings']:
    gs = [g for g in pr['games'] if g['winner']]
    wa = sum(1 for g in gs if g['winner'] == 'a')
    rnd = f'G{pr["group"]} R{pr["round"]}' if pr['phase'] == 'swiss' else f'E{pr["round"]}'
    out.append(f'| {rnd} | {label[pr["a"]]} | {label[pr["b"]]} | {label.get(pr["winner"], "?")} | {pct(wa / max(1, len(gs)))} % |')
out.append('')
print('\n'.join(out))
