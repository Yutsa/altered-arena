/**
 * Card flights: when a new view arrives, every card that changed zone flies, in screen space,
 * from where it was to where it now is (hand -> expedition, expedition -> discard, deck -> hand,
 * opponent's hand -> board...). Hidden places (deck, discard, mana, the opponent's hand) are
 * "piles": elements with data-pile="kind-player". The real card stays hidden until its clone lands.
 *
 * Plain DOM and the Web Animations API: no React state, nothing re-renders during a flight.
 */
import type { CardCatalog, CardView, GameView, PlayerId, Zone } from '@protocol';
import { artUrl } from '../api/catalog';
import { lastDrop } from '../game/drag';

const DURATION = 320;
const STAGGER = 55;
const MAX_FLIGHTS = 10;

interface CardSnap {
  zone: Zone;
  owner: PlayerId;
  ref: string | null;
  rect: DOMRect | null;
}

export interface Snapshot {
  seq: number;
  cards: Map<number, CardSnap>;
  piles: Map<string, DOMRect>;
  hand: Map<PlayerId, number>;
  deck: Map<PlayerId, number>;
}

const ownerOf = (c: CardView) => c.controller ?? c.owner;

function visible(r: DOMRect | null | undefined): r is DOMRect {
  return !!r && r.width > 2 && r.height > 2;
}

function cardEl(root: HTMLElement, id: number): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-card-id="${id}"]`);
}

/** Positions of the cards and piles now on screen. */
export function snapshot(root: HTMLElement, view: GameView): Snapshot {
  const cards = new Map<number, CardSnap>();
  for (const c of view.cards) {
    const el = cardEl(root, c.id);
    cards.set(c.id, { zone: c.zone, owner: ownerOf(c), ref: c.ref, rect: el ? el.getBoundingClientRect() : null });
  }
  const piles = new Map<string, DOMRect>();
  root.querySelectorAll<HTMLElement>('[data-pile]').forEach((el) => piles.set(el.dataset.pile!, el.getBoundingClientRect()));
  return {
    seq: view.seq,
    cards,
    piles,
    hand: new Map(view.players.map((p) => [p.id, p.handCount])),
    deck: new Map(view.players.map((p) => [p.id, p.deckCount])),
  };
}

/** The pile standing for a hidden zone of a player */
function pileOf(s: Snapshot, zone: Zone, owner: PlayerId, you: PlayerId | null): DOMRect | undefined {
  if (zone === 'discard') return s.piles.get(`discard-${owner}`);
  if (zone === 'deck') return s.piles.get(`deck-${owner}`);
  if (zone === 'mana') return s.piles.get(`mana-${owner}`);
  if (zone === 'hand' && owner !== you) return s.piles.get(`hand-${owner}`);
  return undefined;
}

interface Flight {
  from: DOMRect;
  to: DOMRect;
  face: string | null;
  /** the element that stays hidden until the flight lands */
  hide: HTMLElement | null;
  /** fade out on arrival (the card disappears into a pile) */
  intoPile: boolean;
}

/** Starts the flights from the previous snapshot to the new view. Call before paint. */
export function fly(prev: Snapshot, next: Snapshot, view: GameView, root: HTMLElement, layer: HTMLElement, catalog: CardCatalog) {
  const you = view.you;
  const flights: Flight[] = [];
  const face = (ref: string | null) => {
    const d = ref ? catalog[ref] : undefined;
    return d ? artUrl(d) : null;
  };

  for (const c of view.cards) {
    const now = next.cards.get(c.id)!;
    const before = prev.cards.get(c.id);
    if (before && before.zone === now.zone && before.owner === now.owner) continue;
    // Where it comes from
    let from: DOMRect | undefined;
    if (lastDrop.cardId === c.id && performance.now() - lastDrop.at < 4000) {
      const r = now.rect;
      const w = r?.width ?? 60;
      const h = r?.height ?? 84;
      from = new DOMRect(lastDrop.x - w / 2, lastDrop.y - h / 2, w, h);
      lastDrop.cardId = null;
    } else if (before) {
      from = visible(before.rect) ? before.rect : pileOf(prev, before.zone, before.owner, you);
    } else {
      // New to this view: from the deck to my hand, from the opponent's hand to the table
      if (c.zone === 'hand' || c.zone === 'reserve') from = prev.piles.get(`deck-${now.owner}`);
      else if (now.owner !== you) from = prev.piles.get(`hand-${now.owner}`);
    }
    // Where it goes
    const el = visible(now.rect) ? cardEl(root, c.id) : null;
    const to = el ? now.rect! : pileOf(next, now.zone, now.owner, you);
    if (!visible(from) || !visible(to)) {
      if (el && !before) mark(el, 'arena-pop');
      continue;
    }
    flights.push({ from, to, face: face(c.ref ?? before?.ref ?? null), hide: el, intoPile: !el });
  }

  // Cards that left the view (back into a hidden hand or deck)
  for (const [id, before] of prev.cards) {
    if (next.cards.has(id) || !visible(before.rect)) continue;
    const to = before.owner !== you ? next.piles.get(`hand-${before.owner}`) : next.piles.get(`deck-${before.owner}`);
    if (visible(to)) flights.push({ from: before.rect, to, face: face(before.ref), hide: null, intoPile: true });
  }

  // Draws we only know as counts (the opponent's hand)
  for (const p of view.players) {
    if (p.id === you) continue;
    const drawn = Math.min(4, p.handCount - (prev.hand.get(p.id) ?? p.handCount));
    const fromDeck = (prev.deck.get(p.id) ?? p.deckCount) > p.deckCount;
    const from = prev.piles.get(`deck-${p.id}`);
    const to = next.piles.get(`hand-${p.id}`);
    if (drawn > 0 && fromDeck && visible(from) && visible(to))
      for (let i = 0; i < drawn; i++) flights.push({ from, to, face: null, hide: null, intoPile: true });
  }

  flights.slice(0, MAX_FLIGHTS).forEach((f, i) => launch(f, layer, i * STAGGER));
  // Beyond the limit, cards simply appear
  for (const f of flights.slice(MAX_FLIGHTS)) if (f.hide) mark(f.hide, 'arena-pop');
}

/** Plays a one-shot CSS animation class on an element */
function mark(el: HTMLElement, cls: string, ms = 300) {
  el.classList.remove(cls);
  void el.offsetWidth;
  el.classList.add(cls);
  setTimeout(() => el.classList.remove(cls), ms);
}

function launch(f: Flight, layer: HTMLElement, delay: number) {
  const card = document.createElement('div');
  card.className = 'flight';
  if (f.face) card.style.backgroundImage = `url("${f.face}")`;
  else card.classList.add('flight-back');
  // The clone is drawn at its destination and starts transformed back to its origin
  const w = f.intoPile ? Math.min(f.to.width, f.to.height * 0.717) || f.from.width : f.to.width;
  const h = f.intoPile ? w / 0.717 : f.to.height;
  const cx = f.to.left + f.to.width / 2;
  const cy = f.to.top + f.to.height / 2;
  Object.assign(card.style, { left: `${cx - w / 2}px`, top: `${cy - h / 2}px`, width: `${w}px`, height: `${h}px` });
  const dx = f.from.left + f.from.width / 2 - cx;
  const dy = f.from.top + f.from.height / 2 - cy;
  const s = f.from.width / w;
  layer.appendChild(card);
  if (f.hide) f.hide.style.visibility = 'hidden';
  const frames: Keyframe[] = [
    { transform: `translate(${dx}px, ${dy}px) scale(${s}) rotate(-4deg)`, opacity: 1 },
    { transform: `translate(${dx * 0.45}px, ${dy * 0.45 - 18}px) scale(${(s + 1) / 2 + 0.06}) rotate(2deg)`, opacity: 1, offset: 0.5 },
    f.intoPile ? { transform: 'scale(0.9)', opacity: 0 } : { transform: 'none', opacity: 1 },
  ];
  const anim = card.animate(frames, { duration: DURATION, delay, easing: 'cubic-bezier(.3,.7,.3,1)', fill: 'both' });
  const done = () => {
    if (f.hide) {
      f.hide.style.visibility = '';
      mark(f.hide, 'arena-land', 260);
    }
    card.remove();
  };
  anim.onfinish = done;
  anim.oncancel = done;
}
