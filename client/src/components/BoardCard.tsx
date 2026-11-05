import { memo } from 'react';
import { motion } from 'motion/react';
import { useGameCtx } from '../game/GameContext';
import { useCardPointer } from '../game/useCardPointer';
import { useDrag } from '../game/drag';
import { hasActions } from '../game/decisions';
import { Card, CardBack, type CardSize } from './Card';
import './BoardCard.css';

export const cardTransition = { type: 'spring', stiffness: 420, damping: 36, mass: 0.9 } as const;

/** A card of the game: animated between zones (shared layoutId), interactive. */
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
  const { byId, def, aff, ui, decision, manaPending, focusSource } = useGameCtx();
  const { drag } = useDrag();
  const { draggable, handlers } = useCardPointer(id, { touchElsewhere });
  const card = byId.get(id);
  const a = aff.get(id);
  // A card picked for the Mana zone turns face down, as it will be there (and stays so until it gets there)
  const pendingMana = manaPending.has(id);
  const faceDown = pendingMana || (!!a?.picked && decision?.kind === 'select' && !!decision.toMana);
  const cls = [
    'board-card',
    hasActions(a) ? 'is-playable' : '',
    a?.selectable ? 'is-selectable' : '',
    a?.picked ? 'is-picked' : '',
    pendingMana ? 'is-mana-pending' : '',
    a?.source ? 'is-source' : '',
    focusSource === id ? 'is-trigger-focus' : '',
    ui.selected === id ? 'is-selected' : '',
    drag?.cardId === id ? 'is-dragged' : '',
    card?.exhausted ? 'is-exhausted' : '',
    className ?? '',
  ].join(' ');
  return (
    <motion.div
      layoutId={`card-${id}`}
      layout
      transition={cardTransition}
      initial={{ opacity: 0, scale: 0.85 }}
      animate={{ opacity: 1, scale: 1 }}
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
    </motion.div>
  );
});
