import { useSyncExternalStore, type CSSProperties, type PointerEvent as ReactPointerEvent } from 'react';
import { createPortal } from 'react-dom';
import type { Biome, ButtonsDecision, PlayerView, RegionView } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { BiomeGlyph } from '../components/Glyph';
import { regionCenter } from './model';

/**
 * Terrain markers (Nadir & Bubbles): round biome tokens, as on BGA. They lie on the hero card until
 * Noon, when one may be dragged (or tapped, then its region tapped) onto a visible region. A marked
 * region is greyed, with the marker in its middle: it has that biome only while the marker stays.
 */

/** The marker being moved: dragged (x, y) or tapped (no position), and the region under it */
type Held = { id: number; biome: Biome; x: number | null; y: number | null; size: number; over: number | null } | null;

const store = { held: null as Held, subs: new Set<() => void>() };
function setHeld(h: Held) {
  store.held = h;
  store.subs.forEach((f) => f());
}
function useHeld(): Held {
  return useSyncExternalStore(
    (cb) => {
      store.subs.add(cb);
      return () => store.subs.delete(cb);
    },
    () => store.held,
  );
}

/** The Noon decision of Nadir, if it is the current one */
function useMarkRegion(): (ButtonsDecision['markRegion'] & {}) | null {
  const { decision, sending } = useGameCtx();
  return decision?.kind === 'buttons' && decision.markRegion && !sending ? decision.markRegion : null;
}

export function MarkerToken({ biome, className, style }: { biome: Biome; className?: string; style?: CSSProperties }) {
  return (
    <span className={`terrain-marker tot-${biome} ${className ?? ''}`} style={style}>
      <span className="tot-icon">
        <BiomeGlyph biome={biome} />
      </span>
    </span>
  );
}

/** The region under the pointer, among the allowed ones */
function regionAt(x: number, y: number, allowed: number[]): number | null {
  for (const el of document.elementsFromPoint(x, y)) {
    const r = (el as HTMLElement).dataset?.region;
    if (r !== undefined && allowed.includes(Number(r))) return Number(r);
  }
  return null;
}

/** The markers still on a hero card, along its right edge; at Noon they can be dragged onto a region */
export function HeroMarkers({ player }: { player: PlayerView }) {
  const { byId, send } = useGameCtx();
  const mark = useMarkRegion();
  const held = useHeld();
  const hero = player.heroCardId !== null ? byId.get(player.heroCardId) : undefined;
  const biomes = hero?.markers ?? [];
  if (!biomes.length) return null;

  const begin = (e: ReactPointerEvent, id: number, biome: Biome) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const start = { x: e.clientX, y: e.clientY };
    const size = (e.currentTarget as HTMLElement).getBoundingClientRect().width;
    let moved = false;
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 6) return;
      moved = true;
      setHeld({ id, biome, x: ev.clientX, y: ev.clientY, size, over: regionAt(ev.clientX, ev.clientY, mark!.regions) });
    };
    const stop = (drop: boolean) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', cancel);
      window.removeEventListener('keydown', key);
      const h = store.held;
      if (!moved) {
        // A tap: the marker is held until a region is tapped (or tapped again to put it back)
        setHeld(h?.id === id && h.x === null ? null : { id, biome, x: null, y: null, size, over: null });
        return;
      }
      setHeld(null);
      if (drop && h?.over != null) void send({ type: 'button', key: `${id}:${h.over}` });
    };
    const up = () => stop(true);
    const cancel = () => stop(false);
    const key = (ev: KeyboardEvent) => ev.key === 'Escape' && stop(false);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', cancel);
    window.addEventListener('keydown', key);
  };

  return (
    <span className="hero-markers">
      {biomes.map((b) => {
        const id = mark?.markers.find((m) => m.biome === b)?.id;
        const usable = id !== undefined;
        const out = held && held.id === id;
        return (
          <button
            key={b}
            type="button"
            className={`hero-marker ${usable ? 'usable' : ''} ${out ? (held!.x === null ? 'held' : 'out') : ''}`}
            disabled={!usable}
            title={usable ? `Drag the ${b} marker onto a visible region` : `${b} terrain marker`}
            aria-label={`${b} terrain marker`}
            onPointerDown={usable ? (e) => begin(e, id, b) : undefined}
          >
            <MarkerToken biome={b} />
          </button>
        );
      })}
    </span>
  );
}

/** How far (in track units) the expedition tokens move right in a marked region, clear of the marker */
export const MARKED_SHIFT = 0.14;

/** The biomes a region has now: the marker's only, when it is marked */
export function regionBiomes(r: RegionView | undefined): Biome[] | null {
  if (!r) return null;
  return r.marker ? [r.marker.biome] : r.biomes;
}

/**
 * On the track: the marked regions (greyed, the marker in the middle), and at Noon the regions a
 * marker may go to, lit; the one under the dragged marker previews its new biome.
 */
export function RegionMarkers({ regions, u }: { regions: RegionView[]; u: number }) {
  const { send } = useGameCtx();
  const mark = useMarkRegion();
  const held = useHeld();
  const span = (i: number): CSSProperties => ({ left: (regionCenter(i) - 0.5) * u, width: u });
  return (
    <>
      {regions
        .filter((r) => r.marker)
        .flatMap((r) => [
          <span key={`m${r.index}`} className="region-marked" style={span(r.index)} />,
          // On the left of the region: the expedition tokens there move to the right (MARKED_SHIFT)
          <MarkerToken key={`t${r.index}`} biome={r.marker!.biome} className="region-marker" style={{ left: (regionCenter(r.index) - 0.24) * u }} />,
        ])}
      {mark &&
        mark.regions.map((i) => {
          const over = held?.over === i;
          return (
            <span
              key={`d${i}`}
              className={`region-drop ${held ? 'armed' : ''} ${over ? 'over' : ''}`}
              data-region={i}
              style={span(i)}
              onClick={held && held.x === null ? () => (setHeld(null), void send({ type: 'button', key: `${held.id}:${i}` })) : undefined}
              onPointerEnter={held && held.x === null ? () => setHeld({ ...held, over: i }) : undefined}
              onPointerLeave={held && held.x === null ? () => setHeld({ ...held, over: null }) : undefined}
            >
              {over && <MarkerToken biome={held!.biome} className="preview" />}
            </span>
          );
        })}
    </>
  );
}

/** The marker following the pointer, during a drag */
export function MarkerGhost() {
  const held = useHeld();
  if (!held || held.x === null || held.y === null) return null;
  return createPortal(
    <MarkerToken
      biome={held.biome}
      className={`marker-ghost ${held.over != null ? 'over' : ''}`}
      style={{ left: held.x, top: held.y, ['--ghost-size' as string]: `${held.size}px` }}
    />,
    document.body,
  );
}
