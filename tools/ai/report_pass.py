"""Anonymised report of a tournament pass: docs/ai/passes/PASS_N.md from the raw results.

    python3 tools/ai/report_pass.py N [NOTES_FILE]

Reads private/tournament-617607/passes/PASS_N.json (aitool pass), decks.json (labels),
pairings.json (real results and standings) and, when present, passes/AGREE_N.json (agreement
with the real players' decisions, aitool agree). NOTES_FILE (markdown) is appended as the
analysis section.
"""
import json
import math
import os
import sys

DIR = 'private/tournament-617607'
n = int(sys.argv[1])
p = json.load(open(f'{DIR}/passes/PASS_{n}.json'))
decks = json.load(open(f'{DIR}/decks.json'))
real = json.load(open(f'{DIR}/pairings.json'))
label = {d: decks[d]['label'] for d in decks}


def wilson(w, t):
    if t == 0:
        return (0.0, 1.0)
    z = 1.96
    ph = w / t
    den = 1 + z * z / t
    c = ph + z * z / (2 * t)
    h = z * math.sqrt(ph * (1 - ph) / t + z * z / (4 * t * t))
    return (max(0.0, (c - h) / den), min(1.0, (c + h) / den))


def pct(x):
    return f'{100 * x:.0f}'


games = [g for pr in p['pairings'] for g in pr['games']]
errors = [g for g in games if g['error']]
lines = [f'# Pass {n}', '']
lines += [f'Agent: `{p["agent"]}`. {len(p["pairings"])} pairings x {p["games"]} games = {len(games)} games '
          f'({len(errors)} errors), {p["wallSeconds"] / 60:.1f} min on {p["threads"]} threads, '
          f'{sum(g["ms"] for g in games) / max(1, len(games)) / 1000:.2f} s of CPU per game, '
          f'{sum(g["day"] for g in games) / max(1, len(games)):.1f} days per game on average.', '']
refused = sum(sum(g['refused']) for g in games)
decisions = sum(sum(g['decisions']) for g in games)
lines.append(f'Answers refused by the engine (the next candidate is played instead): {refused} of {decisions}.')
lines.append('')

# Matchups of the real pairings
lines += ['## Pairings of the tournament', '',
          'Win rate of deck A against deck B over the pass (95 % Wilson interval); the real game\'s winner in bold.', '',
          '| Phase | Round | Deck A | Deck B | A wins | 95 % | Real winner | Agrees |', '| --- | --- | --- | --- | --- | --- | --- | --- |']
exp_wins = {d: 0.0 for d in decks}
games_by_deck = {d: [0, 0] for d in decks}
far = []
agree_n = 0
for pr in p['pairings']:
    gs = [g for g in pr['games'] if g['winner']]
    wa = sum(1 for g in gs if g['winner'] == 'a')
    t = len(gs)
    rate = wa / t if t else 0.5
    lo, hi = wilson(wa, t)
    exp_wins[pr['a']] += rate
    exp_wins[pr['b']] += 1 - rate
    games_by_deck[pr['a']][0] += wa
    games_by_deck[pr['a']][1] += t
    games_by_deck[pr['b']][0] += t - wa
    games_by_deck[pr['b']][1] += t
    rw = pr['winner']
    fav = pr['a'] if rate > 0.5 else pr['b'] if rate < 0.5 else None
    ok = fav == rw
    agree_n += ok
    real_rate = rate if rw == pr['a'] else 1 - rate
    if real_rate < 0.35:
        far.append((real_rate, pr))
    rnd = f'G{pr["group"]} R{pr["round"]}' if pr['phase'] == 'swiss' else f'E{pr["round"]}'
    a = f'**{label[pr["a"]]}**' if rw == pr['a'] else label[pr['a']]
    b = f'**{label[pr["b"]]}**' if rw == pr['b'] else label[pr['b']]
    lines.append(f'| {pr["phase"]} | {rnd} | {a} | {b} | {pct(rate)} % | {pct(lo)}-{pct(hi)} | {label.get(rw, "?")} | {"yes" if ok else "no"} |')
lines.append('')
lines.append(f'The pass\'s favourite won the real game in {agree_n} of {len(p["pairings"])} pairings ({pct(agree_n / len(p["pairings"]))} %).')
lines.append('')

# Standings
st = {s['deck']: s for s in real['standings']}
real_wins = {d: 0 for d in decks}
for pr in real['pairings']:
    if pr['winner']:
        real_wins[pr['winner']] += 1
sim_order = sorted(decks, key=lambda d: (-exp_wins[d], d))
sim_rank = {d: i + 1 for i, d in enumerate(sim_order)}
real_order = sorted(decks, key=lambda d: (st[d]['rank'], d))
real_pos = {d: i + 1 for i, d in enumerate(real_order)}
nd = len(decks)
rho = 1 - 6 * sum((sim_rank[d] - real_pos[d]) ** 2 for d in decks) / (nd * (nd * nd - 1))
lines += ['## Standings', '',
          'Expected wins: the sum, over the deck\'s real pairings, of its win rate in the pass. '
          'Simulated rank: by expected wins. Real rank: elimination result for the top 8, Swiss record after.', '',
          f'Spearman correlation between the simulated and real orders: {rho:.2f}.', '',
          '| Real rank | Deck | Real wins | Expected wins | Simulated rank | Win rate in the pass | 95 % |', '| --- | --- | --- | --- | --- | --- | --- |']
for d in real_order:
    w, t = games_by_deck[d]
    lo, hi = wilson(w, t)
    lines.append(f'| {st[d]["rank"]} | {label[d]} | {real_wins[d]} | {exp_wins[d]:.2f} | {sim_rank[d]} | {pct(w / t) if t else "-"} % | {pct(lo)}-{pct(hi)} |')
lines.append('')

# End of game, per deck (passes that record it)
ends = {d: [] for d in decks}
for pr in p['pairings']:
    for g in pr['games']:
        e = g.get('end') or []
        if len(e) == 2 and g['winner']:
            # end[0] is deck a's seat
            ends[pr['a']].append((e[0], g['winner'] == 'a'))
            ends[pr['b']].append((e[1], g['winner'] == 'b'))
if any(ends.values()):
    lines += ['## End of game', '', 'Averages at the end of the deck\'s games: Mana Orbs, cards in hand and in Reserve, regions covered (progress), in its losses.', '',
              '| Deck | Days | Mana | Hand | Reserve | Progress | Progress in losses |', '| --- | --- | --- | --- | --- | --- | --- |']
    days = {d: [] for d in decks}
    for pr in p['pairings']:
        for g in pr['games']:
            days[pr['a']].append(g['day'])
            days[pr['b']].append(g['day'])
    avg = lambda xs: sum(xs) / len(xs) if xs else 0
    for d in sorted(decks):
        e = ends[d]
        if not e:
            continue
        lost = [x for x, w in e if not w]
        lines.append(f'| {label[d]} | {avg(days[d]):.1f} | {avg([x["mana"] for x, _ in e]):.1f} | {avg([x["hand"] for x, _ in e]):.1f} | '
                     f'{avg([x["reserve"] for x, _ in e]):.1f} | {avg([x["progress"] for x, _ in e]):.1f} | {avg([x["progress"] for x in lost]):.1f} |')
    lines.append('')

if far:
    lines += ['## Pairings far from the real result', '', 'The real winner won less than 35 % of the pass\'s games:', '']
    for rate, pr in sorted(far, key=lambda x: x[0]):
        loser = pr['b'] if pr['winner'] == pr['a'] else pr['a']
        lines.append(f'- {label[pr["winner"]]} beat {label[loser]} in the tournament, {pct(rate)} % here')
    lines.append('')

agree_file = f'{DIR}/passes/AGREE_{n}.json'
if os.path.exists(agree_file):
    ag = json.load(open(agree_file))
    lines += ['## Agreement with the real players', '',
              f'Share of the real players\' decisions (tournament replays, both players) where the AI picks the same answer: '
              f'**{pct(ag["overall"]["same"] / max(1, ag["overall"]["n"]))} %** of {ag["overall"]["n"]} decisions '
              f'(simple bot: {pct(ag["simple"]["same"] / max(1, ag["simple"]["n"]))} %; only decisions with at least two answers count).', '',
              '| Decision | Decisions | AI | Simple bot |', '| --- | --- | --- | --- |']
    for k, v in sorted(ag['byKind'].items(), key=lambda kv: -kv[1]['n']):
        s = ag['simpleByKind'].get(k, {'same': 0, 'n': 0})
        lines.append(f'| {k} | {v["n"]} | {pct(v["same"] / max(1, v["n"]))} % | {pct(s["same"] / max(1, s["n"]))} % |')
    lines.append('')

if errors:
    lines += ['## Errors', '']
    kinds = {}
    for g in errors:
        kinds.setdefault(g['error'][:80], []).append(g['seed'])
    for k, seeds in kinds.items():
        lines.append(f'- {len(seeds)} x `{k}` (seeds {", ".join(map(str, seeds[:5]))})')
    lines.append('')

if len(sys.argv) > 2 and os.path.exists(sys.argv[2]):
    lines += ['## Analysis', '', open(sys.argv[2]).read().strip(), '']

os.makedirs('docs/ai/passes', exist_ok=True)
open(f'docs/ai/passes/PASS_{n}.md', 'w').write('\n'.join(lines))
summary = {'pass': n, 'agent': p['agent'], 'games': len(games), 'errors': len(errors), 'favouriteAgrees': agree_n,
           'pairings': len(p['pairings']), 'spearman': rho, 'minutes': p['wallSeconds'] / 60,
           'winRate': {d: games_by_deck[d][0] / max(1, games_by_deck[d][1]) for d in decks}, 'expectedWins': exp_wins}
json.dump(summary, open(f'{DIR}/passes/SUMMARY_{n}.json', 'w'), indent=1)
print(f'docs/ai/passes/PASS_{n}.md: favourite agrees {agree_n}/{len(p["pairings"])}, spearman {rho:.2f}, {len(errors)} errors')
