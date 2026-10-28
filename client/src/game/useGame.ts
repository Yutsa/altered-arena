import { useCallback, useEffect, useRef, useState } from 'react';
import type { Answer, GameView, LogEntry, PlayerId } from '@protocol';
import { api, ApiError } from '../api/client';
import { toast } from './toasts';

import type { AnswerBody } from './decisions';
export type { AnswerBody };

export interface GameState {
  view: GameView | null;
  /** Log entries accumulated since the page was opened (the view only carries the last 100) */
  log: LogEntry[];
  sending: boolean;
  connected: boolean;
  fatal: string | null;
  send: (a: AnswerBody) => Promise<boolean>;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Engine bookkeeping lines that mean nothing to a player ("Undo here" checkpoints, empty "[]") */
const NOISE = /^\s*(\[\]+\]?|Undo here)?\s*$/;

function mergeLog(prev: LogEntry[], next: LogEntry[], lastSeq: number): LogEntry[] {
  const fresh = next.filter((e) => e.seq > lastSeq && !NOISE.test(e.text));
  if (!fresh.length) return prev;
  return [...prev, ...fresh].slice(-500);
}

/**
 * Keeps the GameView of one player up to date: initial fetch, then long polling with `since`,
 * and answers. The most recent view (highest seq) always wins.
 */
export function useGame(gameId: string, player: PlayerId | null, token: string | null): GameState {
  const [view, setView] = useState<GameView | null>(null);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [sending, setSending] = useState(false);
  const [connected, setConnected] = useState(true);
  const [fatal, setFatal] = useState<string | null>(null);
  const viewRef = useRef<GameView | null>(null);

  const accept = useCallback((v: GameView) => {
    const cur = viewRef.current;
    if (cur && v.seq < cur.seq) return;
    if (cur && v.seq === cur.seq && v.decision?.id === cur.decision?.id) return;
    const lastSeq = cur ? Math.max(-1, ...cur.log.map((e) => e.seq)) : -1;
    viewRef.current = v;
    setView(v);
    setLog((l) => mergeLog(l, v.log, lastSeq));
  }, []);

  useEffect(() => {
    viewRef.current = null;
    setView(null);
    setLog([]);
    setFatal(null);
    let stopped = false;
    const ctrl = new AbortController();
    (async () => {
      let failures = 0;
      while (!stopped) {
        const cur = viewRef.current;
        if (cur?.phase === 'ended') break;
        try {
          const v = await api.view(gameId, player, token, cur ? cur.seq : undefined, ctrl.signal);
          if (stopped) break;
          failures = 0;
          setConnected(true);
          accept(v);
        } catch (e) {
          if (stopped || (e as Error).name === 'AbortError') break;
          if (e instanceof ApiError && e.user && !viewRef.current) {
            setFatal(e.message);
            break;
          }
          failures++;
          setConnected(false);
          await sleep(Math.min(1000 * 2 ** failures, 10000));
        }
      }
    })();
    return () => {
      stopped = true;
      ctrl.abort();
    };
  }, [gameId, player, token, accept]);

  const send = useCallback(
    async (a: AnswerBody) => {
      const d = viewRef.current?.decision;
      if (!d) return false;
      setSending(true);
      try {
        const v = await api.answer(gameId, player, token, { ...a, decisionId: d.id } as Answer);
        accept(v);
        return true;
      } catch (e) {
        const err = e as ApiError;
        toast(err.user ? err.message : `Server error: ${err.message}`);
        // The decision may be stale: refresh the view without waiting for the poll
        api.view(gameId, player, token).then(accept, () => {});
        return false;
      } finally {
        setSending(false);
      }
    },
    [gameId, player, token, accept],
  );

  return { view, log, sending, connected, fatal, send };
}
