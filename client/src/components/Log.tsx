import { useEffect, useRef } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import type { LogEntry, PlayerId } from '@protocol';
import { RichText } from './RichText';
import './Log.css';

/** Game log, newest at the bottom. */
export function Log({ entries, you, open, onClose }: { entries: LogEntry[]; you: PlayerId | null; open: boolean; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries.length, open]);
  return (
    <aside className={`log ${open ? 'open' : ''}`} aria-label="Game log">
      <div className="log-head">
        <span>Log</span>
        <button type="button" className="btn ghost icon" onClick={onClose} aria-label="Close log">
          ✕
        </button>
      </div>
      <div className="log-list" ref={ref}>
        <AnimatePresence initial={false}>
          {entries.map((e) => (
            <motion.div
              key={e.seq}
              className={`log-entry ${e.player === undefined ? 'neutral' : e.player === you ? 'mine' : 'theirs'}`}
              initial={{ opacity: 0, x: 20 }}
              animate={{ opacity: 1, x: 0 }}
            >
              <RichText text={e.text} />
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </aside>
  );
}

/** One-line ticker with the latest entry (phones) */
export function LogTicker({ entry, onOpen, unread }: { entry: LogEntry | undefined; onOpen: () => void; unread: number }) {
  return (
    <button type="button" className="log-ticker" onClick={onOpen}>
      <span className="lt-label">
        Log{unread > 0 && <span className="lt-badge">{unread > 99 ? '99+' : unread}</span>}
      </span>
      <AnimatePresence mode="popLayout" initial={false}>
        {entry && (
          <motion.span key={entry.seq} className="lt-text" initial={{ y: 14, opacity: 0 }} animate={{ y: 0, opacity: 1 }} exit={{ y: -14, opacity: 0 }}>
            <RichText text={entry.text} />
          </motion.span>
        )}
      </AnimatePresence>
    </button>
  );
}
