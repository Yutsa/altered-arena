import { useRef, type MouseEvent, type PointerEvent } from 'react';
import { useGameCtx } from './GameContext';
import { useDrag } from './drag';
import { useInspect } from './inspect';

const DRAG_THRESHOLD = 8;
const LONG_PRESS_MS = 450;
const HOVER_MS = 280;

/**
 * Pointer behaviour of a card on the table:
 * - click / tap: the decision action (select, pick...) or inspect
 * - drag (playable cards): drag and drop onto a destination
 * - long press (touch) or hover (mouse): large readable version
 */
export function useCardPointer(cardId: number) {
  const { aff, clickCard, inspectCard, decision, sending } = useGameCtx();
  const { begin, drag } = useDrag();
  const { hide } = useInspect();
  const st = useRef<{
    x: number;
    y: number;
    id: number;
    dragging: boolean;
    longPressed: boolean;
    timer?: number;
  } | null>(null);
  const hoverTimer = useRef<number | undefined>(undefined);

  const destinations = decision?.kind === 'turn' && !sending ? aff.get(cardId)?.destinations : undefined;
  const draggable = !!destinations?.length;

  const clear = () => {
    if (st.current?.timer) clearTimeout(st.current.timer);
    st.current = null;
  };

  return {
    draggable,
    handlers: {
      onPointerDown(e: PointerEvent) {
        if (e.button !== 0) return;
        clearTimeout(hoverTimer.current);
        const s = { x: e.clientX, y: e.clientY, id: e.pointerId, dragging: false, longPressed: false, timer: undefined as number | undefined };
        if (e.pointerType !== 'mouse') {
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
            hide();
            begin(cardId, destinations!, e.clientX, e.clientY, e.currentTarget as HTMLElement);
          } else if (!s.longPressed) {
            st.current = null; // a scroll gesture, not a tap
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
        if (e.pointerType !== 'mouse' || drag) return;
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
