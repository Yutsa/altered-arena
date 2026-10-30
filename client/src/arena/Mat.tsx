import { useState, type CSSProperties } from 'react';
import type { Biome, PlayerView, Side, Zone } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { toggleKey } from '../game/decisions';
import { DropZone } from '../components/DropZone';
import { CardListModal } from '../components/Picker';
import { BiomeGlyph } from '../components/Glyph';
import { BIOMES } from '../components/Card';
import { artUrl } from '../api/catalog';
import { MatCard } from './MatCard';
import { deckPatch, HERO_X, rowBackground, type ArenaLayout } from './layout';
import { forecast, regionCenter, STORM_BACK, stormTiles, TRACK_UNITS, type StormTile } from './model';
import { TrackButton } from './TrackButton';
import { ManaBar } from './Hud';
import { HeroMarkers, MARKED_SHIFT, RegionMarkers, regionBiomes } from './TerrainMarkers';

/** A location option of the current decision that designates this zone of this player, if any */
function useLocation(player: PlayerView | undefined, zone: Zone) {
  const { decision, send } = useGameCtx();
  if (!player || decision?.kind !== 'location') return null;
  const loc = decision.locations.find((l) => l.player === player.id && l.zone === zone);
  return loc ? () => void send({ type: 'location', key: loc.key }) : null;
}

/** An expedition option of the current decision, and how to pick it */
function useExpeditionPick(player: PlayerView | undefined, side: Side) {
  const { decision, ui, setUI, send } = useGameCtx();
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

function Piles({ player }: { player: PlayerView }) {
  const { zoneCards, def } = useGameCtx();
  const [open, setOpen] = useState(false);
  const discard = zoneCards(player.id, 'discard');
  const top = discard.at(-1);
  const topDef = def(top);
  return (
    <div className="zone-piles">
      <button
        type="button"
        className="pile pile-discard"
        data-pile={`discard-${player.id}`}
        onClick={() => discard.length && setOpen(true)}
        title={`Discard: ${player.discardCount}`}
      >
        {topDef ? <img src={artUrl(topDef)} alt="" draggable={false} /> : <span className="pile-empty" />}
        <b key={player.discardCount} className="pile-count">
          {player.discardCount}
        </b>
      </button>
      <div className="pile pile-deck" data-pile={`deck-${player.id}`} title={`Deck: ${player.deckCount}`}>
        {player.deckCount > 0 ? <img src="/img/card-back.jpg" alt="" draggable={false} /> : <span className="pile-empty" />}
        <b key={player.deckCount} className="pile-count">
          {player.deckCount}
        </b>
      </div>
      {open && <CardListModal title={`${player.name}: discard`} cardIds={discard.map((c) => c.id)} onClose={() => setOpen(false)} />}
    </div>
  );
}

function ZoneSlots({ player, zone, mine }: { player: PlayerView; zone: 'reserve' | 'landmark'; mine: boolean }) {
  const { zoneCards } = useGameCtx();
  const cards = zoneCards(player.id, zone);
  const slots = zone === 'reserve' ? player.reserveSlots : player.landmarkSlots;
  const pickLocation = useLocation(player, zone);
  const body = (
    <>
      <span className="zone-label">
        {zone === 'reserve' ? 'Reserve' : 'Landmarks'} {cards.length}/{slots}
      </span>
      <div className="zone-cards">
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
    <DropZone destination={zone} className={cls} label={zone === 'reserve' ? 'Reserve' : 'Landmarks'}>
      {body}
    </DropZone>
  ) : (
    <div className={cls}>{body}</div>
  );
}

function MatZone({ l, row, player, mine }: { l: ArenaLayout; row: 0 | 4; player: PlayerView | undefined; mine: boolean }) {
  return (
    <div className={`mat-row mat-zone ${row === 0 ? 'zone-top' : 'zone-bottom'}`} style={rowBackground(l, row)}>
      <div className="mat-patch" style={deckPatch(l, row)} />
      {player && (
        <>
          <Piles player={player} />
          <ZoneSlots player={player} zone="reserve" mine={mine} />
          <ZoneSlots player={player} zone="landmark" mine={mine} />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Expedition bands: totals, hero expedition, hero card, companion expedition, totals
// ---------------------------------------------------------------------------

function Totals({ player, side }: { player: PlayerView; side: Side }) {
  const { view, you } = useGameCtx();
  const exp = player.expeditions[side];
  const f = forecast(view, player, side);
  const who = player.id === you ? 'me' : 'op';
  const arrow = side === 'hero' ? '▶' : '◀';
  return (
    <div className={`totals totals-${side} ${exp.blocked ? 'blocked' : ''}`} title={exp.blocked ? 'Blocked' : undefined}>
      {BIOMES.map((b: Biome) => {
        const inRegion = !f.region || f.region.includes(b);
        const win = f.winning.includes(b);
        return (
          <span key={b} className={`tot tot-${b} ${inRegion ? '' : 'dim'}`}>
            <span className="tot-icon">
              <BiomeGlyph biome={b} />
            </span>
            <b key={exp.stats[b]} className="tot-num">
              {exp.stats[b]}
            </b>
            <span className={`tot-arrow ${win ? `win ${who}` : ''}`}>{arrow}</span>
          </span>
        );
      })}
      {exp.blocked && <span className="tot-blocked">Blocked</span>}
    </div>
  );
}

function ExpCell({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const { zoneCards } = useGameCtx();
  const zone: Zone = side === 'hero' ? 'heroExpedition' : 'companionExpedition';
  const cards = zoneCards(player.id, zone);
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
    <DropZone destination={zone} className={cls} label={side === 'hero' ? 'Hero expedition' : 'Companion expedition'}>
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
      <span className="exp-ribbon" title="Ascended: until Rest, this expedition moves forward even when the opposing one matches it">
        <Wings />
        Ascended <small>until Rest</small>
      </span>
    </>
  );
}

/** The hero card, with the first player token on it as on BGA */
function HeroCell({ player }: { player: PlayerView }) {
  const { view } = useGameCtx();
  return (
    <div className="hero-cell">
      {player.heroCardId !== null && <MatCard id={player.heroCardId} kind="hero" />}
      <HeroMarkers player={player} />
      {view.firstPlayer === player.id && (
        <img key={`${view.day}-${player.id}`} className="first-token" src="/img/icons/first-player.png" alt="First player" title="First player" draggable={false} />
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
          <div className="totals-cell">
            <ManaBar player={player} mine={mine} />
            <Totals player={player} side="hero" />
          </div>
          <ExpCell player={player} side="hero" mine={mine} />
          <HeroCell player={player} />
          <ExpCell player={player} side="companion" mine={mine} />
          <Totals player={player} side="companion" />
          <span />
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The Tumult track
// ---------------------------------------------------------------------------

/** One Tumult card (or half of one) drawn from the storm.jpg sprite, cropped to the track height. */
function stormStyle(t: StormTile, u: number, h: number, part: 'full' | 'left' | 'right'): CSSProperties {
  const tileH = (2 * u * 372) / 520;
  const y = -(t.id * tileH + (tileH - h) / 2);
  // A flipped card shows its other half, upside down
  const half = part === 'full' ? null : t.flipped ? (part === 'left' ? 'right' : 'left') : part;
  const x = half === 'right' ? -u : 0;
  return {
    width: part === 'full' ? 2 * u : u,
    height: h,
    backgroundSize: `${2 * u}px auto`,
    backgroundPosition: `${x}px ${y}px`,
    transform: t.flipped ? 'rotate(180deg)' : undefined,
  };
}

/** Keyed by its card id at the call site: a card turned face up mounts again and plays its flip. */
function Tile({ t, u, h, part, at }: { t: StormTile; u: number; h: number; part: 'full' | 'left' | 'right'; at: number }) {
  return (
    <span
      className={`storm-tile ${t.id !== STORM_BACK ? 'reveal' : ''}`}
      style={{ ...stormStyle(t, u, h, part), left: at * u }}
      aria-hidden="true"
    />
  );
}

function Marker({ player, side, mine, other }: { player: PlayerView; side: Side; mine: boolean; other: boolean }) {
  const { view } = useGameCtx();
  const pick = useExpeditionPick(player, side);
  const exp = player.expeditions[side];
  // Both expeditions of a player in the same region: side by side; a terrain marker: on its right
  const marked = view.regions.some((r) => r.index === exp.position && r.marker);
  const dx = (other ? (side === 'hero' ? -0.22 : 0.22) : 0) + (marked ? MARKED_SHIFT : 0);
  const style: CSSProperties = { left: `${((regionCenter(exp.position) + dx) / TRACK_UNITS) * 100}%` };
  return (
    <button
      type="button"
      className={`marker ${mine ? 'mine' : 'theirs'} ${pick ? 'selectable' : ''} ${pick?.picked ? 'picked' : ''} ${exp.blocked ? 'blocked' : ''} ${exp.ascended ? 'ascended' : ''}`}
      style={style}
      onClick={pick?.pick}
      disabled={!pick}
      title={`${player.name}: ${side} expedition, region ${exp.position}${exp.blocked ? ' (blocked)' : ''}${exp.ascended ? ' (Ascended)' : ''}`}
    >
      {player.faction ? <img src={`/img/tokens/${player.faction}-${side}.png`} alt="" draggable={false} /> : <span>{side[0].toUpperCase()}</span>}
    </button>
  );
}

function Ghost({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const { view } = useGameCtx();
  const f = forecast(view, player, side);
  if (!f.advances || view.phase === 'night' || view.phase === 'setup') return null;
  const next = Math.max(0, Math.min(7, f.next));
  const dx = view.regions.some((r) => r.index === next && r.marker) ? MARKED_SHIFT : 0;
  return (
    <span
      className={`ghost-marker ${mine ? 'mine' : 'theirs'}`}
      style={{ left: `${((regionCenter(next) + dx) / TRACK_UNITS) * 100}%` }}
      title={`Expected after Dusk: region ${f.next}`}
    />
  );
}

function MatTrack({ l }: { l: ArenaLayout }) {
  const { view, me, opp, you } = useGameCtx();
  const trackW = l.plane.w * 0.88;
  const u = trackW / TRACK_UNITS;
  const h = l.trackH;
  const t = stormTiles(view.regions);
  const regions = [...view.regions].sort((a, b) => a.index - b.index);
  const players = [opp, me].filter((p): p is PlayerView => !!p);
  return (
    <div className="mat-row mat-trackrow" style={rowBackground(l, 2)}>
      <div className="track" style={{ width: trackW, height: h, left: HERO_X * l.plane.w - (TRACK_UNITS / 2) * u }}>
        <DropZone destination="spell" className="track-drop" label="Cast">
          <Tile key={`0-${t[0].id}`} t={t[0]} u={u} h={h} part="full" at={0} />
          <Tile key={`1-${t[1].id}`} t={t[1]} u={u} h={h} part="full" at={2} />
          <Tile key={`2l-${t[2].id}`} t={t[2]} u={u} h={h} part="left" at={4} />
          <Tile key={`2r-${t[2].id}`} t={t[2]} u={u} h={h} part="right" at={6} />
          <Tile key={`3-${t[3].id}`} t={t[3]} u={u} h={h} part="full" at={7} />
          <Tile key={`4-${t[4].id}`} t={t[4]} u={u} h={h} part="full" at={9} />
          <RegionMarkers regions={regions} u={u} />
          {players.map((p) => {
            const mine = p.id === (you ?? me?.id);
            const same = p.expeditions.hero.position === p.expeditions.companion.position;
            return (
              <span key={p.id} className={`track-lane ${mine ? 'lane-mine' : 'lane-theirs'}`}>
                <Ghost player={p} side="hero" mine={mine} />
                <Ghost player={p} side="companion" mine={mine} />
                <Marker player={p} side="hero" mine={mine} other={same} />
                <Marker player={p} side="companion" mine={mine} other={same} />
              </span>
            );
          })}
        </DropZone>
        {regions.map((r) => (
          <span key={r.index} className="region-chips" style={{ left: regionCenter(r.index) * u }}>
            {regionBiomes(r)?.map((b) => (
              <span key={b} className={`tot-${b}`}>
                <span className="tot-icon">
                  <BiomeGlyph biome={b} />
                </span>
              </span>
            ))}
          </span>
        ))}
        <div className="track-center" style={{ left: 5 * u, width: u }}>
          <TrackButton />
        </div>
      </div>
    </div>
  );
}

/** The tilted mat: opponent zone, opponent expeditions, Tumult, my expeditions, my zone. */
export function Mat({ l }: { l: ArenaLayout }) {
  const { me, opp, you } = useGameCtx();
  const mine = you !== null;
  return (
    <>
      <MatZone l={l} row={0} player={opp} mine={false} />
      <MatBand l={l} row={1} player={opp} mine={false} />
      <MatTrack l={l} />
      <MatBand l={l} row={3} player={me} mine={mine} />
      <MatZone l={l} row={4} player={me} mine={mine} />
    </>
  );
}
