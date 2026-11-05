import { memo } from 'react';
import { useAff, useCard, useGame, useGameCtx } from '../game/GameContext';
import { useCardPointer } from '../game/useCardPointer';
import { useDragState } from '../game/drag';
import { hasActions } from '../game/decisions';
import { Card, CardBack, type CardSize } from './Card';
import './BoardCard.css';

/**
 * A card of my hand, interactive. Its moves are CSS (the fan slides on the compositor) and the
 * flights of the arena (Web Animations): no JavaScript animation runs on the main thread, and no
 * layout is measured when it renders (a layout animation library measured every card).
 */
export const BoardCard = memo(function BoardCard({
  id,
  size,
  className,
  touchElsewhere = false,
}: {
  id: number;
  size: CardSize;
  className?: string;
  /** Touches are handled by the parent (the hand): only the mouse is handled here */
  touchElsewhere?: boolean;
}) {
  const { def } = useGameCtx('def');
  const card = useCard(id);
  const a = useAff(id);
  const selected = useGame((s) => s.ui.selected === id);
  const focused = useGame((s) => s.focusSource === id);
  const toMana = useGame((s) => s.decision?.kind === 'select' && !!s.decision.toMana);
  const pendingMana = useGame((s) => s.manaPending.has(id));
  const dragged = useDragState((d) => d?.cardId === id);
  const { draggable, handlers } = useCardPointer(id, { touchElsewhere });
  // A card picked for the Mana zone turns face down, as it will be there (and stays so until it gets there)
  const faceDown = pendingMana || (!!a?.picked && toMana);
  const cls = [
    'board-card',
    hasActions(a) ? 'is-playable' : '',
    a?.selectable ? 'is-selectable' : '',
    a?.picked ? 'is-picked' : '',
    pendingMana ? 'is-mana-pending' : '',
    a?.source ? 'is-source' : '',
    focused ? 'is-trigger-focus' : '',
    selected ? 'is-selected' : '',
    dragged ? 'is-dragged' : '',
    card?.exhausted ? 'is-exhausted' : '',
    className ?? '',
  ].join(' ');
  return (
    <div
      className={cls}
      data-card-id={id}
      style={{ touchAction: draggable || touchElsewhere ? 'none' : undefined }}
      {...handlers}
    >
      <div className={`flip ${faceDown ? 'flipped' : ''}`}>
        <Card def={def(card)} card={card} size={size} extraCost={a?.extraCost} className="flip-front" />
        <CardBack size={size} className="flip-back" />
      </div>
      {(a?.support || a?.tap) && (
        <span className="ability-flags">
          {a.support && <span>S</span>}
          {a.tap && <span>T</span>}
        </span>
      )}
    </div>
  );
});
