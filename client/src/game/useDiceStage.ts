import { useEffect, useRef, useState } from 'react';
import type { GameView, LogEntry, PlayerId } from '@protocol';

/** The dice tumble before showing their faces */
const ROLL_MS = 1200;
/** The faces stay on screen, long enough to read them, before the effect plays out */
const RESULT_MS = 1800;

export interface DiceRoll {
  /** The log entry of the roll */
  seq: number;
  player: PlayerId | null;
  values: number[];
  /** false while the dice tumble, true once they show their result */
  settled: boolean;
}

export interface DiceStage {
  view: GameView | null;
  roll: DiceRoll | null;
}

const ROLL = /\brolls ([1-6](?:, [1-6])*)$/;

/** The dice rolled in a log entry ("X rolls 4", "X rolls 2, 5"), or null */
export function rollOf(e: LogEntry): number[] | null {
  const m = ROLL.exec(e.text);
  return m ? m[1].split(', ').map(Number) : null;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/**
 * Die rolls are shown before their result: the server rolls and applies the effect in one answer,
 * so a new view that carries a roll not seen yet is held back. The previous view stays on screen
 * (without its decision) while the dice tumble, then show their faces, then the new view is drawn.
 * The views that arrive meanwhile wait their turn.
 */
export function useDiceStage(latest: GameView | null): DiceStage {
  const [stage, setStage] = useState<DiceStage>({ view: latest, roll: null });
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

  // Another game or another seat: the rolls already made are not shown again
  useEffect(() => {
    seen.current = null;
    queue.current = [];
  }, [latest?.gameId, latest?.you]);

  useEffect(() => {
    if (!latest) {
      shown.current = null;
      setStage({ view: null, roll: null });
      return;
    }
    if (!seen.current || !shown.current) {
      seen.current = new Set(latest.log.map((e) => e.seq));
      shown.current = latest;
      setStage({ view: latest, roll: null });
      return;
    }
    queue.current.push(latest);
    if (running.current) return;
    running.current = true;
    const show = (view: GameView, roll: DiceRoll | null) => {
      shown.current = view;
      setStage({ view, roll });
    };
    void (async () => {
      while (queue.current.length && alive.current) {
        const v = queue.current.shift()!;
        const fresh = v.log.filter((e) => !seen.current!.has(e.seq));
        for (const e of v.log) seen.current!.add(e.seq);
        for (const e of fresh) {
          const values = rollOf(e);
          if (!values) continue;
          // An earlier view, frozen: nothing to decide while the dice roll
          const base = { ...shown.current!, decision: null, waitingFor: [] };
          const roll = { seq: e.seq, player: e.player ?? null, values };
          show(base, { ...roll, settled: false });
          await sleep(ROLL_MS);
          if (!alive.current) return;
          show(base, { ...roll, settled: true });
          await sleep(RESULT_MS);
          if (!alive.current) return;
        }
        if (alive.current) show(v, null);
      }
      running.current = false;
    })();
  }, [latest]);

  return stage;
}
