import { createContext, useCallback, useContext, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { Destination } from '@protocol';
import { useGameCtx } from './GameContext';
import { Card } from '../components/Card';

interface DragState {
  cardId: number;
  destinations: Destination[];
  over: Destination | null;
  /** dropped on a destination: the card waits there for the answer of the server */
  dropped?: boolean;
  /** dropped elsewhere: the card goes back to its place */
  returning?: boolean;
}

interface DragCtxValue {
  drag: DragState | null;
  begin: (cardId: number, destinations: Destination[], x: number, y: number, from: HTMLElement) => void;
  move: (x: number, y: number) => void;
  end: (drop: boolean) => void;
}

/** Where the last dragged card was dropped: the arena flies it from there instead of from the hand */
export const lastDrop: { cardId: number | null; x: number; y: number; at: number } = { cardId: null, x: 0, y: 0, at: 0 };

const DragCtx = createContext<DragCtxValue>({ drag: null, begin: () => {}, move: () => {}, end: () => {} });
export const useDrag = () => useContext(DragCtx);

/** Finds the drop zone under the pointer: the innermost element with data-drop among the allowed destinations. */
function dropAt(x: number, y: number, allowed: Destination[]): Destination | null {
  for (const el of document.elementsFromPoint(x, y)) {
    const d = (el as HTMLElement).dataset?.drop as Destination | undefined;
    if (d && allowed.includes(d)) return d;
  }
  return null;
}

const clamp = (v: number, m: number) => Math.max(-m, Math.min(m, v));

/**
 * Where a card is drawn on the table: its centre, its width (without the rotation of the fan, and
 * with the scale of the stage), and its angle in the fan of the hand.
 */
function homeOf(el: HTMLElement | null): { x: number; y: number; width: number; rot: number } | null {
  if (!el || !el.isConnected) return null;
  const r = el.getBoundingClientRect();
  const stage = el.closest<HTMLElement>('.arena-stage');
  const slot = el.closest<HTMLElement>('.ah-slot');
  const card = el.querySelector<HTMLElement>('.card') ?? el;
  const scale = stage && stage.offsetWidth ? stage.getBoundingClientRect().width / stage.offsetWidth : 1;
  // Cards of the fan: their layout width; cards lying on the tilted mat: what is seen of them
  const width = slot ? card.offsetWidth * scale : Math.min(r.width, r.height * (324 / 450));
  const rot = slot ? parseFloat(slot.style.getPropertyValue('--rot')) || 0 : 0;
  return { x: r.left + r.width / 2, y: r.top + r.height / 2, width, rot };
}
/** Duration of the way back to the hand, when a card is dropped outside any destination */
const RETURN_MS = 160;

/** The ghost card: where it is, where it goes, how it leans */
interface Motion {
  x: number;
  y: number;
  /** where the pointer is (dragging) or where the card goes back (returning) */
  tx: number;
  ty: number;
  /** lean, in degrees: around the vertical axis, the horizontal axis, and in the plane */
  ry: number;
  rx: number;
  rz: number;
  scale: number;
  frame: number;
  /** going back: where it started from, and when */
  back?: { x: number; y: number; rx: number; ry: number; rz: number; scale: number; at: number };
}

/**
 * Hand-written drag and drop on pointer events (mouse, touch, pen). The card leaves its place
 * and a ghost of it follows the pointer with a little delay, leaning with the motion (it tilts
 * towards where it goes, more when the pointer moves fast) and shrinking over a drop zone. Dropped
 * on a destination, it waits there for the server, and the arena flies it from there to its new
 * place; dropped elsewhere, it goes back to its place. Drop zones are found with
 * elementsFromPoint. The ghost is moved in a requestAnimationFrame loop, written to the DOM
 * directly: a drag does not re-render the table on every move.
 *
 * Once a drag has begun, the pointer is followed on the window, not on the card: the drag ends
 * whatever the pointer does (released anywhere, lost, the window left, Escape).
 */
export function DragProvider({ children }: { children: ReactNode }) {
  const { byId, def, play } = useGameCtx();
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState(0);
  /** the card on the table, hidden during the drag: the way back ends exactly on it */
  const source = useRef<HTMLElement | null>(null);
  const m = useRef<Motion>({ x: 0, y: 0, tx: 0, ty: 0, ry: 0, rx: 0, rz: 0, scale: 1, frame: 0 });
  const unlisten = useRef<(() => void) | null>(null);

  const set = (s: DragState | null) => {
    dragRef.current = s;
    setDrag(s);
  };

  // One frame of the ghost: it eases towards its target, and leans with its speed
  const step = useCallback(() => {
    const s = dragRef.current;
    const g = ghostRef.current;
    const k = m.current;
    if (!s) return;
    const returning = !!s.returning;
    if (returning && k.back) {
      // Back to its place in a fixed short time, fast then slowing down
      const b = k.back;
      const p = Math.min(1, (performance.now() - b.at) / RETURN_MS);
      const e = 1 - (1 - p) ** 3;
      // The place is followed every frame: the hand opens again meanwhile
      const home = homeOf(source.current);
      if (home) {
        k.tx = home.x;
        k.ty = home.y;
      }
      k.x = b.x + (k.tx - b.x) * e;
      k.y = b.y + (k.ty - b.y) * e;
      k.rx = b.rx * (1 - e);
      k.ry = b.ry * (1 - e);
      k.rz = b.rz + ((home?.rot ?? 0) - b.rz) * e;
      k.scale = b.scale + (1 - b.scale) * e;
      if (g)
        g.style.transform =
          `translate(${k.x}px, ${k.y}px) translate(-50%, -50%) perspective(700px) ` +
          `rotateX(${k.rx}deg) rotateY(${k.ry}deg) rotate(${k.rz}deg) scale(${k.scale})`;
      if (p >= 1) {
        set(null);
        return;
      }
      k.frame = requestAnimationFrame(step);
      return;
    }
    const ease = s.dropped ? 0.5 : 0.32;
    const vx = (k.tx - k.x) * ease;
    const vy = (k.ty - k.y) * ease;
    k.x += vx;
    k.y += vy;
    const lean = s.dropped ? 0 : 1;
    k.ry += (clamp(vx * 1.6, 28) * lean - k.ry) * 0.25;
    k.rx += (clamp(-vy * 1.3, 24) * lean - k.rx) * 0.25;
    k.rz += (clamp(vx * 0.7, 12) * lean - k.rz) * 0.25;
    const scale = s.over ? 0.86 : 1.08;
    k.scale += (scale - k.scale) * 0.25;
    if (g) {
      g.style.visibility = 'visible';
      g.style.transform =
        `translate(${k.x}px, ${k.y}px) translate(-50%, -50%) perspective(700px) ` +
        `rotateX(${k.rx}deg) rotateY(${k.ry}deg) rotate(${k.rz}deg) scale(${k.scale})`;
    }
    k.frame = requestAnimationFrame(step);
  }, []);

  const begin = useCallback(
    (cardId: number, destinations: Destination[], x: number, y: number, from: HTMLElement) => {
      cancelAnimationFrame(m.current.frame);
      source.current = from;
      const home = homeOf(from)!;
      // The ghost starts on the card, as it is drawn on the table
      m.current = { x: home.x, y: home.y, tx: x, ty: y, ry: 0, rx: 0, rz: home.rot, scale: 1, frame: 0 };
      setSize(home.width);
      set({ cardId, destinations, over: null });
      m.current.frame = requestAnimationFrame(step);
      document.body.classList.add('is-dragging');
      // Follow the pointer on the window until the drag ends
      unlisten.current?.();
      const onMove = (e: PointerEvent) => moveRef.current(e.clientX, e.clientY);
      const onUp = () => endRef.current(true);
      const onCancel = () => endRef.current(false);
      const onKey = (e: KeyboardEvent) => e.key === 'Escape' && endRef.current(false);
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
      window.addEventListener('pointercancel', onCancel);
      window.addEventListener('blur', onCancel);
      window.addEventListener('keydown', onKey);
      unlisten.current = () => {
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onCancel);
        window.removeEventListener('blur', onCancel);
        window.removeEventListener('keydown', onKey);
        unlisten.current = null;
      };
    },
    [step],
  );

  const move = useCallback((x: number, y: number) => {
    const s = dragRef.current;
    if (!s || s.dropped || s.returning) return;
    m.current.tx = x;
    m.current.ty = y;
    const over = dropAt(x, y, s.destinations);
    if (over !== s.over) set({ ...s, over });
  }, []);

  const end = useCallback(
    (drop: boolean) => {
      const s = dragRef.current;
      unlisten.current?.();
      document.body.classList.remove('is-dragging');
      // Already ended (released twice, or released then cancelled)
      if (!s || s.dropped || s.returning) return;
      if (drop && s.over) {
        const k = m.current;
        k.tx = k.x;
        k.ty = k.y;
        Object.assign(lastDrop, { cardId: s.cardId, x: k.x, y: k.y, at: performance.now() });
        const dropped = { ...s, destinations: [], dropped: true };
        set(dropped);
        // The ghost stays where it was dropped until the new view moves the card; meanwhile
        // another drag may have begun, which this must leave alone
        void play(s.cardId, s.over).finally(() => {
          if (dragRef.current !== dropped) return;
          cancelAnimationFrame(m.current.frame);
          set(null);
        });
      } else {
        const k = m.current;
        k.back = { x: k.x, y: k.y, rx: k.rx, ry: k.ry, rz: k.rz, scale: k.scale, at: performance.now() };
        set({ ...s, destinations: [], over: null, returning: true });
      }
    },
    [play],
  );

  const moveRef = useRef(move);
  moveRef.current = move;
  const endRef = useRef(end);
  endRef.current = end;

  const value = useMemo(() => ({ drag, begin, move, end }), [drag, begin, move, end]);
  const card = drag ? byId.get(drag.cardId) : undefined;
  return (
    <DragCtx.Provider value={value}>
      {children}
      {drag &&
        createPortal(
          <div
            ref={ghostRef}
            className={`drag-ghost ${drag.over || drag.dropped ? 'over' : ''}`}
            style={{ ['--card-hand-w' as string]: `${size}px` } as CSSProperties}
          >
            <Card def={def(card)} card={card} size="hand" />
          </div>,
          document.body,
        )}
    </DragCtx.Provider>
  );
}
