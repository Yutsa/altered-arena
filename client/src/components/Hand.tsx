import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { useGameCtx } from '../game/GameContext';
import { BoardCard } from './BoardCard';
import { CardBack } from './Card';
import './Hand.css';

/** Width of the hand cards, read from the CSS variable --card-hand-w */
function useCardWidth(ref: RefObject<HTMLElement | null>, varName: string): number {
  const [w, setW] = useState(110);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => {
      const probe = document.createElement('div');
      probe.style.width = `var(${varName})`;
      probe.style.position = 'absolute';
      el.appendChild(probe);
      setW(probe.getBoundingClientRect().width || 110);
      probe.remove();
    };
    read();
    window.addEventListener('resize', read);
    return () => window.removeEventListener('resize', read);
  }, [ref, varName]);
  return w;
}

function useWidth(ref: RefObject<HTMLElement | null>): number {
  const [w, setW] = useState(0);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, [ref]);
  return w;
}

/** My hand, fanned at the bottom of the screen. */
export function Hand() {
  const { me, zoneCards, ui } = useGameCtx();
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const cardW = useCardWidth(ref, '--card-hand-w');
  const cards = me ? zoneCards(me.id, 'hand') : [];
  const n = cards.length;
  // Overlap the cards when they do not fit
  const natural = n * cardW + (n - 1) * 6;
  const step = n > 1 && natural > width ? (width - cardW) / (n - 1) : cardW + 6;
  const spread = Math.min(4, 24 / Math.max(n, 1));
  return (
    <div className="hand" ref={ref} aria-label="Your hand">
      <div className="hand-fan" style={{ width: n ? (n - 1) * step + cardW : 0 }}>
        {cards.map((c, i) => {
          const off = i - (n - 1) / 2;
          return (
            <div
              key={c.id}
              className={`hand-slot ${ui.selected === c.id ? 'lifted' : ''}`}
              style={{
                left: i * step,
                zIndex: i + 1,
                ['--rot' as string]: `${off * spread}deg`,
                ['--dy' as string]: `${Math.abs(off) * Math.abs(off) * 1.6}px`,
              }}
            >
              <BoardCard id={c.id} size="hand" />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The opponent's hand: card backs. */
export function OpponentHand({ count }: { count: number }) {
  const shown = Math.min(count, 9);
  return (
    <div className="opp-hand" aria-label={`${count} cards in hand`} title={`${count} cards in hand`}>
      {Array.from({ length: shown }, (_, i) => (
        <div key={i} className="opp-hand-card" style={{ ['--i' as string]: i - (shown - 1) / 2 }}>
          <CardBack size="mini" />
        </div>
      ))}
      <span className="opp-hand-count">{count}</span>
    </div>
  );
}
