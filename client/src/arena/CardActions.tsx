import { useLayoutEffect, useRef } from 'react';
import { useGameCtx } from '../game/GameContext';
import { playCost } from '../game/decisions';

const ICONS = {
  // A spell cast: a sparkle, a smaller one and the trail it leaves
  cast: 'M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM18.5 15.5l.6 1.7 1.7.6-1.7.6-.6 1.7-.6-1.7-1.7-.6 1.7-.6zM3.5 20.5c2.6-.8 4.6-2.6 5.8-5.2',
  // A card leaving the Reserve, downwards
  support: 'M7 3h8a2 2 0 0 1 2 2v7M5 7v12a2 2 0 0 0 2 2h6M12 14v7M9 18l3 3 3-3',
  // A card turned sideways
  tap: 'M4 9a8 8 0 0 1 14-3M18 3v3h-3M6 13h12a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1z',
  zoom: 'M10.5 4.5a6 6 0 1 0 0 12 6 6 0 1 0 0-12zM15 15l5 5M10.5 8v5M8 10.5h5',
  cancel: 'M6 6l12 12M18 6L6 18',
};

function Icon({ name, size }: { name: keyof typeof ICONS; size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={ICONS[name]} />
    </svg>
  );
}

/**
 * The actions of the card the player tapped during a turn, in an arc above the card: cast a spell,
 * use its support or exhaust ability, zoom, put it back. Playing it into a zone is done on the mat
 * (its zones light up), so they have no button here.
 */
export function CardActions() {
  const { decision, ui, setUI, aff, byId, def, play, send, inspectCard, sending } = useGameCtx('decision', 'ui', 'setUI', 'aff', 'byId', 'def', 'play', 'send', 'inspectCard', 'sending');
  const id = decision?.kind === 'turn' ? ui.selected : null;
  const ref = useRef<HTMLDivElement>(null);

  // Follows the card: it lifts, the hand fans out again, the window is resized
  useLayoutEffect(() => {
    if (id === null) return;
    let raf = 0;
    const follow = () => {
      const el = document.querySelector<HTMLElement>(`.arena [data-card-id="${id}"]`);
      const box = ref.current;
      if (box) {
        const r = el?.getBoundingClientRect();
        if (r && r.width > 2) {
          box.style.left = `${r.left + r.width / 2}px`;
          box.style.top = `${r.top}px`;
          box.style.visibility = '';
        } else box.style.visibility = 'hidden';
      }
      raf = requestAnimationFrame(follow);
    };
    follow();
    return () => cancelAnimationFrame(raf);
  }, [id]);

  if (id === null || decision?.kind !== 'turn') return null;
  const a = aff.get(id);
  const card = byId.get(id);
  const cd = def(card);
  const name = cd?.name ?? 'the card';
  const cast = !!a?.destinations?.includes('spell');
  const cost = cast ? playCost(decision, card, cd, 'spell') : undefined;
  return (
    <div className="card-actions" ref={ref} style={{ visibility: 'hidden' }} role="toolbar" aria-label={`${name}: actions`}>
      <button type="button" className="ca-side" onClick={() => inspectCard(id)} aria-label={`Zoom on ${name}`}>
        <Icon name="zoom" size={20} />
      </button>
      {cast && (
        <button type="button" className="ca-main ca-cast" disabled={sending} onClick={() => void play(id, 'spell')} aria-label={`Cast ${name}${cost !== undefined ? ` for ${cost} mana` : ''}`}>
          <Icon name="cast" size={28} />
          <span>Cast</span>
          {cost !== undefined && <b className="ca-cost">{cost}</b>}
        </button>
      )}
      {a?.support && (
        <button type="button" className="ca-main" disabled={sending} onClick={() => void send({ type: 'support', cardId: id })} aria-label={`Use the support of ${name}`}>
          <Icon name="support" size={26} />
          <span>Support</span>
        </button>
      )}
      {a?.tap && (
        <button type="button" className="ca-main" disabled={sending} onClick={() => void send({ type: 'tap', cardId: id })} aria-label={`Exhaust ${name}`}>
          <Icon name="tap" size={26} />
          <span>Exhaust</span>
        </button>
      )}
      <button type="button" className="ca-side" onClick={() => setUI((u) => ({ ...u, selected: null }))} aria-label="Put the card back">
        <Icon name="cancel" size={16} />
      </button>
    </div>
  );
}
