import { motion } from 'motion/react';
import type { Biome, PlayerView, RegionView, Side } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { BiomeGlyph } from './Glyph';
import { DropZone } from './DropZone';
import { BoardCard } from './BoardCard';
import './Tumult.css';

const BIOME_COLORS: Record<Biome, string> = {
  forest: 'var(--b-forest)',
  mountain: 'var(--b-mountain)',
  ocean: 'var(--b-ocean)',
};

function regionBackground(r: RegionView): string | undefined {
  if (!r.biomes) return undefined;
  const cols = r.biomes.map((b) => BIOME_COLORS[b]);
  if (cols.length === 1) return cols[0];
  const step = 100 / cols.length;
  return `linear-gradient(135deg, ${cols.map((c, i) => `${c} ${i * step}% ${(i + 1) * step}%`).join(', ')})`;
}

function Marker({ player, side, mine }: { player: PlayerView; side: Side; mine: boolean }) {
  const { decision, ui, setUI, send } = useGameCtx();
  const opt = decision?.kind === 'expedition' ? decision.expeditions.find((o) => o.player === player.id && o.side === side) : undefined;
  const picked = opt && ui.keys.includes(opt.key);
  const exp = player.expeditions[side];
  const onClick = opt
    ? () => {
        if (decision?.kind !== 'expedition') return;
        if (decision.min === 1 && decision.max === 1) void send({ type: 'expedition', keys: [opt.key] });
        else setUI((u) => ({ ...u, keys: u.keys.includes(opt.key) ? u.keys.filter((k) => k !== opt.key) : [...u.keys, opt.key].slice(-decision.max) }));
      }
    : undefined;
  return (
    <motion.button
      type="button"
      layoutId={`marker-${player.id}-${side}`}
      transition={{ type: 'spring', stiffness: 200, damping: 24 }}
      className={`marker ${mine ? 'mine' : 'theirs'} ${opt ? 'selectable' : ''} ${picked ? 'picked' : ''} ${exp.blocked ? 'blocked' : ''}`}
      onClick={onClick}
      disabled={!opt}
      title={`${player.name}: ${side} expedition${exp.blocked ? ' (blocked)' : ''}`}
    >
      {player.faction ? <img src={`/img/tokens/${player.faction}-${side}.png`} alt="" draggable={false} /> : <span>{side[0].toUpperCase()}</span>}
    </motion.button>
  );
}

/** The Tumult: 8 regions; hero expeditions go 0 -> 7, companion expeditions 7 -> 0. */
export function Tumult() {
  const { view, me, opp } = useGameCtx();
  const regions = [...view.regions].sort((a, b) => a.index - b.index);
  const limbo = view.cards.filter((c) => c.zone === 'limbo');
  return (
    <div className="tumult-wrap">
      <div className="tumult" role="list" aria-label="Tumult">
        {regions.map((r) => (
          <div
            key={r.index}
            role="listitem"
            className={`region k-${r.kind} ${r.biomes ? '' : 'face-down'}`}
            style={{ background: regionBackground(r) }}
            title={r.biomes ? `Region ${r.index}: ${r.biomes.join(', ')}` : `Region ${r.index}: face down`}
          >
            <div className="region-markers top">
              {opp &&
                (['hero', 'companion'] as Side[]).map((s) =>
                  opp.expeditions[s].position === r.index ? <Marker key={s} player={opp} side={s} mine={false} /> : null,
                )}
            </div>
            <div className="region-biomes">{r.biomes?.map((b) => <BiomeGlyph key={b} biome={b} />)}</div>
            <div className="region-markers bottom">
              {me &&
                (['hero', 'companion'] as Side[]).map((s) =>
                  me.expeditions[s].position === r.index ? <Marker key={s} player={me} side={s} mine /> : null,
                )}
            </div>
          </div>
        ))}
      </div>
      <DropZone destination="spell" className="cast-zone" label="Cast spell" />
      {limbo.length > 0 && (
        <div className="limbo" aria-label="Being played">
          {limbo.map((c) => (
            <BoardCard key={c.id} id={c.id} size="small" />
          ))}
        </div>
      )}
    </div>
  );
}
