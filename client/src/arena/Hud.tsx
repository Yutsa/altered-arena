import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { Phase, PlayerView } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { BoardCard } from '../components/BoardCard';
import { CardListModal } from '../components/Picker';
import { DecisionBar } from '../components/DecisionBar';
import { useDrag } from '../game/drag';
import { MatCard } from './MatCard';

const PHASES: { phase: Phase; label: string; icon: string }[] = [
  { phase: 'morning', label: 'Morning', icon: 'dawn' },
  { phase: 'noon', label: 'Noon', icon: 'noon' },
  { phase: 'afternoon', label: 'Afternoon', icon: 'afternoon' },
  { phase: 'dusk', label: 'Dusk', icon: 'dusk' },
  { phase: 'night', label: 'Night', icon: 'night' },
];
export const phaseLabel = (p: Phase) =>
  p === 'setup' ? 'Setup' : p === 'tiebreaker' ? 'Tiebreaker' : p === 'ended' ? 'Game over' : (PHASES.find((x) => x.phase === p)?.label ?? p);

/**
 * Mana of a player, as on BGA: a vertical gauge on the mat, beside the totals of the player's
 * expeditions, one slot per Mana Orb, lit when it is ready. Anchor of the cards put in mana; a
 * click lists them.
 */
export function ManaBar({ player, mine }: { player: PlayerView; mine: boolean }) {
  const { zoneCards } = useGameCtx();
  const [open, setOpen] = useState(false);
  const mana = zoneCards(player.id, 'mana');
  return (
    <>
      <button
        type="button"
        className={`mana-bar ${mine ? 'mine' : 'theirs'}`}
        data-pile={`mana-${player.id}`}
        title={`${player.name}: ${player.manaAvailable} Mana available out of ${player.manaTotal}`}
        onClick={() => mana.length && setOpen(true)}
      >
        <span className="mb-label">Mana</span>
        <b key={`${player.manaAvailable}/${player.manaTotal}`} className="mb-count">
          {player.manaAvailable}
          <small>/{player.manaTotal}</small>
        </b>
        <span className="mb-gauge">
          {Array.from({ length: player.manaTotal }, (_, i) => (
            <span key={i} className={`mb-slot ${i < player.manaAvailable ? 'on' : ''}`} />
          ))}
        </span>
      </button>
      {open && <CardListModal title={`${player.name}: mana`} cardIds={mana.map((c) => c.id)} onClose={() => setOpen(false)} />}
    </>
  );
}

/** Name and state of a player */
export function PlayerChip({ player, mine, className }: { player: PlayerView; mine: boolean; className: string }) {
  const { view, decision, send, you } = useGameCtx();
  const waiting = view.waitingFor.includes(player.id);
  const pickable = decision?.kind === 'player' && decision.players.includes(player.id);
  return (
    <div
      className={`chip player-chip ${className} ${mine ? 'mine' : 'theirs'} ${waiting ? 'waiting' : ''} ${pickable ? 'selectable' : ''}`}
      onClick={pickable ? () => void send({ type: 'player', player: player.id }) : undefined}
    >
      <span className="pc-name">
        {player.name}
        {player.bot && <small> bot</small>}
      </span>
      {mine && you === null && <span className="pc-tag">Hand {player.handCount}</span>}
      {player.passed && <span className="pc-tag">Passed</span>}
      {waiting && !mine && <span className="pc-tag thinking">thinking…</span>}
    </div>
  );
}

/** Day and phase, log, connection, way back */
export function InfoChip({ onLeave, connected, onLog, unread, extra }: { onLeave: () => void; connected: boolean; onLog: () => void; unread: number; extra?: ReactNode }) {
  const { view } = useGameCtx();
  return (
    <div className="chip info-chip">
      <span className="ic-day">Day {view.day}</span>
      <span className="ic-phases">
        {PHASES.map((p) => (
          <img key={p.phase} src={`/img/icons/${p.icon}.png`} alt={p.label} title={p.label} className={p.phase === view.phase ? 'current' : ''} />
        ))}
      </span>
      <span className="ic-phase">{phaseLabel(view.phase)}</span>
      {extra}
      <button type="button" className="ic-btn" onClick={onLog}>
        Log{unread > 0 && <span className="ic-badge">{unread > 99 ? '99+' : unread}</span>}
      </button>
      <span className={`conn ${connected ? 'ok' : 'down'}`} title={connected ? 'Connected' : 'Reconnecting…'} />
      <button type="button" className="ic-btn" onClick={onLeave} title="Back to lobby" aria-label="Back to lobby">
        ✕
      </button>
    </div>
  );
}

/** The decision prompt, with the cards being played (limbo) in front of it. */
export function Prompt() {
  const { view } = useGameCtx();
  const limbo = view.cards.filter((c) => c.zone === 'limbo');
  return (
    <div className="prompt">
      <DecisionBar
        primaryOnTrack
        before={
          limbo.length > 0 && (
            <span className="limbo">
              {limbo.map((c) => (
                <MatCard key={c.id} id={c.id} kind="zone" />
              ))}
            </span>
          )
        }
      />
    </div>
  );
}

/** A short banner when the phase changes ("Day 3 · Morning", "Dusk"...). */
export function PhaseBanner() {
  const { view } = useGameCtx();
  const key = `${view.day}:${view.phase}`;
  const first = useRef(true);
  const [shown, setShown] = useState<string | null>(null);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (view.phase === 'setup' || view.phase === 'ended') return;
    setShown(view.phase === 'morning' ? `Day ${view.day} · Morning` : phaseLabel(view.phase));
    const t = setTimeout(() => setShown(null), 1100);
    return () => clearTimeout(t);
  }, [key, view.day, view.phase]);
  if (!shown) return null;
  return (
    <div className="phase-banner" key={key}>
      <span>{shown}</span>
    </div>
  );
}

/**
 * My hand, fanned at the bottom; cards rise when hovered or tapped. A card being dragged leaves
 * the fan, which closes up (and opens again if the card comes back).
 */
export function ArenaHand() {
  const { me, zoneCards, ui, you } = useGameCtx();
  const { drag } = useDrag();
  if (!me || you === null) return null;
  const cards = zoneCards(me.id, 'hand');
  const out = drag && !drag.returning ? drag.cardId : null;
  const fanned = cards.filter((c) => c.id !== out);
  const n = fanned.length;
  const spread = Math.min(6, 30 / Math.max(n, 1));
  const step = n > 6 ? 0.62 : 0.8;
  return (
    <div className="arena-hand" aria-label="Your hand">
      {cards.map((c, j) => {
        // The card that left keeps the place it had, hidden
        const i = c.id === out ? j : fanned.indexOf(c);
        const off = i - ((c.id === out ? cards.length : n) - 1) / 2;
        return (
          <div
            key={c.id}
            className={`ah-slot ${ui.selected === c.id ? 'lifted' : ''}`}
            style={{
              ['--x' as string]: `calc(var(--card-hand-w) * ${off * step})`,
              ['--rot' as string]: `${off * spread}deg`,
              ['--dy' as string]: `${off * off * 3}px`,
              zIndex: i + 1,
            }}
          >
            <BoardCard id={c.id} size="hand" />
          </div>
        );
      })}
    </div>
  );
}

/** The opponent's hand, face down: a smaller fan hanging from the top edge, with its count. */
export function OppHand({ player }: { player: PlayerView }) {
  const n = player.handCount;
  const shown = Math.min(n, 9);
  const step = shown > 1 ? Math.min(0.42, 2.6 / (shown - 1)) : 0;
  const spread = Math.min(5, 26 / Math.max(shown, 1));
  return (
    <div className="opp-hand" aria-label={`${player.name}: ${n} cards in hand`} title={`${n} cards in hand`}>
      {Array.from({ length: shown }, (_, i) => {
        const off = i - (shown - 1) / 2;
        return (
          <img
            key={i}
            className="oh-card"
            src="/img/card-back.jpg"
            alt=""
            draggable={false}
            style={{
              ['--x' as string]: `calc(var(--opp-card-w) * ${off * step})`,
              ['--rot' as string]: `${-off * spread}deg`,
              ['--dy' as string]: `${-off * off * 2}px`,
            }}
          />
        );
      })}
      <span className="oh-pile" data-pile={`hand-${player.id}`} />
      <b key={n} className="oh-count">
        {n}
      </b>
    </div>
  );
}

/** Phones held upright: the table needs landscape. */
export function RotateHint() {
  return (
    <div className="rotate-hint">
      <div className="rh-phone" />
      <p>Turn your phone to landscape to play.</p>
    </div>
  );
}
