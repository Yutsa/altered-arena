import { useEffect, useRef, useState } from 'react';
import type { CastView, CardView, EffectView, GameView, PlayerId } from '@protocol';

/** The spell alone beside the mat, before its targets */
const CAST_MS = 900;
/** The arrows to its targets, before its result */
const TARGET_MS = 1300;
/** A spell without targets stays a while beside the mat */
const ALONE_MS = 1300;
/** The player's own effect, with its targets at once (the player knows it, but did not see it) */
const MINE_MS = 1000;

export interface CastStage {
  /** The view to draw: the latest one, or a step staged before it */
  view: GameView | null;
  /** The targets of the spell staged now (arrows from the spell beside the mat) */
  targets: { cards: number[]; keys: string[] } | null;
  /** The card whose effect is staged now (a copy beside the mat, the card stays at its place) */
  effect: CardView | null;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** `base` with the spell in limbo (from the opponent's hand, or from where it was), without a decision */
function withSpell(base: GameView, c: CastView, latest: GameView, seq: number): GameView {
  const now = latest.cards.find((x) => x.id === c.cardId);
  const was = base.cards.find((x) => x.id === c.cardId);
  const card: CardView = {
    ...(now ?? { id: c.cardId, owner: c.player, position: 0, boosts: 0, counters: {}, statuses: [] }),
    ...(was ?? {}),
    ref: c.ref,
    unique: c.unique,
    zone: 'limbo',
    exhausted: false,
  };
  // A spell already shown (several in one view) leaves limbo: the next one takes its place
  const cards = base.cards.filter((x) => x.id !== c.cardId && x.zone !== 'limbo');
  cards.push(card);
  const players = was ? base.players : base.players.map((p) => (p.id === c.player ? { ...p, handCount: Math.max(0, p.handCount - 1) } : p));
  return { ...base, seq, cards, players, decision: null };
}

/** The source card of an effect: as it is on the table before the effect, or after it (a card
 * just played), or built from the effect (a card no longer visible) */
function sourceCard(e: EffectView, base: GameView, latest: GameView): CardView {
  const c = base.cards.find((x) => x.id === e.cardId && x.ref) ?? latest.cards.find((x) => x.id === e.cardId && x.ref);
  return c ?? { id: e.cardId, owner: e.player, zone: 'limbo', position: 0, exhausted: false, boosts: 0, counters: {}, statuses: [], ref: e.ref, unique: e.unique };
}

/**
 * The effects of the other cards (a character's, a permanent's, a hero's, a reaction...) are
 * staged the same way, whoever the player: the card beside the mat (a copy: the card stays at its
 * place), then the arrows to its targets, then the result. The player's own effects are shown
 * once, with their targets at once.
 *
 * The spells of the other players are shown before their result: a new view that carries a
 * resolved spell not seen yet is drawn in steps. First the previous view with the spell beside the
 * mat (it flies there from the hand or the Reserve), then the arrows to its targets, then the new
 * view (the effect, the spell to the Reserve or the discard). The views that arrive meanwhile wait
 * their turn. The player's own spells are not staged: they saw them.
 */
export function useCastStage(latest: GameView | null, you: PlayerId | null): CastStage {
  const [stage, setStage] = useState<CastStage>({ view: latest, targets: null, effect: null });
  const queue = useRef<GameView[]>([]);
  const running = useRef(false);
  const shown = useRef<GameView | null>(null);
  const seen = useRef<Set<number> | null>(null);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // Another game or another seat: the spells already cast are not shown again
  useEffect(() => {
    seen.current = null;
    queue.current = [];
  }, [latest?.gameId, you]);

  useEffect(() => {
    if (!latest) {
      shown.current = null;
      setStage({ view: null, targets: null, effect: null });
      return;
    }
    if (!seen.current || !shown.current) {
      seen.current = new Set([...(latest.casts ?? []).filter((c) => c.resolved).map((c) => c.seq), ...(latest.effects ?? []).map((e) => e.seq)]);
      shown.current = latest;
      setStage({ view: latest, targets: null, effect: null });
      return;
    }
    queue.current.push(latest);
    if (running.current) return;
    running.current = true;
    const show = (view: GameView, targets: CastStage['targets'], effect: CardView | null = null) => {
      shown.current = view;
      setStage({ view, targets, effect });
    };
    void (async () => {
      while (queue.current.length && alive.current) {
        const v = queue.current.shift()!;
        const casts = (v.casts ?? []).filter((c) => c.resolved && !seen.current!.has(c.seq) && c.player !== you);
        const effects = (v.effects ?? []).filter((e) => !seen.current!.has(e.seq));
        for (const c of v.casts ?? []) if (c.resolved) seen.current!.add(c.seq);
        for (const e of v.effects ?? []) seen.current!.add(e.seq);
        type Step = { seq: number; cast?: CastView; effect?: EffectView };
        const steps: Step[] = [...casts.map((c) => ({ seq: c.seq, cast: c })), ...effects.map((e) => ({ seq: e.seq, effect: e }))].sort((a, b) => a.seq - b.seq);
        for (const step of steps) {
          const base = shown.current!;
          if (step.effect) {
            const e = step.effect;
            const still = { ...base, seq: base.seq + 0.1, decision: null };
            const card = sourceCard(e, base, v);
            const targets = e.targets.filter((id) => base.cards.some((x) => x.id === id));
            const arrows = targets.length ? { cards: targets, keys: [] } : null;
            if (e.player === you) {
              show(still, arrows, card);
              await sleep(MINE_MS);
            } else {
              show(still, null, card);
              await sleep(arrows ? CAST_MS : ALONE_MS);
              if (arrows && alive.current) {
                show({ ...still, seq: still.seq + 0.1 }, arrows, card);
                await sleep(TARGET_MS);
              }
            }
            if (!alive.current) return;
            continue;
          }
          const c = step.cast!;
          const inLimbo = base.cards.some((x) => x.id === c.cardId && x.zone === 'limbo');
          let seq = base.seq + 0.1;
          const spell = withSpell(base, c, v, seq);
          const targets = c.targets.filter((id) => spell.cards.some((x) => x.id === id));
          if (!inLimbo) {
            show(spell, null);
            await sleep(targets.length ? CAST_MS : ALONE_MS);
            seq += 0.1;
          }
          if (targets.length && alive.current) {
            show({ ...spell, seq }, { cards: targets, keys: [] });
            await sleep(TARGET_MS);
          }
          if (!alive.current) return;
        }
        if (alive.current) show(v, null);
      }
      running.current = false;
    })();
  }, [latest, you]);

  return stage;
}
