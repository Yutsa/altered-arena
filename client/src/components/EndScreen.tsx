import { useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import { useGameCtx } from '../game/GameContext';

export function EndScreen({ onLeave }: { onLeave: () => void }) {
  const { view } = useGameCtx();
  const [dismissed, setDismissed] = useState(false);
  const ended = view.phase === 'ended';
  const winner = view.players.find((p) => p.id === view.winner);
  const won = view.winner !== null && view.winner === view.you;
  const title = view.winner === null ? 'Draw' : view.you === null ? `${winner?.name ?? 'Someone'} wins` : won ? 'Victory' : 'Defeat';
  return (
    <AnimatePresence>
      {ended && !dismissed && (
        <motion.div className="end-screen" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
          <motion.div
            className={`end-card ${won ? 'won' : 'lost'}`}
            initial={{ scale: 0.6, rotate: -4 }}
            animate={{ scale: 1, rotate: 0 }}
            transition={{ type: 'spring', stiffness: 260, damping: 18 }}
          >
            <div className="end-title">{title}</div>
            <div className="end-sub">
              {winner ? `${winner.name} won on day ${view.day}.` : 'The game is over.'}
            </div>
            <div className="end-buttons">
              <button type="button" className="btn" onClick={() => setDismissed(true)}>
                View board
              </button>
              <button type="button" className="btn primary" onClick={onLeave}>
                Back to lobby
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
