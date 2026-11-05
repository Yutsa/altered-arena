import { useRef, type MouseEvent, type PointerEvent } from 'react';
import { useGame, useGameCtx } from './GameContext';
import { holdTouch, useDragActions } from './drag';
import { useInspect } from './inspect';

const DRAG_THRESHOLD = 8;
const LONG_PRESS_MS = 450;
const HOVER_MS = 280;

/**
 * Pointer behaviour of a card on the table:
 * - click / tap: the decision action (select, pick...) or inspect
 * - drag (playable cards): drag and drop onto a destination
 * - long press (touch) or hover (mouse): large readable version
 *
 * `touchElsewhere`: touches are handled by a parent (the hand, which follows the finger across its
 * cards): only the mouse is handled here.
 */
export function useCardPointer(cardId: number, { touchElsewhere = false }: { touchElsewhere?: boolean } = {}) {
  const { clickCard, inspectCard } = useGameCtx('clickCard', 'inspectCard');
  const { begin, current } = useDragActions();
  const { hide } = useInspect();
  const st = useRef<{
    x: number;
    y: number;
    id: number;
    dragging: boolean;
    longPressed: boolean;
    touch: boolean;
    timer?: number;
    release?: () => void;
  } | null>(null);
  const hoverTimer = useRef<number | undefined>(undefined);

  const destinations = useGame((s) => (s.decision?.kind === 'turn' && !s.sending ? s.aff.get(cardId)?.destinations : undefined));
  const draggable = !!destinations?.length;

  const clear = () => {
    if (st.current?.timer) clearTimeout(st.current.timer);
    st.current?.release?.();
    st.current = null;
  };

  return {
    draggable,
    handlers: {
      onPointerDown(e: PointerEvent) {
        if (e.button !== 0) return;
        const touch = e.pointerType !== 'mouse';
        if (touch && touchElsewhere) return;
        clearTimeout(hoverTimer.current);
        clear();
        const s: NonNullable<typeof st.current> = { x: e.clientX, y: e.clientY, id: e.pointerId, dragging: false, longPressed: false, touch };
        // A finger on a card that can be dragged: the page must not scroll under it
        if (touch && draggable) s.release = holdTouch();
        if (touch) {
          s.timer = window.setTimeout(() => {
            if (st.current && !st.current.dragging) {
              st.current.longPressed = true;
              navigator.vibrate?.(15);
              inspectCard(cardId, 'modal');
            }
          }, LONG_PRESS_MS);
        }
        st.current = s;
        if (draggable) (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
      },
      onPointerMove(e: PointerEvent) {
        const s = st.current;
        // While dragging, the drag follows the pointer on the window
        if (!s || s.id !== e.pointerId || s.dragging) return;
        if (Math.hypot(e.clientX - s.x, e.clientY - s.y) > DRAG_THRESHOLD) {
          clearTimeout(s.timer);
          if (draggable && !s.longPressed) {
            s.dragging = true;
            s.release?.();
            s.release = undefined;
            hide();
            begin(cardId, destinations!, e.clientX, e.clientY, e.currentTarget as HTMLElement, s.touch);
          } else if (!s.longPressed) {
            clear(); // a scroll gesture, not a tap
          }
        }
      },
      onPointerUp(e: PointerEvent) {
        const s = st.current;
        if (!s || s.id !== e.pointerId) return;
        clear();
        if (!s.dragging && !s.longPressed) clickCard(cardId);
      },
      onPointerCancel() {
        clear();
      },
      onPointerEnter(e: PointerEvent) {
        if (e.pointerType !== 'mouse' || current()) return;
        const x = e.clientX;
        hoverTimer.current = window.setTimeout(() => inspectCard(cardId, 'hover', x), HOVER_MS);
      },
      onPointerLeave(e: PointerEvent) {
        if (e.pointerType !== 'mouse') return;
        clearTimeout(hoverTimer.current);
        hide(true);
      },
      onContextMenu(e: MouseEvent) {
        e.preventDefault();
      },
    },
  };
}
