import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import type { CardView, Phase, PlayerView } from '@protocol';
import { useCard, useGame, useGameCtx, useZone } from '../game/GameContext';
import { BoardCard } from '../components/BoardCard';
import { CardListModal } from '../components/Picker';
import { DecisionBar } from '../components/DecisionBar';
import { useDragState } from '../game/drag';
import { Card } from '../components/Card';
import { CardNotes } from '../components/CardNotes';
import { useHandTouch } from './useHandTouch';

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
 * Mana of a player, as on BGA: a vertical gauge on the mat, left of the player's hero
 * expeditions, one slot per Mana Orb, lit when it is ready. Anchor of the cards put in mana; a
 * click lists them.
 */
/** Orbs the mana gauge draws at full size; beyond, they get thinner to keep its height */
const MANA_ROWS = 8;
/** Orbs in one column at their thinnest (thin bars); beyond, more columns */
const MANA_THIN_ROWS = 24;

export function ManaBar({ player, mine }: { player: PlayerView; mine: boolean }) {
  const mana = useZone(player.id, 'mana');
  const [open, setOpen] = useState(false);
  // Beyond MANA_ROWS orbs the gauge would leave its band: the orbs become thin bars in the same
  // height, then, past MANA_THIN_ROWS, they go on in more columns (as even as possible)
  const cols = Math.max(1, Math.ceil(player.manaTotal / MANA_THIN_ROWS));
  const rows = Math.max(1, Math.ceil(player.manaTotal / cols));
  const thin = rows > MANA_ROWS;
  return (
    <>
      <button
        type="button"
        className={`mana-bar ${mine ? 'mine' : 'theirs'}`}
        data-pile={`mana-${player.id}`}
        aria-label={`${player.name}: ${player.manaAvailable} Mana available out of ${player.manaTotal}`}
        onClick={() => mana.length && setOpen(true)}
      >
        <span className="mb-label">Mana</span>
        <b key={`${player.manaAvailable}/${player.manaTotal}`} className="mb-count">
          {player.manaAvailable}
          <small>/{player.manaTotal}</small>
        </b>
        <span
          className={`mb-gauge ${cols > 1 ? 'multi' : ''} ${thin ? 'thin' : ''}`}
          style={thin ? ({ '--mb-rows': rows, '--mb-full': MANA_ROWS } as CSSProperties) : undefined}
        >
          {Array.from({ length: player.manaTotal }, (_, i) => {
            // Filled column by column, from the mat's edge (bottom for me, top for the opponent)
            const r = i % rows;
            const at = { gridColumn: Math.floor(i / rows) + 1, gridRow: mine ? rows - r : r + 1 };
            return <span key={i} className={`mb-slot ${i < player.manaAvailable ? 'on' : ''}`} style={at} />;
          })}
        </span>
      </button>
      {open && <CardListModal title={`${player.name}: mana`} cardIds={mana.map((c) => c.id)} onClose={() => setOpen(false)} />}
    </>
  );
}

/** Name and state of a player */
export function PlayerChip({ player, mine, className, action }: { player: PlayerView; mine: boolean; className: string; action?: ReactNode }) {
  const { view, decision, send, you } = useGameCtx('view', 'decision', 'send', 'you');
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
      {waiting && !mine && !action && <span className="pc-tag thinking">thinking…</span>}
      {action}
    </div>
  );
}

/**
 * Full screen, on the touch screens that allow it (Android: the address bar takes a sixth of a
 * phone held in landscape). The table is turned to landscape at the same time, where it can be.
 */
const canFullscreen = typeof document !== 'undefined' && !!document.documentElement.requestFullscreen && window.matchMedia('(pointer: coarse)').matches;
function FullscreenButton() {
  const [on, setOn] = useState(() => !!document.fullscreenElement);
  useEffect(() => {
    const f = () => setOn(!!document.fullscreenElement);
    document.addEventListener('fullscreenchange', f);
    return () => document.removeEventListener('fullscreenchange', f);
  }, []);
  if (!canFullscreen) return null;
  const toggle = async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else {
        await document.documentElement.requestFullscreen({ navigationUI: 'hide' });
        await (screen.orientation as ScreenOrientation & { lock?: (o: string) => Promise<void> }).lock?.('landscape').catch(() => {});
      }
    } catch {
      // refused by the browser: nothing to do
    }
  };
  return (
    <button type="button" className="ic-btn ic-full" onClick={() => void toggle()} aria-label={on ? 'Exit full screen' : 'Full screen'}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        {on ? <path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5" /> : <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />}
      </svg>
    </button>
  );
}

/** Day and phase, log, connection, full screen, way back */
export function InfoChip({ onLeave, connected, onLog, unread, extra }: { onLeave: () => void; connected: boolean; onLog: () => void; unread: number; extra?: ReactNode }) {
  const { view } = useGameCtx('view');
  return (
    <div className="chip info-chip">
      <span className="ic-day">Day {view.day}</span>
      <span className="ic-phases">
        {PHASES.map((p) => (
          <img key={p.phase} src={`/img/icons/${p.icon}.png`} alt={p.label} className={p.phase === view.phase ? 'current' : ''} />
        ))}
      </span>
      <span className="ic-phase">{phaseLabel(view.phase)}</span>
      {extra}
      <button type="button" className="ic-btn" onClick={onLog}>
        Log{unread > 0 && <span className="ic-badge">{unread > 99 ? '99+' : unread}</span>}
      </button>
      <span className={`conn ${connected ? 'ok' : 'down'}`} />
      <FullscreenButton />
      <button type="button" className="ic-btn" onClick={onLeave} aria-label="Back to lobby">
        ✕
      </button>
    </div>
  );
}

/** The decision prompt (the card being cast is shown large beside the mat: Cast.tsx) */
export function Prompt() {
  return (
    <div className="prompt">
      <DecisionBar primaryOnTrack only="controls" />
    </div>
  );
}

/**
 * A prompt with nothing to click in it (discard down to 2 in Reserve, pick cards on the board...):
 * on the middle of the Tumult track, clear of the hand and of the zones where the cards are picked.
 * `at`: centre of the track on the stage; `aboveAt`: centre of the opponent's hero, used when the
 * track itself is clicked (Noon of Nadir's terrain markers).
 */
export function TrackNotice({ at, aboveAt }: { at: { x: number; y: number } | null; aboveAt: { x: number; y: number } | null }) {
  const { decision } = useGameCtx('decision');
  // Noon of Nadir: the regions of the track are the targets, the notice goes over the opponent's hero
  const pos = decision?.kind === 'buttons' && decision.markRegion && aboveAt ? aboveAt : at;
  if (!pos) return null;
  return (
    <div className="track-notice" style={{ left: pos.x, top: pos.y }}>
      <DecisionBar primaryOnTrack only="info" />
    </div>
  );
}

/** A short banner when the phase changes ("Day 3 · Morning", "Dusk"...). */
export function PhaseBanner() {
  const { view } = useGameCtx('view');
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

/** Order of the cards in my hand: as drawn (oldest on the left), by type, or by cost */
export type HandSort = 'draw' | 'type' | 'cost';
const HAND_SORTS: { id: HandSort; label: string; short: string; hint: string }[] = [
  { id: 'draw', label: 'Draw order', short: 'Draw', hint: 'Oldest left, newest right' },
  { id: 'type', label: 'Type', short: 'Type', hint: 'Spells, characters, permanents' },
  { id: 'cost', label: 'Cost', short: 'Cost', hint: 'Cheapest first' },
];
export const isHandSort = (s: unknown): s is HandSort => HAND_SORTS.some((o) => o.id === s);
const TYPE_RANK: Record<string, number> = { spell: 0, character: 1, permanent: 2 };

/** Places in the fan, in card widths from its centre: one step per card, no gap between groups */
function fanLayout(n: number) {
  const step = n > 6 ? 0.62 : 0.8;
  const xs = Array.from({ length: n }, (_, i) => i * step);
  const mid = n ? xs[n - 1] / 2 : 0;
  return { xs: xs.map((x) => x - mid), offs: xs.map((x) => (x - mid) / step), half: mid, spread: Math.min(6, 30 / Math.max(n, 1)) };
}

/**
 * The card under the finger in my hand, shown large above it (touch screens have no hover), kept
 * inside the window. Placed once per card, from the card's place at rest (measured when the finger
 * came down, useHandTouch): nothing is measured while the finger slides.
 */
function HandPeek({ id }: { id: number }) {
  const { def } = useGameCtx('def');
  const card = useCard(id);
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const box = ref.current;
    const slot = document.querySelector<HTMLElement>(`.arena-hand .ah-slot[data-hand-card="${id}"]`);
    if (!box || !slot) return;
    const r = slot.dataset.cx && slot.dataset.top ? null : slot.getBoundingClientRect();
    const cx = r ? r.left + r.width / 2 : Number(slot.dataset.cx);
    const top = r ? r.top : Number(slot.dataset.top);
    const w = box.offsetWidth;
    const left = Math.max(8 + w / 2, Math.min(window.innerWidth - 8 - w / 2, cx));
    box.style.left = `${left}px`;
    // Its notes (boosts...) on the side with more room
    box.classList.toggle('notes-left', left > window.innerWidth / 2);
    box.style.bottom = `${Math.max(8, window.innerHeight - top + 6)}px`;
    box.style.visibility = '';
  }, [id]);
  const d = def(card);
  if (!d) return null;
  return createPortal(
    <div className="hand-peek" ref={ref} style={{ visibility: 'hidden' }} aria-hidden="true">
      <Card def={d} card={card} size="zoom" />
      <CardNotes card={card} className="peek-notes" />
    </div>,
    document.body,
  );
}

/**
 * My hand, fanned at the bottom; cards rise when hovered or tapped. On a touch screen the finger
 * slides along it to read the cards, and raises one to drag it (useHandTouch). A card being dragged leaves
 * the fan, which closes up (and opens again if the card comes back). Sorted as the player chose;
 * a big hand closes up rather than reach the controls on its left (`leftBound`).
 */
export function ArenaHand({
  retracted = false,
  inactive = false,
  onEnter,
  onLeave,
  sort = 'draw',
  leftBound,
  stageW,
  cardW,
}: {
  /** Lowered to its top edge (effects to resolve: the hand is not playable) */
  retracted?: boolean;
  inactive?: boolean;
  onEnter?: () => void;
  onLeave?: () => void;
  sort?: HandSort;
  /** Right edge (stage px) of the controls left of the hand, which the fan must not reach */
  leftBound?: number;
  stageW?: number;
  /** Width of a card in the hand (stage px) */
  cardW?: number;
}) {
  const { me, you, def } = useGameCtx('me', 'you', 'def');
  const selected = useGame((s) => s.ui.selected);
  const held = useZone(me?.id, 'hand');
  const out = useDragState((d) => (d && !d.returning ? d.cardId : null));
  const handRef = useRef<HTMLDivElement>(null);
  const touch = useHandTouch(handRef);
  // Draw order: the server lists the hand by card id, so the order of arrival is kept here (the
  // cards held when the table opens come in the server's order)
  const arrival = useRef(new Map<number, number>());
  const arrivals = useRef(0);
  if (!me || you === null) return null;
  const ids = new Set(held.map((c) => c.id));
  for (const id of [...arrival.current.keys()]) if (!ids.has(id)) arrival.current.delete(id);
  for (const c of held) if (!arrival.current.has(c.id)) arrival.current.set(c.id, arrivals.current++);
  const drawn = (c: CardView) => arrival.current.get(c.id) ?? 0;
  const rank = (c: CardView): number => {
    if (sort === 'type') return TYPE_RANK[def(c)?.type ?? ''] ?? 3;
    if (sort === 'cost') return c.costHand ?? def(c)?.costHand ?? 99;
    return 0;
  };
  const cards = [...held].sort((a, b) => rank(a) - rank(b) || drawn(a) - drawn(b));

  const fanned = cards.filter((c) => c.id !== out);
  const fan = fanLayout(fanned.length);
  // The card that left keeps the place it had, hidden
  const whole = out !== null ? fanLayout(cards.length) : fan;
  // Room on the left: the outer card leans out (it turns about a point below it)
  let squeeze = 1;
  if (leftBound !== undefined && stageW && cardW && whole.half > 0) {
    const tilt = (whole.offs[0] ? Math.abs(whole.offs[0]) * whole.spread : 0) * (Math.PI / 180);
    const room = stageW / 2 - leftBound - 24 - cardW / 2 - 1.25 * cardW * (450 / 324) * Math.sin(tilt);
    squeeze = Math.max(0.3, Math.min(1, room / (whole.half * cardW)));
  }
  return (
    <div
      ref={handRef}
      className={`arena-hand ${retracted ? 'retracted' : ''} ${inactive ? 'inactive' : ''} ${touch.focus !== null ? 'touching' : ''}`}
      aria-label="Your hand"
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onPointerDown={touch.onPointerDown}
    >
      {cards.map((c, j) => {
        const l = c.id === out ? whole : fan;
        const i = c.id === out ? j : fanned.indexOf(c);
        const off = l.offs[i];
        return (
          <div
            key={c.id}
            data-hand-card={c.id}
            className={`ah-slot ${selected === c.id ? 'lifted' : ''} ${touch.focus === c.id ? 'focus' : ''}`}
            style={{
              '--x': `calc(var(--card-hand-w) * ${l.xs[i] * squeeze})`,
              '--rot': `${off * l.spread}deg`,
              '--dy': `${off * off * 3}px`,
              zIndex: i + 1,
            }}
          >
            <BoardCard id={c.id} size="hand" touchElsewhere />
          </div>
        );
      })}
      {touch.peek && touch.focus !== null && touch.focus !== out && <HandPeek id={touch.focus} />}
    </div>
  );
}

/**
 * The order of my hand: a round button (the Undo button's look) left of the hand, at a fixed
 * place, the current order written under it; it opens a menu of the three orders.
 */
export function HandSortButton({ sort, onChange }: { sort: HandSort; onChange: (s: HandSort) => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', away);
    document.addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('pointerdown', away);
      document.removeEventListener('keydown', esc);
    };
  }, [open]);
  const current = HAND_SORTS.find((o) => o.id === sort) ?? HAND_SORTS[0];
  const sorted = sort !== 'draw';
  return (
    <div className="hand-sort" ref={ref}>
      {open && (
        <div className="hs-menu" role="menu" aria-label="Sort hand">
          <div className="hs-title">Sort hand</div>
          {HAND_SORTS.map((o) => (
            <button
              key={o.id}
              type="button"
              role="menuitemradio"
              aria-checked={o.id === sort}
              className="hs-opt"
              onClick={() => {
                onChange(o.id);
                setOpen(false);
              }}
            >
              <svg className="hs-check" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                {o.id === sort && <path d="M5 12l5 5L20 7" />}
              </svg>
              <span>
                <span className="hs-label">{o.label}</span>
                <span className="hs-hint">{o.hint}</span>
              </span>
            </button>
          ))}
        </div>
      )}
      <button
        type="button"
        className={`undo-btn hs-btn ${sorted ? 'active' : ''}`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Sort hand: ${current.label}`}
        onClick={() => setOpen((o) => !o)}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M4 6h10" />
          <path d="M4 12h7" />
          <path d="M4 18h4" />
          <path d="M18 5v14" />
          <path d="M15 16l3 3 3-3" />
        </svg>
      </button>
      <span className={`undo-caption hs-caption ${sorted ? 'active' : ''}`} aria-hidden="true">
        {current.short}
      </span>
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
    <div className="opp-hand" aria-label={`${player.name}: ${n} cards in hand`}>
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
              '--x': `calc(var(--opp-card-w) * ${off * step})`,
              '--rot': `${-off * spread}deg`,
              '--dy': `${-off * off * 2}px`,
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
