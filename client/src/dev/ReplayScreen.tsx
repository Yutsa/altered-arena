import { useEffect, useMemo, useState } from 'react';
import type { GameView } from '@protocol';
import { useCatalog } from '../api/catalog';
import { InspectProvider } from '../game/inspect';
import { GameProvider } from '../game/GameContext';
import { DragProvider } from '../game/drag';
import { Table } from '../components/Table';
import { Inspector } from '../components/Inspector';
import { toast } from '../game/toasts';
import type { AnswerBody } from '../game/useGame';

/**
 * Dev page (?replay): steps through recorded GameViews (a JSON array, e.g. written by
 * scripts/check-decisions.ts --record). Answers are not sent: they are shown as a toast so that the
 * renderers of every decision kind can be checked without a server.
 */
export default function ReplayScreen() {
  const { catalog } = useCatalog();
  const [views, setViews] = useState<GameView[]>([]);
  const [i, setI] = useState(0);
  const view = views[i];
  const refs = useMemo(() => (view ? view.cards.map((c) => c.ref).filter((r): r is string => !!r) : []), [view]);
  const log = useMemo(() => views.slice(0, i + 1).flatMap((v, k) => (k === 0 ? v.log : v.log.filter((e) => e.seq > views[k - 1].log.at(-1)!.seq))), [views, i]);

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight') setI((x) => Math.min(x + 1, views.length - 1));
      if (e.key === 'ArrowLeft') setI((x) => Math.max(x - 1, 0));
    };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [views.length]);

  // ?replay=/some/file.json loads a file served by the dev server
  useEffect(() => {
    const src = new URLSearchParams(location.search).get('replay');
    if (!src) return;
    fetch(src)
      .then((r) => r.json())
      .then((data: GameView[] | GameView) => setViews((Array.isArray(data) ? data : [data]).filter((v) => v && 'seq' in v)))
      .catch((e: Error) => toast(`Cannot load ${src}: ${e.message}`));
  }, []);

  const load = async (f: File) => {
    try {
      const data = JSON.parse(await f.text()) as GameView[] | GameView;
      setViews((Array.isArray(data) ? data : [data]).filter((v) => v && 'seq' in v));
      setI(0);
    } catch (e) {
      toast(`Cannot read ${f.name}: ${(e as Error).message}`);
    }
  };
  const send = async (a: AnswerBody) => {
    toast(`Answer: ${JSON.stringify({ decisionId: view?.decision?.id, ...a })}`, 'info', 6000);
    console.info('answer', { decisionId: view?.decision?.id, ...a });
    return true;
  };

  if (!catalog || !view)
    return (
      <div className="center-screen">
        <h2>Replay</h2>
        <p className="muted">Load (or pass ?replay=/file.json) a JSON array of GameViews (scripts/check-decisions.ts --record out.json).</p>
        <input type="file" accept="application/json" onChange={(e) => e.target.files?.[0] && void load(e.target.files[0])} />
      </div>
    );
  const controls = (
    <span className="replay-controls">
      <button type="button" className="btn tiny" onClick={() => setI((x) => Math.max(0, x - 1))}>
        ◀
      </button>
      <input type="range" min={0} max={views.length - 1} value={i} onChange={(e) => setI(Number(e.target.value))} />
      <button type="button" className="btn tiny" onClick={() => setI((x) => Math.min(views.length - 1, x + 1))}>
        ▶
      </button>
      <small className="muted">
        {i + 1}/{views.length} {view.decision?.kind ?? ''}
      </small>
    </span>
  );
  return (
    <InspectProvider catalog={catalog} preferredRefs={refs}>
      <GameProvider view={view} catalog={catalog} log={log} sending={false} send={send}>
        <DragProvider>
          <Table onLeave={() => setViews([])} connected extraTop={controls} />
        </DragProvider>
      </GameProvider>
      <Inspector />
    </InspectProvider>
  );
}
