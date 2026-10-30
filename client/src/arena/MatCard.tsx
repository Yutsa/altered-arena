import { memo, useEffect, useRef, useState, type SyntheticEvent } from 'react';
import type { Biome, CardView } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { useCardPointer } from '../game/useCardPointer';
import { useDrag } from '../game/drag';
import { hasActions } from '../game/decisions';
import { STATUS_INFO } from '../game/keywords';
import { artUrl } from '../api/catalog';
import { BIOMES } from '../components/Card';
import { Glyph } from '../components/Glyph';

type Fx = { key: number; text: string; up: boolean };

/**
 * A card lying on the mat: its art, and its statistics in a column on the left. Boosted
 * statistics are drawn in green (lowered ones in red): nothing covers the art. Gains and losses
 * pop a short "+1" above the card.
 */
export const MatCard = memo(function MatCard({ id, kind }: { id: number; kind: 'exp' | 'zone' | 'hero' }) {
  const { byId, def, aff, ui, send } = useGameCtx();
  const { drag } = useDrag();
  const { draggable, handlers } = useCardPointer(id);
  const card = byId.get(id);
  const d = def(card);
  const a = aff.get(id);
  const fx = useStatFx(card);
  const [artFailed, setArtFailed] = useState(false);
  const isChar = !!card?.stats && (d?.type === 'character' || d?.type === 'token');
  const counters = Object.entries(card?.counters ?? {}).filter(([, n]) => n > 0);
  const cls = [
    'mc',
    `mc-${kind}`,
    hasActions(a) ? 'is-playable' : '',
    a?.selectable ? 'is-selectable' : '',
    a?.picked ? 'is-picked' : '',
    a?.source ? 'is-source' : '',
    ui.selected === id ? 'is-selected' : '',
    drag?.cardId === id ? 'is-dragged' : '',
    card?.exhausted ? 'is-exhausted' : '',
    fx ? (fx.up ? 'fx-up' : 'fx-down') : '',
  ].join(' ');
  return (
    <div
      className={cls}
      data-card-id={id}
      data-rarity={d?.rarity}
      style={{ touchAction: draggable ? 'none' : undefined }}
      title={d?.name}
      {...handlers}
    >
      {!d ? (
        <img className="mc-art" src="/img/card-back.jpg" alt="Face-down card" draggable={false} />
      ) : !artFailed ? (
        <img className="mc-art" src={artUrl(d)} alt={d.name} draggable={false} loading="lazy" onError={() => setArtFailed(true)} />
      ) : (
        <div className="mc-art mc-art-missing">{d.name}</div>
      )}
      {kind === 'hero' && d && <span className="mc-name">{d.name}</span>}
      {isChar && d && (
        <span className="mc-stats">
          {BIOMES.map((b: Biome) => {
            const base = d[b] ?? 0;
            const cur = card!.stats![b];
            return (
              <span key={b} className={`mc-stat s-${b} ${cur > base ? 'up' : cur < base ? 'down' : ''}`}>
                {cur}
              </span>
            );
          })}
        </span>
      )}
      {kind === 'zone' && card?.zone === 'reserve' && card.costReserve !== undefined && (
        <span className={`mc-cost ${d?.costReserve !== undefined && card.costReserve < d.costReserve ? 'down' : ''}`}>{card.costReserve}</span>
      )}
      {(card?.statuses.length ?? 0) > 0 && (
        <span className="mc-status">
          {card!.statuses.map((s) => {
            const info = STATUS_INFO[s];
            return info?.icon ? (
              <img key={s} src={`/img/icons/${info.icon}.png`} alt={info.label} title={info.label} />
            ) : (
              <b key={s} title={info?.label ?? s}>
                {(info?.label ?? s).slice(0, 3)}
              </b>
            );
          })}
        </span>
      )}
      {counters.length > 0 && (
        <span className="mc-counters">
          {counters.map(([name, n]) => (
            <span key={name} className="mc-counter" title={`${n} ${name}`}>
              {n}
            </span>
          ))}
        </span>
      )}
      {a?.extraCost ? <span className="mc-extra">+{a.extraCost}</span> : null}
      {kind === 'zone' && card?.zone === 'reserve' && d?.support && (
        <SupportIcon support={d.support} faction={d.faction} usable={!!a?.support} onUse={() => void send({ type: 'support', cardId: id })} />
      )}
      {fx && (
        <span key={fx.key} className={`mc-fx ${fx.up ? 'up' : 'down'}`}>
          {fx.text}
        </span>
      )}
    </div>
  );
});

/**
 * The support ability of a card in Reserve, as on BGA: a tag in its faction colour at the bottom
 * right, {D} (discard me to use it) or {I} (always on). When it can be used now, it pulses and one
 * click uses it.
 */
function SupportIcon({ support, faction, usable, onUse }: { support: string; faction: string; usable: boolean; onUse: () => void }) {
  const discard = /^#?\{D\}/.test(support.trim());
  const stop = (e: SyntheticEvent) => e.stopPropagation();
  return (
    <button
      type="button"
      className={`mc-support f-${faction} ${usable ? 'usable' : ''}`}
      disabled={!usable}
      title={usable ? 'Use its support ability' : discard ? 'Support ability: discard me from Reserve' : 'Support ability (in Reserve)'}
      onPointerDown={stop}
      onPointerUp={stop}
      onClick={(e) => {
        e.stopPropagation();
        if (usable) onUse();
      }}
    >
      <Glyph name={discard ? 'discard' : 'infinity'} />
    </button>
  );
}

/** A short effect when a card's boosts or statistics change (not when it first appears). */
function useStatFx(card: CardView | undefined): Fx | null {
  const prev = useRef<{ boosts: number; total: number; counters: number } | null>(null);
  const [fx, setFx] = useState<Fx | null>(null);
  const boosts = card?.boosts ?? 0;
  const total = card?.stats ? card.stats.forest + card.stats.mountain + card.stats.ocean : 0;
  const counters = Object.values(card?.counters ?? {}).reduce((s, n) => s + n, 0);
  useEffect(() => {
    const p = prev.current;
    prev.current = { boosts, total, counters };
    if (!p) return;
    let next: Fx | null = null;
    if (boosts !== p.boosts) {
      const dlt = boosts - p.boosts;
      next = { key: Date.now(), text: `${dlt > 0 ? '+' : ''}${dlt}`, up: dlt > 0 };
    } else if (total !== p.total) {
      next = { key: Date.now(), text: total > p.total ? '▲' : '▼', up: total > p.total };
    } else if (counters !== p.counters) {
      next = { key: Date.now(), text: `${counters > p.counters ? '+' : ''}${counters - p.counters}`, up: counters > p.counters };
    }
    if (next) setFx(next);
  }, [boosts, total, counters]);
  useEffect(() => {
    if (!fx) return;
    const t = setTimeout(() => setFx(null), 900);
    return () => clearTimeout(t);
  }, [fx]);
  return fx;
}
