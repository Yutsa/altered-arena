import { memo, useState } from 'react';
import type { Biome, CardDef, CardView } from '@protocol';
import { artUrl } from '../api/catalog';
import { BiomeGlyph, Glyph } from './Glyph';
import { RichText } from './RichText';
import { STATUS_INFO } from '../game/keywords';
import './Card.css';

export type CardSize = 'mini' | 'small' | 'hand' | 'zoom';
export const BIOMES: Biome[] = ['forest', 'mountain', 'ocean'];

export interface CardProps {
  def: CardDef | null | undefined;
  card?: CardView;
  size: CardSize;
  /** Extra mana a target costs (SelectDecision.costs) */
  extraCost?: number;
  className?: string;
}

function cmp(cur: number | undefined, base: number | undefined): '' | 'up' | 'down' {
  if (cur === undefined || base === undefined || cur === base) return '';
  return cur > base ? 'up' : 'down';
}

export function CardBack({ size, className }: { size: CardSize; className?: string }) {
  return (
    <div className={`card card-${size} card-back ${className ?? ''}`}>
      <div className="card-inner">
        <img className="card-art" src="/img/card-back.jpg" alt="" draggable={false} />
      </div>
    </div>
  );
}

export const Card = memo(function Card({ def, card, size, extraCost, className }: CardProps) {
  const [artFailed, setArtFailed] = useState(false);
  if (!def) return <CardBack size={size} className={className} />;
  const isToken = def.type === 'token' || !!card?.token;
  const hasCosts = def.type !== 'hero' && !isToken && def.costHand !== undefined;
  const hasStats = def.type === 'character' || (def.type === 'token' && def.forest !== undefined);
  const costHand = card?.costHand ?? def.costHand;
  const costReserve = card?.costReserve ?? def.costReserve;
  const statuses = card?.statuses ?? [];
  const counters = Object.entries(card?.counters ?? {}).filter(([, n]) => n > 0);
  const showText = size === 'zoom' || size === 'hand';
  return (
    <div
      className={[
        'card',
        `card-${size}`,
        `f-${def.faction}`,
        `t-${def.type}`,
        isToken ? 'is-token' : '',
        card?.exhausted ? 'is-exhausted' : '',
        className ?? '',
      ].join(' ')}
      data-rarity={def.rarity}
    >
      <div className="card-inner">
        {artFailed ? (
          <div className="card-art card-art-missing" />
        ) : (
          <img
            className="card-art"
            src={artUrl(def)}
            alt=""
            loading="lazy"
            decoding="async"
            draggable={false}
            onError={() => setArtFailed(true)}
          />
        )}
        <div className="card-frame" />
        {hasCosts && (
          <div className="card-costs">
            <span className={`cost cost-hand ${cmp(costHand, def.costHand)}`} title="Hand cost">
              {costHand}
            </span>
            <span className={`cost cost-reserve ${cmp(costReserve, def.costReserve)}`} title="Reserve cost">
              {costReserve}
            </span>
          </div>
        )}
        {def.type !== 'hero' && !isToken && <span className={`card-rarity r-${def.rarity}`} title={def.rarity} />}
        <div className="card-plate">
          <div className="card-name">{def.name}</div>
          <div className="card-typeline">{def.typeline}</div>
        </div>
        {hasStats && (
          <div className="card-stats">
            {BIOMES.map((b) => {
              const base = def[b] ?? 0;
              const cur = card?.stats?.[b] ?? base;
              return (
                <span key={b} className={`stat stat-${b} ${cmp(cur, base)}`} title={`${b} ${cur}`}>
                  <BiomeGlyph biome={b} />
                  <b>{cur}</b>
                </span>
              );
            })}
          </div>
        )}
        {showText && (def.effect || def.support) && (
          <div className="card-text">
            {def.effect && (
              <div className="card-effect">
                <RichText text={def.effect} />
              </div>
            )}
            {def.support && (
              <div className="card-support">
                <Glyph name="reserve" title="Support (in Reserve)" />
                <RichText text={def.support} />
              </div>
            )}
          </div>
        )}
        {def.type === 'hero' && size !== 'mini' && (def.reserveSlots || def.landmarkSlots) ? (
          <div className="card-slots">
            <span title="Reserve slots">
              <Glyph name="reserve" /> {def.reserveSlots}
            </span>
            <span title="Landmark slots">
              <Glyph name="permanent" /> {def.landmarkSlots}
            </span>
          </div>
        ) : null}
        {(statuses.length > 0 || (card?.boosts ?? 0) > 0 || counters.length > 0) && (
          <div className="card-badges">
            {(card?.boosts ?? 0) > 0 && (
              <span className="badge badge-boost" title={`${card!.boosts} boost(s)`}>
                <img src="/img/icons/boost.png" alt="" />
                {card!.boosts}
              </span>
            )}
            {statuses.map((s) => {
              const info = STATUS_INFO[s];
              return info?.icon ? (
                <span key={s} className={`badge badge-status badge-${s}`} title={info.label}>
                  <img src={`/img/icons/${info.icon}.png`} alt={info.label} />
                </span>
              ) : (
                <span key={s} className={`badge badge-status badge-text badge-${s}`} title={info?.label ?? s}>
                  {(info?.label ?? s).slice(0, 3)}
                </span>
              );
            })}
            {counters.map(([name, n]) => (
              <span key={name} className="badge badge-counter" title={`${n} ${name}`}>
                <Glyph name="charge" />
                {n}
              </span>
            ))}
          </div>
        )}
        {extraCost ? (
          <span className="card-extra-cost" title={`Costs ${extraCost} more`}>
            +{extraCost}
          </span>
        ) : null}
      </div>
    </div>
  );
});
