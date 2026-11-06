import { useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import type { Biome, GameView, PlayerView, Side, Zone } from '@protocol';
import { useCard, useGame, useGameCtx, useZone } from '../game/GameContext';
import { toggleKey } from '../game/decisions';
import { DropZone } from '../components/DropZone';
import { CardListModal } from '../components/Picker';
import { BiomeGlyph } from '../components/Glyph';
import { BIOMES } from '../components/Card';
import { artUrl } from '../api/catalog';
import { MatCard } from './MatCard';
import { deckPatch, HERO_X, rowBackground, type ArenaLayout } from './layout';
import { forecast, regionCenter, STORM_BACK, stormTiles, TRACK_UNITS, type StormTile } from './model';
import { revealedOnTop } from './reveals';
import { useCardPointer } from '../game/useCardPointer';
import { ManaBar } from './Hud';
import { HeroMarkers, RegionMarkers, regionBiomes } from './TerrainMarkers';

/** A location option of the current decision that designates this zone of this player, if any */
function useLocation(player: PlayerView | undefined, zone: Zone) {
  const { decision, send } = useGameCtx('decision', 'send');
  if (!player || decision?.kind !== 'location') return null;
  const loc = decision.locations.find((l) => l.player === player.id && l.zone === zone);
  return loc ? () => void send({ type: 'location', key: loc.key }) : null;
}

/** An expedition option of the current decision, and how to pick it */
function useExpeditionPick(player: PlayerView | undefined, side: Side) {
  const { decision, ui, setUI, send } = useGameCtx('decision', 'ui', 'setUI', 'send');
  if (!player || decision?.kind !== 'expedition') return null;
  const opt = decision.expeditions.find((o) => o.player === player.id && o.side === side);
  if (!opt) return null;
  const picked = ui.keys.includes(opt.key);
  const pick = () => {
    if (decision.min === 1 && decision.max === 1) void send({ type: 'expedition', keys: [opt.key] });
    else setUI((u) => toggleKey(decision, u, opt.key));
  };
  return { key: opt.key, picked, pick };
}

// ---------------------------------------------------------------------------
// Player zones: discard, deck, Reserve, Landmarks
// ---------------------------------------------------------------------------

/** A card revealed on top of the deck: face up there until it is drawn */
function RevealedTop({ id }: { id: number }) {
  const { def } = useGameCtx('def');
  const card = useCard(id);
  const { handlers } = useCardPointer(id);
  const d = def(card);
  return (
    <span className="pile-revealed" data-card-id={id} {...handlers}>
      {d && <img src={artUrl(d)} alt={d.name} draggable={false} />}
      <span className="pile-tag">Revealed</span>
    </span>
  );
}

function Piles({ player }: { player: PlayerView }) {
  const { def } = useGameCtx('def');
  const revealed = useGame((s) => revealedOnTop(s.view).get(player.id));
  const [open, setOpen] = useState(false);
  const discard = useZone(player.id, 'discard');
  const top = discard.at(-1);
  const topDef = def(top);
  return (
    <div className="zone-piles">
      <button
        type="button"
        className="pile pile-discard"
        data-pile={`discard-${player.id}`}
        onClick={() => discard.length && setOpen(true)}
        aria-label={`Discard: ${player.discardCount}`}
      >
        {topDef ? <img src={artUrl(topDef)} alt="" draggable={false} /> : <span className="pile-empty" />}
        <b key={player.discardCount} className="pile-count">
          {player.discardCount}
        </b>
      </button>
      <div className="pile pile-deck" data-pile={`deck-${player.id}`}>
        {player.deckCount > 0 ? <img src="/img/card-back.jpg" alt="" draggable={false} /> : <span className="pile-empty" />}
        {revealed !== undefined && <RevealedTop id={revealed} />}
        <b key={player.deckCount} className="pile-count">
          {player.deckCount}
        </b>
      </div>
      {open && <CardListModal title={`${player.name}: discard`} cardIds={discard.map((c) => c.id)} onClose={() => setOpen(false)} />}
    </div>
  );
}

function ZoneSlots({ player, zone, mine, place }: { player: PlayerView; zone: 'reserve' | 'landmark'; mine: boolean; place: ZonePlace }) {
  const cards = useZone(player.id, zone);
  const slots = zone === 'reserve' ? player.reserveSlots : player.landmarkSlots;
  const pickLocation = useLocation(player, zone);
  const body = (
    <>
      <span className="zone-label">
        {zone === 'reserve' ? 'Reserve' : 'Landmarks'} {cards.length}/{slots}
      </span>
      <div className={`zone-cards ${place.step !== undefined ? 'packed' : ''}`} style={place.step !== undefined ? ({ '--zone-step': `${place.step}px` } as CSSProperties) : undefined}>
        {cards.map((c) => (
          <MatCard key={c.id} id={c.id} kind="zone" />
        ))}
        {Array.from({ length: Math.max(0, slots - cards.length) }, (_, i) => (
          <span key={`e${i}`} className={`zone-slot ${zone === 'landmark' ? 'slot-landmark' : ''}`} />
        ))}
      </div>
      {pickLocation && <button type="button" className="pick-overlay" onClick={pickLocation} aria-label={`Choose ${zone}`} />}
    </>
  );
  const cls = `zone-group zone-${zone} ${pickLocation ? 'selectable' : ''}`;
  return mine ? (
    <DropZone destination={zone} className={cls} label={zone === 'reserve' ? 'Reserve' : 'Landmarks'} style={place.style}>
      {body}
    </DropZone>
  ) : (
    <div className={cls} style={place.style}>
      {body}
    </div>
  );
}

/** Where a zone group is (left edge of the Reserve, right edge of the Landmarks) and, when its cards overlap, their step */
type ZonePlace = { style: CSSProperties; step?: number };

const ZONE_GAP = 8;
/**
 * Left edge of the Reserve and right edge of the Landmarks, as fractions of the mat width. Mine are
 * against my piles and near the right edge (the main button is above them, under the track); the
 * opponent's start at 25% and stop at 13% (24% on phones, clear of the info chip).
 */
const reserveLeft = (l: ArenaLayout, row: 0 | 4) => (row === 0 ? 0.25 : l.mode === 'phone' ? 0.19 : 0.225);
const landmarkRight = (l: ArenaLayout, row: 0 | 4) => (row === 0 ? (l.mode === 'phone' ? 0.24 : 0.13) : l.mode === 'phone' ? 0.12 : 0.105);

/**
 * Reserve and Landmarks share the row between the piles and the right edge. When their cards do
 * not fit side by side (ten in each, rare but possible), each group gets a width in proportion to
 * its cards and they overlap to fit in it. My cards are larger than the opponent's (--my-zone-card-w).
 */
function zonePlaces(l: ArenaLayout, row: 0 | 4, nReserve: number, nLandmarks: number): [ZonePlace, ZonePlace] {
  const left = reserveLeft(l, row);
  const right = landmarkRight(l, row);
  const reserve: ZonePlace = { style: { left: `${left * 100}%` } };
  const landmarks: ZonePlace = { style: { right: `${right * 100}%` } };
  const cw = parseFloat(l.vars[row === 0 ? '--zone-card-w' : '--my-zone-card-w']);
  // Padding of both groups (4px each side) and a margin between them
  const avail = (1 - left - right) * l.plane.w - 16 - 3 * ZONE_GAP;
  const width = (n: number) => n * cw + Math.max(0, n - 1) * ZONE_GAP;
  if (width(nReserve) + width(nLandmarks) <= avail) return [reserve, landmarks];
  const step = (n: number) => (n > 1 ? Math.min(cw + ZONE_GAP, (avail * (n / (nReserve + nLandmarks)) - cw) / (n - 1)) : undefined);
  return [
    { ...reserve, step: step(nReserve) },
    { ...landmarks, step: step(nLandmarks) },
  ];
}

function MatZone({ l, row, player, mine }: { l: ArenaLayout; row: 0 | 4; player: PlayerView | undefined; mine: boolean }) {
  const nReserve = useZone(player?.id, 'reserve').length;
  const nLandmarks = useZone(player?.id, 'landmark').length;
  const n = (count: number, slots: number) => (player ? Math.max(count, slots) : 0);
  const [reserve, landmarks] = zonePlaces(l, row, n(nReserve, player?.reserveSlots ?? 0), n(nLandmarks, player?.landmarkSlots ?? 0));
  return (
    <div className={`mat-row mat-zone ${row === 0 ? 'zone-top' : 'zone-bottom'}`} style={rowBackground(l, row)}>
      <div className="mat-patch" style={deckPatch(l, row)} />
      {player && (
        <>
          <Piles player={player} />
          <ZoneSlots player={player} zone="reserve" mine={mine} place={reserve} />
          <ZoneSlots player={player} zone="landmark" mine={mine} place={landmarks} />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Expedition bands: Mana, hero expedition, hero card, companion expedition
// ---------------------------------------------------------------------------

function ExpCell({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const zone: Zone = side === 'hero' ? 'heroExpedition' : 'companionExpedition';
  const cards = useZone(player.id, zone);
  const pickExp = useExpeditionPick(player, side);
  const pickLocation = useLocation(player, zone);
  const pick = pickExp?.pick ?? pickLocation;
  const ascended = !!player.expeditions[side].ascended;
  const body = (
    <>
      {ascended && <ExpSky />}
      <div className="exp-cards">
        {cards.map((c) => (
          <span key={c.id} className="exp-slot">
            <MatCard id={c.id} kind="exp" />
          </span>
        ))}
      </div>
      {mine && (
        <DropZone
          destination={side === 'hero' ? 'heroExpeditionScout' : 'companionExpeditionScout'}
          className="scout-zone"
          label="Scout"
        />
      )}
      {pick && (
        <button
          type="button"
          className={`pick-overlay ${pickExp?.picked ? 'picked' : ''}`}
          data-exp-key={pickExp?.key}
          onClick={pick}
          aria-label={`Choose the ${side} expedition`}
        />
      )}
    </>
  );
  const cls = `exp-cell exp-${side} ${pick ? 'selectable' : ''} ${pickExp?.picked ? 'picked' : ''} ${ascended ? 'is-ascended' : ''}`;
  return mine ? (
    <DropZone destination={zone} className={cls}>
      {body}
    </DropZone>
  ) : (
    <div className={cls}>{body}</div>
  );
}

/** A pair of wings: the mark of an Ascended expedition */
export function Wings({ className }: { className?: string }) {
  const wing = 'M24 15C19 6 10 2 1 3c3 2 5 4 7 6-3 0-5 1-6 3 3 1 6 1 9 1-2 1-4 3-4 5 5 1 12 0 17-3z';
  return (
    <svg className={className} viewBox="0 0 64 26" aria-hidden="true">
      <path d={wing} />
      <path d={wing} transform="translate(64 0) scale(-1 1)" />
    </svg>
  );
}

/**
 * An Ascended expedition (Soar...) takes to the sky until Rest: its zone turns to sky with clouds
 * along its bottom, its cards float above their shadows, and a ribbon names the state.
 */
function ExpSky() {
  return (
    <>
      <span className="exp-sky" aria-hidden="true">
        <i />
        <i />
        <i />
        <i />
        <i />
      </span>
      <span className="exp-ribbon">
        <Wings />
        Ascended <small>until Rest</small>
      </span>
    </>
  );
}

/** The hero card, with the first player token on it as on BGA */
function HeroCell({ player }: { player: PlayerView }) {
  const { view } = useGameCtx('view');
  return (
    <div className="hero-cell">
      {player.heroCardId !== null && <MatCard id={player.heroCardId} kind="hero" />}
      <HeroMarkers player={player} />
      {view.firstPlayer === player.id && (
        <img key={`${view.day}-${player.id}`} className="first-token" src="/img/icons/first-player.png" alt="First player" draggable={false} />
      )}
    </div>
  );
}

function MatBand({ l, row, player, mine }: { l: ArenaLayout; row: 1 | 3; player: PlayerView | undefined; mine: boolean }) {
  return (
    <div className={`mat-row mat-band ${mine ? 'band-mine' : 'band-theirs'}`} style={rowBackground(l, row)}>
      {player && (
        <>
          <span />
          <div className="mana-cell">
            <ManaBar player={player} mine={mine} />
          </div>
          <ExpCell player={player} side="hero" mine={mine} />
          <HeroCell player={player} />
          <ExpCell player={player} side="companion" mine={mine} />
          <span />
          <span />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The Tumult track
// ---------------------------------------------------------------------------

/** One face of a Tumult card, drawn from the storm.jpg sprite and cropped to the track height */
function stormFace(id: number, u: number, h: number): CSSProperties {
  // 1 px wider than its place: the next card covers its edge, which the scaling leaves half
  // transparent (the widths are fractional): without it a thin line of the mat shows between them
  const w = 2 * u + 1;
  const tileH = (w * 372) / 520;
  const y = -(id * tileH + (tileH - h) / 2);
  return { backgroundSize: `${w}px auto`, backgroundPosition: `0 ${y}px` };
}

/** A Tumult card turning face up: it flips as a card picked for Mana, on its side */
const STORM_FLIP_MS = 550;

/**
 * One Tumult card of the track, with its two faces. When it turns face up (a region revealed),
 * it flips from its back to its face; the cards already face up when the game opens do not.
 */
function Tile({ t, u, h, at }: { t: StormTile; u: number; h: number; at: number }) {
  const card = useRef<HTMLSpanElement>(null);
  const prev = useRef(t.id);
  // In 3D only while it turns: two 3D faces at rest are drawn on layers of their own, whose
  // rounding leaves a thin gap between the cards of the track
  const [turning, setTurning] = useState(false);
  useLayoutEffect(() => {
    const was = prev.current;
    prev.current = t.id;
    if (was !== STORM_BACK || t.id === STORM_BACK || !card.current) return;
    setTurning(true);
    // The card lies on its side on the track: it turns around the track's axis, its far edge
    // coming over towards the player
    const anim = card.current.animate([{ transform: 'rotateX(-180deg)' }, { transform: 'rotateX(0deg)' }], {
      duration: STORM_FLIP_MS,
      easing: 'cubic-bezier(0.3, 0.9, 0.3, 1.15)',
    });
    anim.onfinish = anim.oncancel = () => setTurning(false);
  }, [t.id]);
  return (
    <span className={`storm-tile ${turning ? 'turning' : ''}`} style={{ width: 2 * u + 1, height: h, left: at * u }} aria-hidden="true">
      <span className="storm-card" ref={card}>
        {/* A card turned by 180° (its two regions swapped) turns its face only, not the flip */}
        <span className="storm-face" style={{ ...stormFace(t.id, u, h), transform: t.flipped ? 'rotate(180deg)' : undefined }} />
        {turning && <span className="storm-face storm-back" style={stormFace(STORM_BACK, u, h)} />}
      </span>
    </span>
  );
}

/** Shift of a token whose region also holds the other player's: side by side, the opponent's on the left */
const SHARED_SHIFT = 0.2;

/**
 * On one line (phones), a token whose region also holds the other player's moves aside; on two lines
 * each player has a line of their own. A terrain marker never moves the tokens (RegionMarkers places
 * it clear of them): moved, the tokens of region 0 or 7 went under the totals post.
 */
function shiftAt(view: GameView, player: PlayerView, region: number, mine: boolean, twoLanes: boolean) {
  const o = view.players.find((p) => p.id !== player.id);
  const shared = !twoLanes && !!o && (o.expeditions.hero.position === region || o.expeditions.companion.position === region);
  return shared ? (mine ? SHARED_SHIFT : -SHARED_SHIFT) : 0;
}

function Marker({ player, side, mine, other, twoLanes }: { player: PlayerView; side: Side; mine: boolean; other: boolean; twoLanes: boolean }) {
  const { view } = useGameCtx('view');
  const pick = useExpeditionPick(player, side);
  const exp = player.expeditions[side];
  // Both expeditions of a player in the same region: side by side, each on its way (the hero one
  // goes right); see shiftAt for the other player and the terrain marker
  const dx = (other ? (side === 'hero' ? 0.22 : -0.22) : 0) + shiftAt(view, player, exp.position, mine, twoLanes);
  // As on BGA: an arrow on the side it would move to, at Dusk as things stand
  const moving = forecast(view, player, side).advances && view.phase !== 'night' && view.phase !== 'setup';
  // Where it is on the track (a fraction of its width): a translate, which the compositor animates
  const style: CSSProperties = { '--at': (regionCenter(exp.position) + dx) / TRACK_UNITS };
  return (
    <button
      type="button"
      className={`marker ${mine ? 'mine' : 'theirs'} ${pick ? 'selectable' : ''} ${pick?.picked ? 'picked' : ''} ${exp.blocked ? 'blocked' : ''} ${exp.ascended ? 'ascended' : ''}`}
      style={style}
      onClick={pick?.pick}
      disabled={!pick}
      aria-label={`${player.name}: ${side} expedition, region ${exp.position}${exp.blocked ? ' (blocked)' : ''}${exp.ascended ? ' (Ascended)' : ''}${moving ? ', moves forward at Dusk' : ''}`}
    >
      {player.faction ? <img src={`/img/tokens/${player.faction}-${side}.png`} alt="" draggable={false} /> : <span>{side[0].toUpperCase()}</span>}
      {moving && <span className={`marker-arrow ${side}`} aria-hidden="true" />}
    </button>
  );
}

function Ghost({ player, side, mine, twoLanes }: { player: PlayerView; side: Side; mine: boolean; twoLanes: boolean }) {
  const { view } = useGameCtx('view');
  const f = forecast(view, player, side);
  if (!f.advances || view.phase === 'night' || view.phase === 'setup') return null;
  const next = Math.max(0, Math.min(7, f.next));
  const dx = shiftAt(view, player, next, mine, twoLanes);
  return (
    <span
      className={`ghost-marker ${mine ? 'mine' : 'theirs'}`}
      style={{ '--at': (regionCenter(next) + dx) / TRACK_UNITS }}
    />
  );
}

/**
 * Centre of the totals post on each end of the track, in track units from that end: on the cream
 * part of the end card, between the hero (companion) emblem and region 0 (7), so both stay visible.
 * The emblem reaches about 0.37 unit from the end and the tokens of region 0 (7) start near 1.36:
 * the larger post of the desktop track (two lines, 0.78 unit wide) is centred between them.
 */
const POST_X = 0.715;
const POST_X_WIDE = 0.86;

/** One player's statistics in a post: a cell per biome, dimmed out of their region, lit where they win */
function PostRow({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const { view } = useGameCtx('view');
  const exp = player.expeditions[side];
  const f = forecast(view, player, side);
  return (
    <>
      {BIOMES.map((b: Biome) => {
        const inRegion = !f.region || f.region.includes(b);
        const win = f.winning.includes(b);
        const cls = `post-num ${inRegion ? '' : 'dim'} ${win ? `win ${mine ? 'me' : 'op'}` : ''} ${exp.blocked ? 'blocked' : ''}`;
        return (
          <b key={`${b}-${exp.stats[b]}`} className={cls}>
            {exp.stats[b]}
          </b>
        );
      })}
    </>
  );
}

/**
 * The statistics of both expeditions of one side, as a post at that end of the track: the
 * opponent's on top, the biomes, mine below, so each biome reads as one column.
 */
function TotalsPost({ side, u, x }: { side: Side; u: number; x: number }) {
  const { me, opp, you } = useGameCtx('me', 'opp', 'you');
  if (!me || !opp) return null;
  const mineId = you ?? me.id;
  const label = (p: PlayerView) =>
    `${p.name}: ${BIOMES.map((b) => `${b} ${p.expeditions[side].stats[b]}`).join(', ')}${p.expeditions[side].blocked ? ' (blocked)' : ''}`;
  return (
    <div
      className={`totals-post post-${side}`}
      style={{ left: (side === 'hero' ? x : TRACK_UNITS - x) * u }}
      role="group"
      aria-label={`${side === 'hero' ? 'Hero' : 'Companion'} expeditions. ${label(opp)}. ${label(me)}.`}
    >
      <PostRow player={opp} side={side} mine={opp.id === mineId} />
      {BIOMES.map((b: Biome) => (
        <span key={b} className={`tot-${b}`}>
          <span className="tot-icon">
            <BiomeGlyph biome={b} />
          </span>
        </span>
      ))}
      <PostRow player={me} side={side} mine={me.id === mineId} />
    </div>
  );
}

function MatTrack({ l }: { l: ArenaLayout }) {
  const { view, me, opp, you } = useGameCtx('view', 'me', 'opp', 'you');
  const trackW = l.plane.w * 0.88;
  const u = trackW / TRACK_UNITS;
  const h = l.trackH;
  const t = stormTiles(view.regions);
  const regions = [...view.regions].sort((a, b) => a.index - b.index);
  const players = [opp, me].filter((p): p is PlayerView => !!p);
  return (
    <div className="mat-row mat-trackrow" style={rowBackground(l, 2)}>
      <div className={`track ${l.twoLanes ? 'two-lanes' : ''}`} style={{ width: trackW, height: h, left: HERO_X * l.plane.w - (TRACK_UNITS / 2) * u }}>
        <DropZone destination="spell" className="track-drop">
          <Tile key={0} t={t[0]} u={u} h={h} at={0} />
          <Tile key={1} t={t[1]} u={u} h={h} at={2} />
          <Tile key={2} t={t[2]} u={u} h={h} at={4} />
          <Tile key={3} t={t[3]} u={u} h={h} at={6} />
          <Tile key={4} t={t[4]} u={u} h={h} at={8} />
          <RegionMarkers regions={regions} u={u} twoLanes={l.twoLanes} />
          {players.map((p) => {
            const mine = p.id === (you ?? me?.id);
            const same = p.expeditions.hero.position === p.expeditions.companion.position;
            return (
              <span key={p.id} className={`track-lane ${mine ? 'lane-mine' : 'lane-theirs'}`}>
                <Ghost player={p} side="hero" mine={mine} twoLanes={l.twoLanes} />
                <Ghost player={p} side="companion" mine={mine} twoLanes={l.twoLanes} />
                <Marker player={p} side="hero" mine={mine} other={same} twoLanes={l.twoLanes} />
                <Marker player={p} side="companion" mine={mine} other={same} twoLanes={l.twoLanes} />
              </span>
            );
          })}
        </DropZone>
        {regions.flatMap((r) => {
          const biomes = regionBiomes(r);
          // Face down: no biomes to show. Marked, on one line: the terrain marker is where the chips go
          if (!biomes?.length || (r.marker && !l.twoLanes)) return [];
          // On two lines, at both edges of the track (by each player's line); on one, at the bottom
          return (l.twoLanes ? ['top', 'bottom'] : ['bottom']).map((edge) => (
            <span key={`${r.index}-${edge}`} className={`region-chips chips-${edge}`} style={{ left: regionCenter(r.index) * u }}>
              {biomes.map((b) => (
                <span key={b} className={`tot-${b}`}>
                  <span className="tot-icon">
                    <BiomeGlyph biome={b} />
                  </span>
                </span>
              ))}
            </span>
          ));
        })}
        <TotalsPost side="hero" u={u} x={l.twoLanes ? POST_X_WIDE : POST_X} />
        <TotalsPost side="companion" u={u} x={l.twoLanes ? POST_X_WIDE : POST_X} />
      </div>
    </div>
  );
}

/** Over the opponent's side of the mat once they have passed (as on BGA): darker and duller */
function PassedVeil({ l, player }: { l: ArenaLayout; player: PlayerView | undefined }) {
  if (!player?.passed) return null;
  return <div className="passed-veil" style={{ height: l.rows[0] + l.rows[1] }} />;
}

/** The tilted mat: opponent zone, opponent expeditions, Tumult, my expeditions, my zone. */
export function Mat({ l }: { l: ArenaLayout }) {
  const { me, opp, you } = useGameCtx('me', 'opp', 'you');
  const mine = you !== null;
  return (
    <>
      <MatZone l={l} row={0} player={opp} mine={false} />
      <MatBand l={l} row={1} player={opp} mine={false} />
      <PassedVeil l={l} player={opp} />
      <MatTrack l={l} />
      <MatBand l={l} row={3} player={me} mine={mine} />
      <MatZone l={l} row={4} player={me} mine={mine} />
    </>
  );
}
