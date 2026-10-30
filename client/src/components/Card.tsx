import { memo, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import type { Biome, CardDef, CardView } from '@protocol';
import { artUrl } from '../api/catalog';
import { Glyph } from './Glyph';
import { RichText } from './RichText';
import { STATUS_INFO } from '../game/keywords';
import './Card.css';

export type CardSize = 'small' | 'hand' | 'zoom';
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
      <img className="fc-art" src="/img/card-back.jpg" alt="" draggable={false} />
    </div>
  );
}

// ---------------------------------------------------------------------------
// The printed card, drawn as on BGA: the art, the faction frame of its type and rarity
// (img/card/frames/<faction>.png, 6 x 8 frames), costs, statistics, name and texts on a
// 324 x 450 card. Every length is a multiple of --u, 1/324 of the card width.
// ---------------------------------------------------------------------------

type FrameType = 'hero' | 'token' | 'character' | 'spell' | 'permanent' | 'gear';
const RARITY: Record<CardDef['rarity'], number> = { common: 0, rare: 1, unique: 2, exalted: 3 };

/** Position of each frame in the sprite, by frame type, rarity, "support" (has a support ability) and size (2: bigger text box) */
const FRAMES: Record<string, string> = {
  hero: '0% 28.57%',
  token: '20% 28.57%',
  'character-0-0-1': '40% 0%', 'character-0-0-2': '60% 0%', 'character-0-1-1': '0% 0%', 'character-0-1-2': '20% 0%',
  'character-1-0-1': '80% 28.57%', 'character-1-0-2': '100% 28.57%', 'character-1-1-1': '40% 28.57%', 'character-1-1-2': '60% 28.57%',
  'character-2-0-1': '80% 57.14%', 'character-2-0-2': '100% 57.14%', 'character-2-1-1': '40% 57.14%', 'character-2-1-2': '60% 57.14%',
  'character-3-0-1': '80% 85.71%', 'character-3-0-2': '100% 85.71%', 'character-3-1-1': '40% 85.71%', 'character-3-1-2': '60% 85.71%',
  'spell-0-0-1': '80% 14.28%', 'spell-0-0-2': '100% 14.28%', 'spell-0-1-1': '40% 14.28%', 'spell-0-1-2': '60% 14.28%',
  'spell-1-0-1': '0% 57.14%', 'spell-1-0-2': '20% 57.14%', 'spell-1-1-1': '80% 42.85%', 'spell-1-1-2': '100% 42.85%',
  'permanent-0-0-1': '0% 14.28%', 'permanent-0-0-2': '20% 14.28%', 'permanent-0-1-1': '80% 0%', 'permanent-0-1-2': '100% 0%',
  'permanent-1-0-1': '40% 42.85%', 'permanent-1-0-2': '60% 42.85%', 'permanent-1-1-1': '0% 42.85%', 'permanent-1-1-2': '20% 42.85%',
  'permanent-3-1-1': '80% 100%', 'permanent-3-1-2': '100% 100%',
  'feat-0-1-1': '0% 100%', 'feat-0-1-2': '20% 100%', 'feat-1-1-1': '40% 100%', 'feat-1-1-2': '60% 100%',
  'feat-3-1-1': '80% 100%', 'feat-3-1-2': '100% 100%',
  'gear-0-0-1': '40% 71.43%', 'gear-0-0-2': '60% 71.43%', 'gear-0-1-1': '0% 71.43%', 'gear-0-1-2': '20% 71.43%',
  'gear-1-0-1': '0% 85.71%', 'gear-1-0-2': '20% 85.71%', 'gear-1-1-1': '80% 71.43%', 'gear-1-1-2': '100% 71.43%',
};

function framePosition(type: FrameType, feat: boolean, rarity: number, support: boolean, size: number): string {
  if (type === 'hero' || type === 'token') return FRAMES[type];
  const kind = feat ? 'feat' : type;
  // Rarities without a frame of their own (unique spells...) use the rare one, then the common one
  for (const r of [rarity, 1, 0]) {
    const f = FRAMES[`${kind}-${r}-${support ? 1 : 0}-${size}`] ?? FRAMES[`${type}-${r}-${support ? 1 : 0}-${size}`];
    if (f) return f;
  }
  return FRAMES[`${type}-0-${support ? 1 : 0}-${size}`] ?? '0% 0%';
}

function frameType(def: CardDef, token: boolean): FrameType {
  if (def.type === 'hero') return 'hero';
  if (def.type === 'character') return token ? 'token' : 'character';
  if (def.type === 'spell') return 'spell';
  if (def.type === 'token' && def.forest !== undefined) return 'token';
  return def.subtypes.includes('landmark') ? 'permanent' : 'gear';
}

/** Set icon of the footer, from the set of the art */
const SETS: Record<string, string> = { CORE: 'core', COREKS: 'ks2', ALIZE: 'tbf', BISE: 'wfm', CYCLONE: 'so', EOLE: 'roc' };
const setIcon = (asset: string) => SETS[/^ALT_([A-Z]+)_/.exec(asset)?.[1] ?? ''] ?? 'core';

/** The flavor text, only when there is room for it (BGA getFlavorTextIfFitting) */
function flavorIfFitting(def: CardDef, token: boolean): string {
  const flavor = def.flavor ?? '';
  const max = def.support === '' ? 250 : Math.min(250 - (def.support?.length ?? 0), 180);
  if (!flavor || token || def.effect.length + flavor.length >= max) return '';
  return flavor;
}

/** Relative sizes of the statistics (BGA getBiomesUISizes): 0 for 0, then smaller / middle / larger */
export function statSizes(def: CardDef): Record<Biome, number> {
  const v = { forest: def.forest ?? 0, mountain: def.mountain ?? 0, ocean: def.ocean ?? 0 };
  const sorted = [v.forest, v.mountain, v.ocean].sort((a, b) => a - b);
  const mid = sorted[1] === 0 ? sorted[2] : sorted[1];
  const size = (x: number) => (x === 0 ? 0 : x < mid ? 1 : x === mid ? 2 : 3);
  return { forest: size(v.forest), mountain: size(v.mountain), ocean: size(v.ocean) };
}

/**
 * Shrinks the texts until they fit (BGA autofitCardFrame): the support ability first, then the
 * effect, which may also take the bigger text box of the frame.
 */
function useAutofit(root: RefObject<HTMLDivElement | null>, key: string, canGrow: boolean, setSize: (n: number) => void) {
  const [fit, setFit] = useState<{ key: string; text?: number; support?: number; tight?: boolean } | null>(null);
  useLayoutEffect(() => {
    const el = root.current;
    const text = el?.querySelector<HTMLElement>('.fc-text');
    const effect = el?.querySelector<HTMLElement>('.fc-effect');
    if (!el || !text || !effect || text.clientHeight === 0) return;
    const support = el.querySelector<HTMLElement>('.fc-support');
    const px = (n: number) => `calc(${n} * var(--u))`;
    const out: { key: string; text?: number; support?: number; tight?: boolean } = { key };
    text.style.fontSize = '';
    if (support) {
      support.style.fontSize = '';
      support.style.lineHeight = '';
    }
    if (support && support.textContent?.trim() && support.clientHeight > 0) {
      const ok = () => support.scrollHeight <= support.clientHeight + 1;
      for (let s = 13; s >= 9 && !ok(); s--) {
        support.style.fontSize = px(s);
        out.support = s;
      }
      if (!ok()) {
        support.style.lineHeight = '1';
        out.tight = true;
      }
    }
    const effectOk = () => text.offsetHeight - (support?.offsetHeight ?? 0) >= effect.offsetHeight;
    if (!effectOk()) {
      // Heroes and tokens have a single frame: their text only gets smaller
      if (canGrow) {
        el.dataset.size = '2';
        setSize(2);
      }
      for (let s = 13; s >= (canGrow ? 10 : 8) && !effectOk(); s--) {
        text.style.fontSize = px(s);
        out.text = s;
      }
    }
    setFit(out);
    // Measured once per printed card
  }, [key, canGrow, root, setSize]);
  return fit?.key === key ? fit : null;
}

function Printed({ def, card, token }: { def: CardDef; card?: CardView; token: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState(1);
  const key = `${def.ref}:${def.name}`;
  const [sizeKey, setSizeKey] = useState(key);
  if (sizeKey !== key) {
    setSizeKey(key);
    setSize(1);
  }
  const type = frameType(def, token);
  const fit = useAutofit(ref, key, type !== 'hero' && type !== 'token', setSize);
  const rarity = RARITY[def.rarity] ?? 0;
  const support = !!def.support;
  const feat = def.subtypes.includes('feat');
  const hasCosts = type !== 'hero' && type !== 'token' && !token && def.costHand !== undefined;
  const hasStats = type === 'character' || type === 'token';
  const sizes = statSizes(def);
  const flavor = flavorIfFitting(def, token);
  const permanent = type === 'permanent' || type === 'gear';
  const px = (n: number | undefined) => (n ? `calc(${n} * var(--u))` : undefined);
  return (
    <div
      ref={ref}
      className="fc"
      data-frame={type}
      data-rarity={type === 'token' ? 0 : rarity}
      data-support={support ? 1 : 0}
      data-size={size}
      data-faction={def.faction}
    >
      <div
        className="fc-frame"
        style={{ backgroundImage: `url(/img/card/frames/${def.faction}.png)`, backgroundPosition: framePosition(type, feat, rarity, support, size) }}
      />
      {hasCosts && (
        <>
          <span className={`fc-cost-hand ${cmp(card?.costHand, def.costHand)}`}>{card?.costHand ?? def.costHand}</span>
          <span className={`fc-cost-reserve ${cmp(card?.costReserve, def.costReserve)}`}>{card?.costReserve ?? def.costReserve}</span>
        </>
      )}
      <div className="fc-name">{def.name}</div>
      <div className="fc-typeline">{def.typeline}</div>
      {hasStats &&
        BIOMES.map((b) => {
          const base = def[b] ?? 0;
          const cur = card?.stats?.[b] ?? base;
          return (
            <span key={b} className={`fc-stat fc-${b} ${cmp(cur, base)}`} data-size={sizes[b]}>
              {cur}
            </span>
          );
        })}
      {permanent && (
        <div className="fc-subpermanent">
          {def.subtypes.includes('landmark')
            ? '(Play me in your Landmark zone. I don’t gain Fleeting.)'
            : '(Play me in an Expedition. If it moves forward, I go to Reserve during Rest.)'}
        </div>
      )}
      <div className="fc-text" style={{ fontSize: px(fit?.text) }}>
        <span className="fc-qr">
          <span />
        </span>
        <div className="fc-effect">
          <RichText text={def.effect} />
          {flavor && (
            <>
              {def.effect && <hr />}
              <span className="fc-flavor">{flavor}</span>
            </>
          )}
        </div>
      </div>
      {support && (
        <div className="fc-support" style={{ fontSize: px(fit?.support), lineHeight: fit?.tight ? 1 : undefined }}>
          <RichText text={def.support} />
        </div>
      )}
      <div className="fc-footer">
        <span className="fc-set" style={{ backgroundImage: `url(/img/card/set-${setIcon(def.asset)}.png)` }} />
        {def.artist}
      </div>
    </div>
  );
}

export const Card = memo(function Card({ def, card, size, extraCost, className }: CardProps) {
  const [artFailed, setArtFailed] = useState(false);
  if (!def) return <CardBack size={size} className={className} />;
  const token = def.type === 'token' || !!card?.token;
  const statuses = card?.statuses ?? [];
  const counters = Object.entries(card?.counters ?? {}).filter(([, n]) => n > 0);
  return (
    <div
      className={['card', `card-${size}`, `t-${def.type}`, token ? 'is-token' : '', card?.exhausted ? 'is-exhausted' : '', className ?? ''].join(' ')}
      data-rarity={def.rarity}
    >
      {artFailed ? (
        <div className="fc-art fc-art-missing" />
      ) : (
        <img
          className="fc-art"
          src={artUrl(def)}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          onError={() => setArtFailed(true)}
        />
      )}
      <Printed def={def} card={card} token={token} />
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
  );
});
