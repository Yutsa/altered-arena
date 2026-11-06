import { useEffect, useRef, useState } from 'react';
import type { GameView, PlayerView, Side } from '@protocol';

/** "Dusk" alone on the board, before anything moves */
const DUSK_MS = 1300;
/** After each line of expedition tokens has moved (the tokens slide for 0.45 s) */
const MOVE_MS = 700;
/** Night: the expeditions sent to the Reserve or discarded, before the next morning */
const NIGHT_MS = 1300;

const DAYTIME = new Set(['morning', 'noon', 'afternoon', 'dusk']);
const EXPEDITIONS = new Set(['heroExpedition', 'companionExpedition']);

/** A step of the end of the day: its view, and how long it stays before the next one. The steps
 * keep the phase `dusk` until the Night, so the time is the step's own, not the phase's. */
export interface DuskStep {
  view: GameView;
  ms: number;
}

/** The view `next` reached from `prev` went through Dusk (a new day, the Night, or the game's end) */
export function crossesDusk(prev: GameView, next: GameView): boolean {
  if (next.gameId !== prev.gameId || next.you !== prev.you || next.seq <= prev.seq) return false;
  if (!DAYTIME.has(prev.phase)) return false;
  return next.day > prev.day || next.phase === 'night' || next.phase === 'tiebreaker' || next.phase === 'ended';
}

/** The players of `base` whose `side` expedition is where it is in `next` (its forecast arrow gone) */
function moved(base: PlayerView[], next: GameView, side: Side): PlayerView[] {
  return base.map((p) => {
    const n = next.players.find((x) => x.id === p.id);
    if (!n) return p;
    const e = n.expeditions[side];
    return {
      ...p,
      expeditions: {
        ...p.expeditions,
        [side]: { ...p.expeditions[side], position: e.position, blocked: e.blocked, ascended: e.ascended, advances: false },
      },
    };
  });
}

/**
 * The steps from `prev` (daytime) to `next` (after Dusk), each a view of its own:
 * Dusk announced, the hero expeditions move, then the companion ones, then the Night (the
 * characters and gear of the expeditions go to the Reserve or the discard). A step that changes
 * nothing is left out. `next` itself is not in the list.
 */
export function duskSteps(prev: GameView, next: GameView): DuskStep[] {
  const steps: DuskStep[] = [];
  let seq = prev.seq;
  const push = (v: Omit<GameView, 'seq'>, ms: number) => steps.push({ view: { ...v, seq: (seq += 0.01) } as GameView, ms });
  const dusk: GameView = { ...prev, phase: 'dusk', decision: null, waitingFor: [], canUndo: false };
  push(dusk, DUSK_MS);

  const hero = moved(dusk.players, next, 'hero');
  const heroMoves = hero.some((p, i) => p.expeditions.hero.position !== dusk.players[i].expeditions.hero.position);
  // The regions reached are revealed (a Tumult card turned face up)
  if (heroMoves) push({ ...dusk, players: hero, regions: next.regions }, MOVE_MS);
  const companion = moved(hero, next, 'companion');
  const companionMoves = companion.some((p, i) => p.expeditions.companion.position !== hero[i].expeditions.companion.position);
  if (companionMoves) push({ ...dusk, players: companion, regions: next.regions }, MOVE_MS);

  // The Night, when `next` is already the next day: the cards of the expeditions go where they
  // are in `next` (tokens vanish); the rest stays as it was (the morning draws come after)
  if (next.day > prev.day && next.phase !== 'night') {
    const after = new Map(next.cards.map((c) => [c.id, c]));
    const cards = prev.cards.flatMap((c) => {
      if (!EXPEDITIONS.has(c.zone)) return [c];
      const n = after.get(c.id);
      return n ? [n] : [];
    });
    const players = companion.map((p) => ({ ...p, discardCount: next.players.find((x) => x.id === p.id)?.discardCount ?? p.discardCount }));
    push({ ...dusk, phase: 'night', players, regions: next.regions, cards }, NIGHT_MS);
  }
  return steps;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * The end of the day is drawn in steps (see `duskSteps`), each held a while, so that a player
 * sees what happens one thing after the other: the views that arrive meanwhile wait their turn.
 * An undo or another game shows the new view at once.
 */
export function useDuskStage(latest: GameView | null): GameView | null {
  const [view, setView] = useState<GameView | null>(latest);
  const queue = useRef<GameView[]>([]);
  const running = useRef(false);
  const shown = useRef<GameView | null>(null);
  const alive = useRef(true);
  /** Bumped to abandon the steps under way */
  const run = useRef(0);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    const show = (v: GameView | null) => {
      shown.current = v;
      setView(v);
    };
    const cur = shown.current;
    const last = queue.current.at(-1) ?? cur;
    if (!latest || !cur || !last || latest.gameId !== last.gameId || latest.you !== last.you || latest.seq < last.seq) {
      run.current++;
      queue.current = [];
      running.current = false;
      show(latest);
      return;
    }
    queue.current.push(latest);
    if (running.current) return;
    running.current = true;
    const id = ++run.current;
    void (async () => {
      while (queue.current.length && alive.current && run.current === id) {
        const v = queue.current.shift()!;
        const prev = shown.current!;
        if (crossesDusk(prev, v)) {
          const steps = duskSteps(prev, v);
          for (let i = 0; i < steps.length; i++) {
            show(steps[i].view);
            await sleep(steps[i].ms);
            if (!alive.current || run.current !== id) return;
          }
        }
        show(v);
      }
      if (run.current === id) running.current = false;
    })();
  }, [latest]);

  return view;
}
