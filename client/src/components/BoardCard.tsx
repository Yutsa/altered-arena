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
export const BoardCard = memo(function BoardCard({ id, size, className }: { id: number; size: CardSize; className?: string }) {
  const { byId, def, aff, ui, decision } = useGameCtx();
  const { drag } = useDrag();
  const { draggable, handlers } = useCardPointer(id);
  const card = byId.get(id);
  const a = aff.get(id);
  // A card picked for the Mana zone turns face down, as it will be there
  const faceDown = !!a?.picked && decision?.kind === 'select' && !!decision.toMana;
  const cls = [
    'board-card',
    hasActions(a) ? 'is-playable' : '',
    a?.selectable ? 'is-selectable' : '',
    a?.picked ? 'is-picked' : '',
    a?.source ? 'is-source' : '',
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
      style={{ touchAction: draggable ? 'none' : undefined }}
      {...handlers}
    >
      <div className={`flip ${faceDown ? 'flipped' : ''}`}>
        <Card def={def(card)} card={card} size={size} extraCost={a?.extraCost} className="flip-front" />
        <CardBack size={size} className="flip-back" />
      </div>
      {(a?.support || a?.tap) && (
        <span className="ability-flags">
          {a.support && <span title="Support ability available">S</span>}
          {a.tap && <span title="Exhaust ability available">T</span>}
        </span>
      )}
    </motion.div>
  );
});
