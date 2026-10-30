import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { Destination } from '@protocol';
import { useGameCtx } from './GameContext';
import { Card } from '../components/Card';

interface DragState {
  cardId: number;
  destinations: Destination[];
  over: Destination | null;
}

interface DragCtxValue {
  drag: DragState | null;
  begin: (cardId: number, destinations: Destination[], x: number, y: number) => void;
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

/**
 * Hand-written drag and drop on pointer events (mouse, touch, pen): a ghost card follows the
 * pointer, drop zones are found with elementsFromPoint. Positions are written to the DOM directly
 * so that a drag does not re-render the table on every move.
 */
export function DragProvider({ children }: { children: ReactNode }) {
  const { byId, def, play } = useGameCtx();
  const [drag, setDrag] = useState<DragState | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const ghostRef = useRef<HTMLDivElement>(null);
  const pos = useRef({ x: 0, y: 0 });

  const place = () => {
    const g = ghostRef.current;
    if (g) g.style.transform = `translate(${pos.current.x}px, ${pos.current.y}px) translate(-50%, -60%) rotate(-4deg)`;
  };

  const begin = useCallback((cardId: number, destinations: Destination[], x: number, y: number) => {
    pos.current = { x, y };
    const s = { cardId, destinations, over: null };
    dragRef.current = s;
    setDrag(s);
    requestAnimationFrame(place);
    document.body.classList.add('is-dragging');
  }, []);

  const move = useCallback((x: number, y: number) => {
    const s = dragRef.current;
    if (!s) return;
    pos.current = { x, y };
    place();
    const over = dropAt(x, y, s.destinations);
    if (over !== s.over) {
      const n = { ...s, over };
      dragRef.current = n;
      setDrag(n);
    }
  }, []);

  const end = useCallback(
    (drop: boolean) => {
      const s = dragRef.current;
      dragRef.current = null;
      setDrag(null);
      document.body.classList.remove('is-dragging');
      if (drop && s?.over) {
        Object.assign(lastDrop, { cardId: s.cardId, x: pos.current.x, y: pos.current.y, at: performance.now() });
        play(s.cardId, s.over);
      }
    },
    [play],
  );

  const value = useMemo(() => ({ drag, begin, move, end }), [drag, begin, move, end]);
  const card = drag ? byId.get(drag.cardId) : undefined;
  return (
    <DragCtx.Provider value={value}>
      {children}
      {drag &&
        createPortal(
          <div ref={ghostRef} className={`drag-ghost ${drag.over ? 'over' : ''}`}>
            <Card def={def(card)} card={card} size="hand" />
          </div>,
          document.body,
        )}
    </DragCtx.Provider>
  );
}
