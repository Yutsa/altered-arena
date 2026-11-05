import { memo, useEffect, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'motion/react';
import type { LogEntry, PlayerId } from '@protocol';
import { RichText } from './RichText';
import './Log.css';

/**
 * Game log, newest at the bottom. Its entries are drawn once it has been opened (closed, it is off
 * screen): up to 500 animated lines that a new view would otherwise draw again.
 */
export const Log = memo(function Log({ entries, you, open, onClose }: { entries: LogEntry[]; you: PlayerId | null; open: boolean; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [opened, setOpened] = useState(open);
  if (open && !opened) setOpened(true);
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
        {opened && (
          <AnimatePresence initial={false}>
            {entries.map((e) => (
              <motion.div
                key={e.seq}
                className={`log-entry ${e.player === undefined ? 'neutral' : e.player === you ? 'mine' : 'theirs'}`}
                initial={{ opacity: 0, x: 20 }}
                animate={{ opacity: 1, x: 0 }}
              >
                <LogText text={e.text} />
              </motion.div>
            ))}
          </AnimatePresence>
        )}
      </div>
    </aside>
  );
});

/** The text of an entry, parsed once */
const LogText = memo(function LogText({ text }: { text: string }) {
  return <RichText text={text} />;
});

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
