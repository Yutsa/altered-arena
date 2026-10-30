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
import './Lobby.css';

type ReplayInfo = { table: number; players: { name: string; faction: string; hero: string }[]; steps: number };
type ReplayStep = { move: number; player: number; answer: Record<string, unknown> };

const SPEEDS = [0.25, 0.5, 1, 2, 4];

/**
 * Replays (?replay): the BGA games the server has in its native replay format, played from one
 * player's side (?replay=TABLE&seat=2001), automatically or step by step. ?replay=/file.json loads
 * recorded GameViews instead (scripts/check-decisions.ts --record), for development.
 */
export default function ReplayScreen() {
  const q = new URLSearchParams(location.search);
  const src = q.get('replay') ?? '';
  if (/^\d+$/.test(src)) return <Viewer table={src} seat={Number(q.get('seat')) || 2001} />;
  if (src) return <Viewer file={src} />;
  return <ReplayList />;
}

function ReplayList() {
  const [list, setList] = useState<ReplayInfo[] | null>(null);
  useEffect(() => {
    fetch('/api/v1/replays')
      .then((r) => r.json())
      .then(setList)
      .catch((e: Error) => toast(`Cannot load the replays: ${e.message}`));
  }, []);
  return (
    <div className="lobby-scroll">
      <div className="lobby">
        <header className="lobby-head">
          <h1>
            Altered <span>Replays</span>
          </h1>
          <a className="btn ghost" href="?">
            Back
          </a>
        </header>
        {!list && <p className="muted">Loading…</p>}
        {list && list.length === 0 && <p className="muted">No replay on this server (ARENA_REPLAYS).</p>}
        {list && (
          <ul className="replay-list">
            {list.map((r) => (
              <li key={r.table}>
                <span className="replay-players">
                  {r.players.map((p, i) => (
                    <span key={i}>
                      {i > 0 && <span className="muted"> vs </span>}
                      <b>{p.name}</b> <small className="muted">{p.faction}</small>
                    </span>
                  ))}
                </span>
                <small className="muted">
                  #{r.table} · {r.steps} steps
                </small>
                <span className="replay-seats">
                  {r.players.map((p, i) => (
                    <a key={i} className="btn tiny" href={`?replay=${r.table}&seat=${2001 + i}`}>
                      {p.name}'s side
                    </a>
                  ))}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function Viewer({ table, seat, file }: { table?: string; seat?: number; file?: string }) {
  const { catalog } = useCatalog();
  const [views, setViews] = useState<GameView[]>([]);
  const [steps, setSteps] = useState<ReplayStep[]>([]);
  const [i, setI] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const view = views[i];
  const refs = useMemo(() => (view ? view.cards.map((c) => c.ref).filter((r): r is string => !!r) : []), [view]);
  const log = useMemo(
    () => views.slice(0, i + 1).flatMap((v, k) => (k === 0 ? v.log : v.log.filter((e) => e.seq > (views[k - 1].log.at(-1)?.seq ?? -1)))),
    [views, i],
  );

  useEffect(() => {
    const url = table ? `/api/v1/replays/${table}?seat=${seat}` : file!;
    fetch(url)
      .then((r) => r.json())
      .then((data: { views: GameView[]; steps: ReplayStep[] } | GameView[] | GameView) => {
        if (Array.isArray(data) || !('views' in data)) {
          setViews((Array.isArray(data) ? data : [data]).filter((v) => v && 'seq' in v));
        } else {
          setViews(data.views.filter((v) => v && 'seq' in v));
          setSteps(data.steps);
        }
      })
      .catch((e: Error) => toast(`Cannot load ${url}: ${e.message}`));
  }, [table, seat, file]);

  // Automatic play: one step every 1/speed seconds, stopping at the end
  useEffect(() => {
    if (!playing) return;
    if (i >= views.length - 1) {
      setPlaying(false);
      return;
    }
    const t = setTimeout(() => setI((x) => Math.min(x + 1, views.length - 1)), 1000 / speed);
    return () => clearTimeout(t);
  }, [playing, i, speed, views.length]);

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight') setI((x) => Math.min(x + 1, views.length - 1));
      if (e.key === 'ArrowLeft') setI((x) => Math.max(x - 1, 0));
      if (e.key === ' ') {
        e.preventDefault();
        setPlaying((p) => !p);
      }
    };
    window.addEventListener('keydown', k);
    return () => window.removeEventListener('keydown', k);
  }, [views.length]);

  // Read-only: an answer is only shown
  const send = async (a: AnswerBody) => {
    toast(`Replay: answers are not sent (${a.type})`, 'info', 3000);
    return true;
  };

  if (!catalog || !view) return <div className="center-screen">Loading the replay…</div>;
  const step = i > 0 ? steps[i - 1] : undefined;
  const controls = (
    <span className="replay-controls">
      <button type="button" className="btn tiny" title="Start" onClick={() => setI(0)}>
        ⏮
      </button>
      <button type="button" className="btn tiny" title="Previous step (←)" onClick={() => setI((x) => Math.max(0, x - 1))}>
        ◀
      </button>
      <button type="button" className="btn tiny" title="Play / pause (space)" onClick={() => setPlaying((p) => !p)}>
        {playing ? '⏸' : '▶'}
      </button>
      <button type="button" className="btn tiny" title="Next step (→)" onClick={() => setI((x) => Math.min(views.length - 1, x + 1))}>
        ▶|
      </button>
      <select value={speed} onChange={(e) => setSpeed(Number(e.target.value))} title="Steps per second">
        {SPEEDS.map((s) => (
          <option key={s} value={s}>
            ×{s}
          </option>
        ))}
      </select>
      <input type="range" min={0} max={views.length - 1} value={i} onChange={(e) => setI(Number(e.target.value))} />
      <small className="muted">
        {i}/{views.length - 1}
        {step ? ` · ${step.answer.type as string}` : ''}
      </small>
      <a className="btn tiny ghost" href="?replay">
        List
      </a>
    </span>
  );
  return (
    <InspectProvider catalog={catalog} preferredRefs={refs}>
      <GameProvider view={view} catalog={catalog} log={log} sending={false} send={send}>
        <DragProvider>
          <Table onLeave={() => (location.search = '?replay')} connected extraTop={controls} />
        </DragProvider>
      </GameProvider>
      <Inspector />
    </InspectProvider>
  );
}
