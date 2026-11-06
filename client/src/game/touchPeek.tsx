import { useEffect, useLayoutEffect, useRef, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { Card } from '../components/Card';
import { CardNotes } from '../components/CardNotes';
import { useCard, useGameCtx } from './GameContext';

/**
 * A card held under a finger on the table, shown large above it while the finger stays, as the
 * cards of the hand are (Hud.tsx, HandPeek). Touch screens have no hover: this is their quick
 * read. The full-screen detail opens on a tap (a card with nothing to do) or on the zoom button of
 * a selected card of the hand.
 */
interface Peek {
  id: number;
  /** Where the card is on the screen, when the finger came down */
  rect: { left: number; top: number; width: number; bottom: number };
}

let peek: Peek | null = null;
const listeners = new Set<() => void>();
const set = (p: Peek | null) => {
  peek = p;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function showTouchPeek(id: number, el: Element) {
  const r = el.getBoundingClientRect();
  set({ id, rect: { left: r.left, top: r.top, width: r.width, bottom: r.bottom } });
}

export function hideTouchPeek() {
  if (peek) set(null);
}

/** Space kept between the card and its large version, and from the edges of the window (px) */
const GAP = 8;

/** The large version of the held card: above it if it fits there, else below it, inside the window */
export function TouchPeek() {
  const p = useSyncExternalStore(subscribe, () => peek);
  // Android's browser takes a finger held on a card for a long press of its own: it vibrates and
  // opens its context menu (cancelled by the cards, but the vibration comes first). Cancelling the
  // touch start on a card stops that gesture; the cards work on pointer events, not on the touch
  // events nor the click this also cancels. Not on the buttons inside a card (their click).
  useEffect(() => {
    const onTouchStart = (e: TouchEvent) => {
      const t = e.target as Element | null;
      if (e.cancelable && t?.closest?.('[data-card-id]') && !t.closest('button')) e.preventDefault();
    };
    document.addEventListener('touchstart', onTouchStart, { passive: false });
    return () => document.removeEventListener('touchstart', onTouchStart);
  }, []);
  if (!p) return null;
  return <PeekCard key={p.id} p={p} />;
}

function PeekCard({ p }: { p: Peek }) {
  const { def } = useGameCtx('def');
  const card = useCard(p.id);
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const box = ref.current;
    if (!box) return;
    const w = box.offsetWidth;
    const h = box.offsetHeight;
    const cx = p.rect.left + p.rect.width / 2;
    const left = Math.max(GAP + w / 2, Math.min(window.innerWidth - GAP - w / 2, cx));
    box.style.left = `${left}px`;
    // Its notes (boosts...) on the side with more room
    box.classList.toggle('notes-left', left > window.innerWidth / 2);
    const above = p.rect.top - GAP - h;
    const below = p.rect.bottom + GAP;
    const top = above >= GAP ? above : below + h <= window.innerHeight - GAP ? below : Math.max(GAP, (window.innerHeight - h) / 2);
    box.style.top = `${top}px`;
    box.style.visibility = '';
  }, [p]);
  const d = def(card);
  if (!d) return null;
  return createPortal(
    <div className="hand-peek touch-peek" ref={ref} style={{ visibility: 'hidden' }} aria-hidden="true">
      <Card def={d} card={card} size="zoom" />
      <CardNotes card={card} className="peek-notes" />
    </div>,
    document.body,
  );
}
