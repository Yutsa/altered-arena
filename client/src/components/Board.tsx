import type { Biome, PlayerView, Side, Zone } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { BoardCard } from './BoardCard';
import { DropZone, useOfferedDestinations } from './DropZone';
import { BiomeGlyph, Glyph } from './Glyph';
import { BIOMES } from './Card';
import './Board.css';

/** A location option of the current decision that designates this zone of this player, if any */
function useLocation(player: PlayerView, zone: Zone) {
  const { decision, send } = useGameCtx();
  if (decision?.kind !== 'location') return null;
  const loc = decision.locations.find((l) => l.player === player.id && l.zone === zone);
  return loc ? () => void send({ type: 'location', key: loc.key }) : null;
}

function ExpeditionStats({ player, side }: { player: PlayerView; side: Side }) {
  const { view } = useGameCtx();
  const exp = player.expeditions[side];
  const region = view.regions.find((r) => r.index === exp.position);
  const active = new Set<Biome>(region?.biomes ?? []);
  return (
    <div className={`exp-stats ${exp.blocked ? 'blocked' : ''}`} title={`Region ${exp.position}${exp.blocked ? ', blocked' : ''}`}>
      {BIOMES.map((b) => (
        <span key={b} className={`exp-stat stat-${b} ${active.has(b) ? 'active' : region?.biomes ? 'inactive' : ''}`}>
          <BiomeGlyph biome={b} />
          <b>{exp.stats[b]}</b>
        </span>
      ))}
      {exp.blocked && <span className="exp-blocked">Blocked</span>}
    </div>
  );
}

/** One expedition line (the characters sent in the hero or companion expedition). */
export function ExpeditionRow({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const { zoneCards, decision, ui, setUI, send } = useGameCtx();
  const { destinations } = useOfferedDestinations();
  const zone: Zone = side === 'hero' ? 'heroExpedition' : 'companionExpedition';
  const cards = zoneCards(player.id, zone);
  const opt = decision?.kind === 'expedition' ? decision.expeditions.find((o) => o.player === player.id && o.side === side) : undefined;
  const picked = !!opt && ui.keys.includes(opt.key);
  const pickLocation = useLocation(player, zone);
  const scoutDest = side === 'hero' ? 'heroExpeditionScout' : 'companionExpeditionScout';
  const onPick = opt
    ? () => {
        if (decision?.kind !== 'expedition') return;
        if (decision.min === 1 && decision.max === 1) void send({ type: 'expedition', keys: [opt.key] });
        else setUI((u) => ({ ...u, keys: u.keys.includes(opt.key) ? u.keys.filter((k) => k !== opt.key) : [...u.keys, opt.key].slice(-decision.max) }));
      }
    : undefined;
  const body = (
    <>
      <div className="exp-head">
        <span className="exp-title">{side === 'hero' ? 'Hero' : 'Companion'}</span>
        <ExpeditionStats player={player} side={side} />
      </div>
      <div className="exp-cards">
        {cards.map((c) => (
          <BoardCard key={c.id} id={c.id} size="mini" />
        ))}
        {cards.length === 0 && <span className="zone-empty">No characters</span>}
      </div>
      {mine && destinations.includes(scoutDest) && (
        <DropZone destination={scoutDest} className="scout-zone" label="Scout" />
      )}
      {opt && (
        <button type="button" className={`exp-pick ${picked ? 'picked' : ''}`} onClick={onPick}>
          {picked ? 'Selected' : 'Choose'}
        </button>
      )}
      {pickLocation && (
        <button type="button" className="exp-pick" onClick={pickLocation}>
          Choose
        </button>
      )}
    </>
  );
  const cls = `expedition exp-${side} ${mine ? 'mine' : 'theirs'} ${opt || pickLocation ? 'selectable' : ''} ${picked ? 'picked' : ''}`;
  if (mine)
    return (
      <DropZone destination={zone} className={cls} label={side === 'hero' ? 'Hero expedition' : 'Companion expedition'}>
        {body}
      </DropZone>
    );
  return <div className={cls}>{body}</div>;
}

/** Reserve or Landmarks of a player, with empty slots up to the hero's limit. */
export function ZoneRow({ player, zone, mine }: { player: PlayerView; zone: 'reserve' | 'landmark'; mine: boolean }) {
  const { zoneCards } = useGameCtx();
  const cards = zoneCards(player.id, zone);
  const slots = zone === 'reserve' ? player.reserveSlots : player.landmarkSlots;
  const empty = Math.max(0, slots - cards.length);
  const label = zone === 'reserve' ? 'Reserve' : 'Landmarks';
  const pickLocation = useLocation(player, zone);
  const body = (
    <>
      <div className="zone-title">
        <Glyph name={zone === 'reserve' ? 'reserve' : 'permanent'} /> {label}
        <span className="zone-count">
          {cards.length}/{slots}
        </span>
      </div>
      <div className="zone-cards">
        {cards.map((c) => (
          <BoardCard key={c.id} id={c.id} size="mini" />
        ))}
        {Array.from({ length: empty }, (_, i) => (
          <div key={`e${i}`} className="slot-empty" />
        ))}
      </div>
      {pickLocation && (
        <button type="button" className="exp-pick" onClick={pickLocation}>
          Choose
        </button>
      )}
    </>
  );
  const cls = `zone-row zone-${zone} ${mine ? 'mine' : 'theirs'} ${pickLocation ? 'selectable' : ''}`;
  return mine ? (
    <DropZone destination={zone} className={cls} label={label}>
      {body}
    </DropZone>
  ) : (
    <div className={cls}>{body}</div>
  );
}
