import { AnimatePresence, motion } from 'motion/react';
import { dismissToast, useToasts } from '../game/toasts';

export function Toasts() {
  const toasts = useToasts();
  return (
    <div className="toasts" aria-live="assertive">
      <AnimatePresence>
        {toasts.map((t) => (
          <motion.button
            type="button"
            key={t.id}
            className={`toast ${t.kind}`}
            initial={{ opacity: 0, y: -20, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -10 }}
            onClick={() => dismissToast(t.id)}
          >
            {t.text}
          </motion.button>
        ))}
      </AnimatePresence>
    </div>
  );
}
