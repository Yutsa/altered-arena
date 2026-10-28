import { useEffect, useRef, useState, type ReactNode } from 'react';
import { motion } from 'motion/react';
import type { PlayerView } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { PlayerBar } from './PlayerBar';
import { ExpeditionRow, ZoneRow } from './Board';
import { Tumult } from './Tumult';
import { Hand } from './Hand';
import { DecisionBar } from './DecisionBar';
import { Log, LogTicker } from './Log';
import { PhaseIndicator } from './PhaseIndicator';
import { EndScreen } from './EndScreen';
import './Table.css';

function Side({ player, mine }: { player: PlayerView; mine: boolean }) {
  return (
    <section className={`side ${mine ? 'mine' : 'theirs'}`} aria-label={mine ? 'Your side' : `${player.name}'s side`}>
      <div className="a-bar">
        <PlayerBar player={player} mine={mine} />
      </div>
      <div className="a-reserve">
        <ZoneRow player={player} zone="reserve" mine={mine} />
      </div>
      <div className="a-landmark">
        <ZoneRow player={player} zone="landmark" mine={mine} />
      </div>
      <div className="a-hero">
        <ExpeditionRow player={player} side="hero" mine={mine} />
      </div>
      <div className="a-companion">
        <ExpeditionRow player={player} side="companion" mine={mine} />
      </div>
    </section>
  );
}

/** The game table: opponent, Tumult, me, hand, decision, log. */
export function Table({ onLeave, connected, extraTop }: { onLeave: () => void; connected: boolean; extraTop?: ReactNode }) {
  const { view, me, opp, log, you } = useGameCtx();
  const [logOpen, setLogOpen] = useState(() => window.matchMedia('(min-width: 1200px)').matches);
  const [seen, setSeen] = useState(0);
  const lastSeq = log.length ? log[log.length - 1].seq : 0;
  useEffect(() => {
    if (logOpen) setSeen(lastSeq);
  }, [logOpen, lastSeq]);
  const unread = log.filter((e) => e.seq > seen).length;
  const scrollRef = useRef<HTMLDivElement>(null);
  return (
    <div className={`game ${logOpen ? 'log-open' : ''}`}>
      <header className="topbar">
        <button type="button" className="btn ghost icon" onClick={onLeave} title="Back to lobby" aria-label="Back to lobby">
          ←
        </button>
        <PhaseIndicator day={view.day} phase={view.phase} />
        {extraTop}
        <span className={`conn ${connected ? 'ok' : 'down'}`} title={connected ? 'Connected' : 'Reconnecting…'} />
        <button type="button" className="btn ghost log-toggle" onClick={() => setLogOpen((o) => !o)} aria-pressed={logOpen}>
          Log{!logOpen && unread > 0 && <span className="lt-badge">{unread > 99 ? '99+' : unread}</span>}
        </button>
      </header>
      <main className="table">
        <motion.div className="board-scroll" ref={scrollRef} layoutScroll>
          <div className="board">
            {opp && <Side player={opp} mine={false} />}
            <Tumult />
            {me && <Side player={me} mine={you !== null} />}
          </div>
        </motion.div>
        <div className="dock">
          <div className="dock-ticker">
            <LogTicker entry={log[log.length - 1]} onOpen={() => setLogOpen(true)} unread={unread} />
          </div>
          <div className="dock-decision">
            <DecisionBar />
          </div>
          {you !== null && (
            <div className="dock-hand">
              <Hand />
            </div>
          )}
        </div>
      </main>
      <Log entries={log} you={you} open={logOpen} onClose={() => setLogOpen(false)} />
      <EndScreen onLeave={onLeave} />
    </div>
  );
}
