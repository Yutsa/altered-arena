import type { CSSProperties, ReactNode } from 'react';
import type { Destination } from '@protocol';
import { useGame, useGameCtx } from '../game/GameContext';
import { useDragState } from '../game/drag';

const NONE: Destination[] = [];

/** Which destinations are currently offered: by the dragged card, else by the tapped card. */
export function useOfferedDestinations(): { cardId: number | null; destinations: Destination[]; over: Destination | null } {
  const drag = useDragState((d) => d);
  const selected = useGame((s) => s.ui.selected);
  const selectedDest = useGame((s) => (s.ui.selected !== null ? s.aff.get(s.ui.selected)?.destinations : undefined));
  if (drag) return { cardId: drag.cardId, destinations: drag.destinations, over: drag.over };
  if (selected !== null) return { cardId: selected, destinations: selectedDest ?? NONE, over: null };
  return { cardId: null, destinations: NONE, over: null };
}

/** An area a card can be dropped on (drag) or tapped (after tapping a card). */
export function DropZone({
  destination,
  className,
  children,
  label,
  style,
}: {
  destination: Destination;
  className?: string;
  children?: ReactNode;
  label?: string;
  style?: CSSProperties;
}) {
  const { play } = useGameCtx('play');
  const { cardId, destinations, over } = useOfferedDestinations();
  const valid = destinations.includes(destination);
  return (
    <div
      data-drop={valid ? destination : undefined}
      className={`drop-zone ${className ?? ''} ${valid ? 'drop-valid' : ''} ${over === destination ? 'drop-over' : ''}`}
      onClick={valid && cardId !== null ? () => play(cardId, destination) : undefined}
      style={style}
    >
      {children}
      {valid && label && <span className="drop-label">{label}</span>}
    </div>
  );
}
