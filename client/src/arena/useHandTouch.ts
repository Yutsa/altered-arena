import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type RefObject } from 'react';
import { useGameCtx, useGameStore } from '../game/GameContext';
import { holdTouch, useDragActions } from '../game/drag';
import { useInspect } from '../game/inspect';

/** A finger that moves less than this is a tap (px) */
const TAP_SLOP = 10;
/** A finger raised this far above where it touched the hand takes the card out (px) */
const LIFT = 34;
/** Held this long, or moved: the card under the finger is shown large above the hand (ms) */
const PEEK_MS = 110;

interface Gesture {
  pointer: number;
  x0: number;
  y0: number;
  card: number;
  moved: boolean;
  timers: number[];
  unlisten: () => void;
}

/**
 * My hand under a finger, as in the card games made for phones: the finger slides along the hand
 * and the card under it rises and is shown large above the hand (what hovering does with a
 * mouse); raised towards the table, the card leaves the hand and is dragged; a tap selects it
 * (its actions, the zones it can be played on, its zoom). Holding a card shows it large too: there
 * is no long press to wait for.
 *
 * The card under the finger is the one whose place in the fan is nearest, by x only: the finger
 * can slide along the bottom edge of the screen without leaving the hand.
 */
export function useHandTouch(handRef: RefObject<HTMLDivElement | null>) {
  const { clickCard } = useGameCtx('clickCard');
  const { begin } = useDragActions();
  const store = useGameStore();
  const { hide } = useInspect();
  const [focus, setFocus] = useState<number | null>(null);
  const [peek, setPeek] = useState(false);
  const g = useRef<Gesture | null>(null);

  // The latest values, for the window listeners of a gesture
  // The game state is read when the finger moves (no subscription: the hand is not drawn again)
  const live = useRef({ clickCard, begin, hide });
  live.current = { clickCard, begin, hide };

  const stop = useCallback(() => {
    const cur = g.current;
    if (!cur) return;
    cur.timers.forEach(clearTimeout);
    cur.unlisten();
    g.current = null;
    setFocus(null);
    setPeek(false);
  }, []);
  useEffect(() => stop, [stop]);

  /** The card of the hand whose place is nearest to x (screen px) */
  const cardAt = (x: number): number | null => {
    const hand = handRef.current;
    if (!hand) return null;
    let best: number | null = null;
    let dist = Infinity;
    for (const slot of hand.querySelectorAll<HTMLElement>('.ah-slot[data-hand-card]')) {
      // The place at rest: the slot's layout box, without the rise and scale of the focused card
      const r = slot.getBoundingClientRect();
      const cx = parseFloat(slot.dataset.cx ?? '') || r.left + r.width / 2;
      const d = Math.abs(cx - x);
      if (d < dist) {
        dist = d;
        best = Number(slot.dataset.handCard);
      }
    }
    return best;
  };

  const onPointerDown = (e: ReactPointerEvent) => {
    if (e.pointerType === 'mouse' || e.button !== 0) return;
    const el = (e.target as HTMLElement).closest<HTMLElement>('.ah-slot[data-hand-card]');
    if (!el) return;
    stop();
    // Places at rest of every card, measured once: the focused card rises, its place does not move
    for (const slot of handRef.current?.querySelectorAll<HTMLElement>('.ah-slot[data-hand-card]') ?? []) {
      const r = slot.getBoundingClientRect();
      slot.dataset.cx = String(r.left + r.width / 2);
      slot.dataset.top = String(r.top);
    }
    const card = Number(el.dataset.handCard);
    const release = holdTouch();
    const onMove = (ev: PointerEvent) => move(ev);
    const onUp = (ev: PointerEvent) => up(ev);
    const onCancel = (ev: PointerEvent) => ev.pointerId === g.current?.pointer && stop();
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onCancel);
    const cur: Gesture = {
      pointer: e.pointerId,
      x0: e.clientX,
      y0: e.clientY,
      card,
      moved: false,
      timers: [],
      unlisten: () => {
        release();
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        window.removeEventListener('pointercancel', onCancel);
      },
    };
    cur.timers.push(window.setTimeout(() => g.current === cur && setPeek(true), PEEK_MS));
    g.current = cur;
    setFocus(card);
    live.current.hide();
  };

  function move(e: PointerEvent) {
    const cur = g.current;
    if (!cur || e.pointerId !== cur.pointer) return;
    const dx = e.clientX - cur.x0;
    const dy = e.clientY - cur.y0;
    if (!cur.moved && Math.hypot(dx, dy) > TAP_SLOP) {
      cur.moved = true;
      setPeek(true);
    }
    if (!cur.moved) return;
    // Raised towards the table: the card leaves the hand, if it can be played
    const { begin } = live.current;
    const { aff, decision, sending } = store.getState();
    const destinations = decision?.kind === 'turn' && !sending ? aff.get(cur.card)?.destinations : undefined;
    if (-dy > LIFT && destinations?.length) {
      const el = handRef.current?.querySelector<HTMLElement>(`.ah-slot[data-hand-card="${cur.card}"] [data-card-id]`);
      const id = cur.card;
      stop();
      if (el) begin(id, destinations, e.clientX, e.clientY, el, true);
      return;
    }
    // Along the hand: the card under the finger
    if (-dy <= LIFT) {
      const at = cardAt(e.clientX);
      if (at !== null && at !== cur.card) {
        cur.card = at;
        setFocus(at);
      }
    }
  }

  function up(e: PointerEvent) {
    const cur = g.current;
    if (!cur || e.pointerId !== cur.pointer) return;
    const tap = !cur.moved;
    const id = cur.card;
    stop();
    if (tap) live.current.clickCard(id);
  }

  return { focus, peek, onPointerDown };
}
