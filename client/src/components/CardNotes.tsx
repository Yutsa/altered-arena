import type { CardView } from '@protocol';
import { KEYWORDS } from '../game/keywords';
import './CardNotes.css';

/**
 * Beside a zoomed card, what is on it that its printed text does not say, as on BGA: its boosts,
 * with their rule. Nothing when there is nothing to tell.
 */
export function CardNotes({ card, className }: { card?: CardView; className?: string }) {
  const boosts = card?.boosts ?? 0;
  if (!boosts) return null;
  return (
    <div className={`card-notes ${className ?? ''}`}>
      <div className="cn-note">
        <span className="cn-icon">
          <img src="/img/icons/boost.png" alt="" />
          {boosts > 1 && <b>{boosts}</b>}
        </span>
        <span>{KEYWORDS.BB.reminder}</span>
      </div>
    </div>
  );
}

/** Whether CardNotes has something to show */
export const hasCardNotes = (card?: CardView) => (card?.boosts ?? 0) > 0;
