import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { useGameCtx } from '../game/GameContext';
import { Log } from '../components/Log';
import { EndScreen } from '../components/EndScreen';
import { useArenaLayout } from './layout';
import { Mat } from './Mat';
import { ArenaHand, HandSortButton, InfoChip, isHandSort, OppHand, PhaseBanner, PlayerChip, Prompt, RotateHint, TrackNotice, type HandSort } from './Hud';
import { saveHandSort, savedHandSort } from '../screens/storage';
import { fly, snapshot, type Snapshot } from './flights';
import { launchReveal, REVEAL_MS } from './reveals';
import { CastFocus, TargetArrows } from './Cast';
import { CardActions } from './CardActions';
import { TrackButton } from './TrackButton';
import { EffectQueue, EffectQueueBadge } from './EffectQueue';
import { effectQueue } from '../game/decisions';
import { MarkerGhost } from './TerrainMarkers';
import './arena.css';

const coarse = typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches;

/**
 * The game table: the BGA mat tilted in perspective (CSS 3D, plain DOM), with the hand, the
 * prompt and the players around it. Cards changing zone fly between their places (flights.ts).
 */
export function Arena({
  onLeave,
  connected,
  extraTop,
  oppAction,
  undo,
}: {
  onLeave: () => void;
  connected: boolean;
  extraTop?: ReactNode;
  /** A control on the opponent's name (a game against oneself: take their seat) */
  oppAction?: ReactNode;
  /** Against bots: takes back the player's last move (a round button beside the main one) */
  undo?: { onUndo: () => void; disabled: boolean };
}) {
  const { view, me, opp, log, you, catalog, decision } = useGameCtx('view', 'me', 'opp', 'log', 'you', 'catalog', 'decision');
  // Undo: against bots, the server takes back my last move (the engine's undo is not offered)
  const undoBtn = undo;
  // Effects to resolve: the hand is not playable, it retracts. The queue's minimize button shrinks
  // it into a badge right of the main button and brings the hand up; the badge brings the queue
  // back. Minimized, it stays so through the next effects, until there are none left.
  const queueOn = !!effectQueue(decision);
  const [minimized, setMinimized] = useState(false);
  useEffect(() => {
    if (!queueOn) setMinimized(false);
  }, [queueOn]);
  const { layout: l, scale, portrait } = useArenaLayout();
  // Order of my hand, remembered on this device
  const [handSort, setHandSort] = useState<HandSort>(() => {
    const s = savedHandSort();
    return isHandSort(s) ? s : 'draw';
  });
  const changeHandSort = (s: HandSort) => {
    setHandSort(s);
    saveHandSort(s);
  };
  // Right edge of the controls bottom left (my name, the sort button): the hand stays clear of it.
  // Offsets are stage pixels (the stage's scale does not change them)
  const meDockRef = useRef<HTMLDivElement>(null);
  const [meDockRight, setMeDockRight] = useState<number | undefined>(undefined);
  useLayoutEffect(() => {
    const el = meDockRef.current;
    if (!el) return;
    const measure = () => setMeDockRight(el.offsetLeft + el.offsetWidth);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [l.mode]);
  const rootRef = useRef<HTMLDivElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const snap = useRef<Snapshot | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;
  // Reveals already shown (those of the first view are not: they happened before)
  const seenReveals = useRef<Set<number> | null>(null);

  // Flights: compare the cards' places before and after each new view, before paint
  useLayoutEffect(() => {
    const root = rootRef.current;
    const layer = layerRef.current;
    if (!root || !layer) return;
    const next = snapshot(root, view);
    const prev = snap.current;
    snap.current = next;
    const reveals = view.reveals ?? [];
    const fresh = seenReveals.current ? reveals.filter((r) => !seenReveals.current!.has(r.seq)) : [];
    seenReveals.current = new Set([...(seenReveals.current ?? []), ...reveals.map((r) => r.seq)]);
    if (prev && prev.seq !== view.seq) {
      const draws = new Map<number, number>();
      for (const r of fresh) if (r.to === 'hand') draws.set(r.player, (draws.get(r.player) ?? 0) + 1);
      fly(prev, next, view, root, layer, catalog, { cards: new Set(fresh.map((r) => r.cardId)), draws });
      fresh.forEach((r, i) => launchReveal(r, prev, next, view, root, layer, catalog, i * REVEAL_MS));
    }
    // Measure again once the cards have settled (flights, hand layout)
    const t = setTimeout(() => {
      if (rootRef.current) snap.current = snapshot(rootRef.current, viewRef.current);
    }, 450);
    return () => clearTimeout(t);
  }, [view, catalog]);
  // A new layout or scale moves everything: measure again, without flights
  useEffect(() => {
    const id = requestAnimationFrame(() => {
      if (rootRef.current) snap.current = snapshot(rootRef.current, viewRef.current);
    });
    return () => cancelAnimationFrame(id);
  }, [l, scale]);

  // Centre of the Tumult track on the stage (the mat is tilted: measured), for the information prompts
  const stageRef = useRef<HTMLDivElement>(null);
  const [trackAt, setTrackAt] = useState<{ x: number; y: number } | null>(null);
  useLayoutEffect(() => {
    const stage = stageRef.current;
    const track = stage?.querySelector('.track');
    if (!stage || !track) return;
    const s = stage.getBoundingClientRect();
    const t = track.getBoundingClientRect();
    setTrackAt({ x: (t.left + t.width / 2 - s.left) / scale, y: (t.top + t.height / 2 - s.top) / scale });
  }, [l, scale]);

  const [logOpen, setLogOpen] = useState(false);
  const [seen, setSeen] = useState(0);
  const lastSeq = log.length ? log[log.length - 1].seq : 0;
  useEffect(() => {
    if (logOpen) setSeen(lastSeq);
  }, [logOpen, lastSeq]);
  const unread = log.filter((e) => e.seq > seen).length;

  const stageStyle = { width: l.W, height: l.H, transform: `translate(-50%, -50%) scale(${scale})`, ...l.vars } as CSSProperties;
  return (
    <div className={`arena layout-${l.mode}`} ref={rootRef} style={{ '--stage-scale': scale } as CSSProperties}>
      <div className="arena-stage" ref={stageRef} style={stageStyle}>
        <div
          className="arena-scene"
          style={{ left: l.plane.left, top: l.plane.top, width: l.plane.w, height: l.plane.h, perspective: `${l.plane.perspective}px` }}
        >
          <div className="arena-plane" data-cast="" style={{ transform: `rotateX(${l.plane.tilt}deg)` }}>
            <Mat l={l} />
          </div>
        </div>
        {opp && <PlayerChip player={opp} mine={false} className="chip-opp" action={oppAction} />}
        {/* Bottom left: my name, then the sort button (on phones my name is bottom right) */}
        <div className="me-dock" ref={meDockRef}>
          {me && l.mode === 'wide' && <PlayerChip player={me} mine={you !== null} className="chip-me" />}
          {me && you !== null && <HandSortButton sort={handSort} onChange={changeHandSort} />}
        </div>
        {me && l.mode !== 'wide' && <PlayerChip player={me} mine={you !== null} className="chip-me" />}
        {opp && <OppHand player={opp} />}
        <InfoChip onLeave={onLeave} connected={connected} onLog={() => setLogOpen((o) => !o)} unread={unread} />
        <ArenaHand
          retracted={queueOn && !minimized}
          inactive={queueOn}
          sort={handSort}
          leftBound={meDockRight}
          stageW={l.W}
          cardW={parseFloat(l.vars['--card-hand-w'])}
        />
        {/* Every decision is taken at the bottom of the screen, centered above the hand */}
        <div className="center-dock">
          <Prompt />
          <EffectQueue collapsed={minimized} onCollapse={() => setMinimized(true)} />
        </div>
        <div className="end-dock">
          {undoBtn && (
            <span className="undo-dock">
              <button type="button" className="undo-btn" disabled={undoBtn.disabled} onClick={undoBtn.onUndo} aria-label="Undo your last move">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M9 14 4 9l5-5" />
                  <path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11" />
                </svg>
              </button>
              <span className="undo-caption" aria-hidden="true">
                Undo
              </span>
            </span>
          )}
          <TrackButton />
          {queueOn && minimized && <EffectQueueBadge onExpand={() => setMinimized(false)} />}
        </div>
        <TrackNotice at={trackAt} />
        <PhaseBanner />
        {extraTop && <div className="arena-extra">{extraTop}</div>}
      </div>
      <CastFocus />
      <CardActions />
      <TargetArrows />
      <MarkerGhost />
      <div className="arena-flights" ref={layerRef} aria-hidden="true" />
      <Log entries={log} you={you} open={logOpen} onClose={() => setLogOpen(false)} />
      <EndScreen onLeave={onLeave} />
      {portrait && coarse && <RotateHint />}
    </div>
  );
}
