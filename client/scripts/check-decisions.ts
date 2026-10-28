/**
 * Plays whole games against the server through the client's decision logic (src/game/decisions.ts):
 * every answer is built the way the UI builds it (picking cards with togglePick, confirmAnswer...).
 * Checks that every decision kind has a renderer, that answers are accepted, that every card a
 * decision refers to is known, and reports markup the client does not know.
 *
 *   node scripts/check-decisions.ts [--games N] [--hotseat] [--record out.json] [--server URL]
 */
import type { CardCatalog, Decision, GameCreated, GameView, PlayerId } from '../../protocol/protocol.ts';
import { affordances, confirmAnswer, emptyUI, isOnBoard, pickerCardIds, toggleKey, togglePick, type UIState, type AnswerBody } from '../src/game/decisions.ts';
import { KEYWORDS } from '../src/game/keywords.ts';
import { writeFileSync } from 'node:fs';

const args = process.argv.slice(2);
const opt = (name: string, def?: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : def;
};
const BASE = opt('--server', 'http://localhost:8080')!;
const GAMES = Number(opt('--games', '3'));
const HOTSEAT = args.includes('--hotseat');
const RECORD = opt('--record');

/** Decision kinds DecisionBar renders natively (keep in sync with components/DecisionBar.tsx) */
const RENDERED = new Set(['turn', 'select', 'choice', 'confirm', 'expedition', 'player', 'number', 'location', 'exchange', 'cleanup', 'deck', 'buttons']);

async function call<T>(method: string, path: string, body?: unknown): Promise<T & { error?: string; user?: boolean }> {
  const r = await fetch(BASE + path, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  return (await r.json()) as T & { error?: string };
}

const rnd = <T>(l: T[]): T => l[Math.floor(Math.random() * l.length)];
const shuffle = <T>(l: T[]): T[] => [...l].sort(() => Math.random() - 0.5);

const stats = {
  kinds: {} as Record<string, number>,
  refused: [] as string[],
  unknownCards: new Set<string>(),
  unknownMarkup: new Set<string>(),
  picker: 0,
  missingCost: 0,
  winners: [] as (PlayerId | null)[],
};

function checkMarkup(text: string, where: string) {
  for (const m of text.matchAll(/<([A-Z0-9_]+)(?::[^>]+)?>/g)) if (!KEYWORDS[m[1]] && m[1] !== 'BR') stats.unknownMarkup.add(`${m[1]} (${where})`);
}

/** The answer the UI would send for this decision, built with the UI's own helpers. */
function uiAnswer(view: GameView, d: Decision, catalog: CardCatalog): AnswerBody {
  let ui: UIState = emptyUI(d.id);
  const aff = affordances(d, ui);
  const byId = new Map(view.cards.map((c) => [c.id, c]));
  const known = (id: number, what: string) => {
    const c = byId.get(id);
    if (!c) stats.unknownCards.add(`${d.kind}/${what}: card ${id} not in view.cards (${d.prompt})`);
    else if (c.ref && !catalog[c.ref]) stats.unknownCards.add(`${d.kind}/${what}: ref ${c.ref} not in catalog`);
  };
  if (d.sourceCardId !== undefined) known(d.sourceCardId, 'sourceCardId');
  checkMarkup(d.prompt, `prompt of ${d.kind}`);
  switch (d.kind) {
    case 'turn': {
      d.play.forEach((p) => known(p.cardId, 'play'));
      if (d.play.some((p) => p.cost === undefined)) stats.missingCost++;
      const actions: AnswerBody[] = [];
      for (const [id, a] of aff) {
        for (const dest of a.destinations ?? []) actions.push({ type: 'play', cardId: id, destination: dest });
        if (a.support) actions.push({ type: 'support', cardId: id });
        if (a.tap) actions.push({ type: 'tap', cardId: id });
      }
      if (actions.length && Math.random() < 0.85) return rnd(actions);
      if (d.canEndTurn) return { type: 'endTurn' };
      return { type: 'pass' };
    }
    case 'select': {
      d.cardIds.forEach((id) => known(id, 'cardIds'));
      if (pickerCardIds(view, d).length) stats.picker++;
      for (const id of d.cardIds) if (!isOnBoard(byId.get(id), view.you) && !d.showInPicker) void 0;
      const n = d.min + Math.floor(Math.random() * (d.max - d.min + 1));
      for (const id of shuffle(d.cardIds).slice(0, n)) ui = togglePick(d, ui, id);
      const a = confirmAnswer(d, ui);
      if (!a) throw new Error(`select: no valid answer after picking ${n} (${d.min}..${d.max})`);
      if (a.type === 'select' && a.cardIds.length === 0 && d.canPass) return { type: 'pass' };
      return a;
    }
    case 'cleanup': {
      for (const id of shuffle([...d.reserve.cardIds, ...d.landmarks.cardIds])) ui = togglePick(d, ui, id);
      const a = confirmAnswer(d, ui);
      if (!a) throw new Error('cleanup: no valid answer');
      return a;
    }
    case 'exchange': {
      if (!d.hand.length || !d.reserve.length) return { type: 'pass' };
      ui = togglePick(d, ui, rnd(d.hand));
      ui = togglePick(d, ui, rnd(d.reserve));
      return confirmAnswer(d, ui) ?? { type: 'pass' };
    }
    case 'expedition': {
      for (const o of shuffle(d.expeditions).slice(0, Math.max(1, d.min))) ui = toggleKey(d, ui, o.key);
      return confirmAnswer(d, ui) ?? { type: 'pass' };
    }
    case 'confirm':
      return confirmAnswer(d, ui)!;
    case 'choice': {
      d.choices.forEach((c) => {
        checkMarkup(c.label, 'choice label');
        if (c.sourceCardId !== undefined) known(c.sourceCardId, 'choice.sourceCardId');
      });
      const en = d.choices.filter((c) => c.enabled);
      return en.length ? { type: 'choice', id: rnd(en).id } : { type: 'pass' };
    }
    case 'player':
      return { type: 'player', player: rnd(d.players) };
    case 'number':
      return { type: 'number', value: rnd(d.values) };
    case 'location':
      return { type: 'location', key: rnd(d.locations).key };
    case 'deck':
      return { type: 'deck', deck: rnd(d.decks).id };
    case 'buttons':
      return d.buttons.length ? { type: 'button', key: rnd(d.buttons).key } : { type: 'pass' };
  }
}

async function playGame(catalog: CardCatalog, decks: { id: string }[]) {
  const players = HOTSEAT
    ? [{ name: 'Alice' }, { name: 'Bob' }]
    : [{ name: 'Check', deck: rnd(decks).id }, { name: 'Bot', bot: true }];
  const g = await call<GameCreated>('POST', '/api/v1/games', { players });
  const humans = g.players.filter((p) => !p.bot).map((p) => p.id);
  const tokens = new Map(g.players.map((p) => [p.id, p.token]));
  const seat = (p: PlayerId) => `player=${p}${tokens.get(p) ? `&token=${encodeURIComponent(tokens.get(p)!)}` : ''}`;
  const recorded: GameView[] = [];
  let steps = 0;
  for (;;) {
    let acted = false;
    let ended: GameView | null = null;
    for (const me of humans) {
      const view = await call<GameView>('GET', `/api/v1/games/${g.gameId}?${seat(me)}`);
      if (view.error) throw new Error(view.error);
      if (me === humans[0]) recorded.push(view);
      if (view.phase === 'ended') {
        ended = view;
        break;
      }
      for (const e of view.log) checkMarkup(e.text, 'log');
      const d = view.decision;
      if (!d) continue;
      stats.kinds[d.kind] = (stats.kinds[d.kind] ?? 0) + 1;
      if (!RENDERED.has(d.kind)) throw new Error(`No renderer for decision kind ${d.kind}`);
      const body = uiAnswer(view, d, catalog);
      let res = await call<GameView>('POST', `/api/v1/games/${g.gameId}/answer?${seat(me)}`, { ...body, decisionId: d.id });
      if (res.error) {
        stats.refused.push(`${d.kind} "${d.prompt}" ${JSON.stringify(body)} -> ${res.error}`);
        if (d.canPass) res = await call<GameView>('POST', `/api/v1/games/${g.gameId}/answer?${seat(me)}`, { decisionId: d.id, type: 'pass' });
        if (res.error && d.kind === 'turn' && d.canEndTurn)
          res = await call<GameView>('POST', `/api/v1/games/${g.gameId}/answer?${seat(me)}`, { decisionId: d.id, type: 'endTurn' });
        if (res.error) throw new Error(`stuck on ${d.kind}: ${res.error}`);
      }
      acted = true;
      steps++;
    }
    if (ended) {
      stats.winners.push(ended.winner);
      break;
    }
    if (steps > 1500) throw new Error('too many steps');
    if (!acted) {
      const v = await call<GameView>('GET', `/api/v1/games/${g.gameId}?${seat(humans[0])}&since=${recorded.at(-1)!.seq}`);
      if (v.phase === 'ended') {
        recorded.push(v);
        stats.winners.push(v.winner);
        break;
      }
    }
  }
  console.log(`game ${g.gameId}: ${steps} decisions, winner ${stats.winners.at(-1)}`);
  return recorded;
}

const catalog = await call<CardCatalog>('GET', '/api/v1/cards');
const decks = await call<{ id: string }[]>('GET', '/api/v1/decks');
let all: GameView[] = [];
for (let i = 0; i < GAMES; i++) all = all.concat(await playGame(catalog, decks));
if (RECORD) writeFileSync(RECORD, JSON.stringify(all));
console.log('decision kinds:', stats.kinds);
console.log('select decisions shown in a picker:', stats.picker);
console.log('turn decisions with PlayOption.cost missing:', stats.missingCost);
console.log('refused answers:', stats.refused.length);
for (const r of stats.refused.slice(0, 20)) console.log('  ', r);
console.log('unknown cards:', [...stats.unknownCards].slice(0, 20));
console.log('unknown markup:', [...stats.unknownMarkup]);
