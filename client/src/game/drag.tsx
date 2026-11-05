import { createContext, useCallback, useContext, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { createStore, useStore, type StoreApi } from 'zustand';
import { createPortal } from 'react-dom';
import type { Destination } from '@protocol';
import { useCard, useGameCtx } from './GameContext';
import { Card } from '../components/Card';

interface DragState {
  cardId: number;
  destinations: Destination[];
  over: Destination | null;
  /** dropped on a destination: the card waits there for the answer of the server */
  dropped?: boolean;
  /** dropped elsewhere: the card goes back to its place */
  returning?: boolean;
  /** dragged by a finger */
  touch?: boolean;
  /** a spell held by a finger over the Cancel zone */
  cancelling?: boolean;
  /**
   * a spell held by a finger over the Cast zone. Not `over`: nothing else on the table reacts (the
   * spell's drop zone on the tilted mat lit up, which a phone repainted on every frame)
   */
  casting?: boolean;
}

/**
 * A spell dragged by a finger is not cast anywhere on the board, as it is with a mouse: a finger
 * cannot easily take it back to the hand. Two large zones show instead, Cancel on the left and Cast
 * on the right (SpellZones).
 */
const touchSpell = (s: DragState | null) => !!s?.touch && s.destinations.includes('spell');

/** The spell zone under a point: 'cast', 'cancel' or null */
function spellZoneAt(x: number, y: number): 'cast' | 'cancel' | null {
  // The zones let pointer events through (elementsFromPoint skips them): their boxes are read
  for (const el of document.querySelectorAll<HTMLElement>('[data-spell-zone]')) {
    const r = el.getBoundingClientRect();
    if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return el.dataset.spellZone as 'cast' | 'cancel';
  }
  return null;
}

interface DragActions {
  /** `touch`: the ghost is drawn above the finger (which would hide it), and larger */
  begin: (cardId: number, destinations: Destination[], x: number, y: number, from: HTMLElement, touch?: boolean) => void;
  move: (x: number, y: number) => void;
  end: (drop: boolean) => void;
  /** The drag going on now (read in event handlers: no subscription) */
  current: () => DragState | null;
}

/** Where the last dragged card was dropped: the arena flies it from there instead of from the hand */
export const lastDrop: { cardId: number | null; x: number; y: number; at: number } = { cardId: null, x: 0, y: 0, at: 0 };

const noDrag = createStore<DragState | null>(() => null);
const DragCtx = createContext<{ store: StoreApi<DragState | null>; actions: DragActions }>({
  store: noDrag,
  actions: { begin: () => {}, move: () => {}, end: () => {}, current: () => null },
});
/** The drag functions (stable: no redraw when a drag starts or ends) */
export const useDragActions = () => useContext(DragCtx).actions;
/** A value derived from the drag going on: the component is drawn again only when it changes */
export function useDragState<T>(select: (d: DragState | null) => T): T {
  return useStore(useContext(DragCtx).store, select);
}
/** The drag going on and the drag functions */
export function useDrag() {
  return { drag: useDragState((d) => d), ...useDragActions() };
}

/**
 * Finds the drop zone under the pointer: the innermost element with data-drop among the allowed
 * destinations. A spell is cast anywhere on the board (data-cast), except back on the hand.
 */
function dropAt(x: number, y: number, allowed: Destination[], board = true): Destination | null {
  for (const el of document.elementsFromPoint(x, y)) {
    const h = el as HTMLElement;
    if (h.closest?.('.arena-hand')) return null;
    const d = h.dataset?.drop as Destination | undefined;
    if (d && allowed.includes(d)) return d;
    if (board && h.dataset?.cast !== undefined && allowed.includes('spell')) return 'spell';
  }
  return null;
}

/** Classes of the body during a drag: a spell is dragged, a spell is over the board */
function bodyClasses(s: DragState | null) {
  const spell = !!s && !s.dropped && !s.returning && s.destinations.includes('spell');
  // A finger casts on the Cast zone only: the board is not lit as a target
  document.body.classList.toggle('drag-spell', spell && !s!.touch);
  document.body.classList.toggle('drag-cast', spell && s!.over === 'spell');
}

const clamp = (v: number, m: number) => Math.max(-m, Math.min(m, v));

/** Touch drags: the ghost is this much larger than the card in the hand, and sits above the finger */
const TOUCH_GHOST_SCALE = 1.5;
const TOUCH_GHOST_MAX_W = 140;

/**
 * Stops the browser from scrolling, zooming or going back while a finger drags a card. On iOS
 * Safari touch-action alone is not enough (a pan cancels the pointer events): touchmove is
 * cancelled as well. Returns the function that removes the listener.
 */
export function holdTouch(): () => void {
  const stop = (e: TouchEvent) => {
    if (e.cancelable) e.preventDefault();
  };
  window.addEventListener('touchmove', stop, { passive: false });
  return () => window.removeEventListener('touchmove', stop);
}

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
  const { def, play } = useGameCtx('def', 'play');
  const [store] = useState(() => createStore<DragState | null>(() => null));
  const drag = useStore(store);
  const dragRef = useRef<DragState | null>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState(0);
  /** the card on the table, hidden during the drag: the way back ends exactly on it */
  const source = useRef<HTMLElement | null>(null);
  const m = useRef<Motion>({ x: 0, y: 0, tx: 0, ty: 0, ry: 0, rx: 0, rz: 0, scale: 1, frame: 0 });
  const unlisten = useRef<(() => void) | null>(null);
  /** The pointer position not yet hit tested (move -> next frame) */
  const aim = useRef<{ x: number; y: number } | null>(null);
  /** Touch drags: how far above the finger the ghost's centre is (px) */
  const lift = useRef(0);

  const set = (s: DragState | null) => {
    dragRef.current = s;
    store.setState(s, true);
    bodyClasses(s);
  };

  // One frame of the ghost: it eases towards its target, and leans with its speed
  const step = useCallback(() => {
    hitTestRef.current();
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
    // Over the board, a spell rises to be cast; any other card shrinks into its zone
    const scale = s.over === 'spell' ? 1.16 : s.over ? 0.86 : 1.08;
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
    (cardId: number, destinations: Destination[], x: number, y: number, from: HTMLElement, touch = false) => {
      cancelAnimationFrame(m.current.frame);
      source.current = from;
      const home = homeOf(from)!;
      const width = touch ? Math.max(home.width, Math.min(TOUCH_GHOST_MAX_W, home.width * TOUCH_GHOST_SCALE)) : home.width;
      // The finger holds the card by its lower part: the card shows, and still reaches the zones near the hand
      lift.current = touch ? width * (450 / 324) * 0.3 : 0;
      // The ghost starts on the card, as it is drawn on the table
      m.current = { x: home.x, y: home.y, tx: x, ty: y - lift.current, ry: 0, rx: 0, rz: home.rot, scale: 1, frame: 0 };
      setSize(width);
      set({ cardId, destinations, over: null, touch });
      m.current.frame = requestAnimationFrame(step);
      document.body.classList.add('is-dragging');
      // Follow the pointer on the window until the drag ends
      unlisten.current?.();
      const onMove = (e: PointerEvent) => moveRef.current(e.clientX, e.clientY);
      const onUp = () => endRef.current(true);
      const onCancel = () => endRef.current(false);
      const onKey = (e: KeyboardEvent) => e.key === 'Escape' && endRef.current(false);
      const release = holdTouch();
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
        release();
        unlisten.current = null;
      };
    },
    [step],
  );

  const move = useCallback((x: number, y: number) => {
    const s = dragRef.current;
    if (!s || s.dropped || s.returning) return;
    // A touch drag aims with the card, drawn above the finger
    m.current.tx = x;
    m.current.ty = y - lift.current;
    // What is under it is found in the next frame, once (a touch screen sends several moves per frame)
    aim.current = { x, y };
  }, []);

  /** The drop zone under the last pointer position, if it moved since the last frame */
  const hitTest = () => {
    const p = aim.current;
    aim.current = null;
    const s = dragRef.current;
    if (!p || !s || s.dropped || s.returning) return;
    const { x, y } = p;
    const cy = y - lift.current;
    if (touchSpell(s)) {
      // The Cast and Cancel zones are large: the card or the finger may be on them
      const zone = spellZoneAt(x, cy) ?? spellZoneAt(x, y);
      const over = zone ? null : dropAt(x, cy, s.destinations, false);
      const cancelling = zone === 'cancel';
      const casting = zone === 'cast';
      if (over !== s.over || cancelling !== !!s.cancelling || casting !== !!s.casting) set({ ...s, over, cancelling, casting });
      return;
    }
    const over = dropAt(x, cy, s.destinations) ?? (cy !== y ? dropAt(x, y, s.destinations) : null);
    if (over !== s.over) set({ ...s, over });
  };
  const hitTestRef = useRef(hitTest);
  hitTestRef.current = hitTest;

  const end = useCallback(
    (drop: boolean) => {
      // Released before the next frame: what is under the pointer now decides
      hitTestRef.current();
      const s = dragRef.current;
      unlisten.current?.();
      document.body.classList.remove('is-dragging');
      // Already ended (released twice, or released then cancelled)
      if (!s || s.dropped || s.returning) return;
      const target = s.over ?? (s.casting ? 'spell' : null);
      if (drop && target) {
        const k = m.current;
        k.tx = k.x;
        k.ty = k.y;
        Object.assign(lastDrop, { cardId: s.cardId, x: k.x, y: k.y, at: performance.now() });
        const dropped = { ...s, destinations: [], dropped: true };
        set(dropped);
        // The ghost stays where it was dropped until the new view moves the card; meanwhile
        // another drag may have begun, which this must leave alone
        void play(s.cardId, target).finally(() => {
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

  const current = useCallback(() => dragRef.current, []);
  const value = useMemo(() => ({ store, actions: { begin, move, end, current } }), [store, begin, move, end, current]);
  const card = useCard(drag?.cardId);
  return (
    <DragCtx.Provider value={value}>
      {children}
      {drag &&
        createPortal(
          <div
            ref={ghostRef}
            className={`drag-ghost ${drag.over || drag.dropped ? 'over' : ''} ${drag.over === 'spell' ? 'cast' : ''}`}
            style={{ '--card-hand-w': `${size}px` } as CSSProperties}
          >
            <Card def={def(card)} card={card} size="hand" />
            {drag.over === 'spell' && !drag.dropped && <span className="cast-tag">Release to cast</span>}
          </div>,
          document.body,
        )}
      {drag && touchSpell(drag) && !drag.dropped && !drag.returning && <SpellZones cast={!!drag.casting} cancel={!!drag.cancelling} />}
    </DragCtx.Provider>
  );
}

/** A spell held by a finger: let it go on the left to cancel, on the right to cast */
function SpellZones({ cast, cancel }: { cast: boolean; cancel: boolean }) {
  return createPortal(
    <div className="spell-zones" aria-hidden="true">
      <div className={`spell-zone sz-cancel ${cancel ? 'on' : ''}`} data-spell-zone="cancel">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round">
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
        <span>Cancel</span>
      </div>
      <div className={`spell-zone sz-cast ${cast ? 'on' : ''}`} data-spell-zone="cast">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" />
        </svg>
        <span>Cast</span>
      </div>
    </div>,
    document.body,
  );
}
