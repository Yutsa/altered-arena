import type { ReactNode } from 'react';
import type { Destination } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { useDrag } from '../game/drag';

/** Which destinations are currently offered: by the dragged card, else by the tapped card. */
export function useOfferedDestinations(): { cardId: number | null; destinations: Destination[]; over: Destination | null } {
  const { aff, ui } = useGameCtx();
  const { drag } = useDrag();
  if (drag) return { cardId: drag.cardId, destinations: drag.destinations, over: drag.over };
  if (ui.selected !== null) return { cardId: ui.selected, destinations: aff.get(ui.selected)?.destinations ?? [], over: null };
  return { cardId: null, destinations: [], over: null };
}

/** An area a card can be dropped on (drag) or tapped (after tapping a card). */
export function DropZone({
  destination,
  className,
  children,
  label,
}: {
  destination: Destination;
  className?: string;
  children?: ReactNode;
  label?: string;
}) {
  const { play } = useGameCtx();
  const { cardId, destinations, over } = useOfferedDestinations();
  const valid = destinations.includes(destination);
  return (
    <div
      data-drop={valid ? destination : undefined}
      className={`drop-zone ${className ?? ''} ${valid ? 'drop-valid' : ''} ${over === destination ? 'drop-over' : ''}`}
      onClick={valid && cardId !== null ? () => play(cardId, destination) : undefined}
    >
      {children}
      {valid && label && <span className="drop-label">{label}</span>}
    </div>
  );
}
