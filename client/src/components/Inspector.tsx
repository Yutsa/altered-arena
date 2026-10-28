import { createPortal } from 'react-dom';
import { AnimatePresence, motion } from 'motion/react';
import { useInspect, type InspectTarget } from '../game/inspect';
import { Card, BIOMES } from './Card';
import { STATUS_INFO } from '../game/keywords';
import { RichText } from './RichText';
import { BiomeGlyph } from './Glyph';
import { FACTION_NAMES } from '../api/catalog';
import './Inspector.css';

function hasDetails(t: InspectTarget): boolean {
  const c = t.card;
  if (!c) return false;
  const changed = c.stats && BIOMES.some((b) => c.stats![b] !== (t.def[b] ?? 0));
  return !!changed || c.boosts > 0 || c.exhausted || c.statuses.length > 0 || Object.keys(c.counters).length > 0;
}

/** Large readable version of a card: hover preview on desktop, modal on tap / long press. */
export function Inspector() {
  const { target, hide } = useInspect();
  return createPortal(
    <AnimatePresence>
      {target && (
        <motion.div
          key={target.mode}
          className={`inspector insp-${target.mode} ${target.mode === 'hover' && (target.x ?? 0) > window.innerWidth / 2 ? 'left' : 'right'}`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.12 }}
          onClick={() => hide()}
        >
          <motion.div
            className="inspector-content"
            initial={{ scale: 0.92, y: 10 }}
            animate={{ scale: 1, y: 0 }}
            transition={{ type: 'spring', stiffness: 500, damping: 34 }}
          >
            <Card def={target.def} card={target.card} size="zoom" />
            {(target.mode === 'modal' || hasDetails(target)) && (
            <div className="inspector-info">
              {target.mode === 'modal' && (
                <>
                  <div className="ii-name">{target.def.name}</div>
                  <div className="ii-type">
                    {target.def.typeline} · {FACTION_NAMES[target.def.faction]} · {target.def.rarity}
                  </div>
                </>
              )}
              {target.card?.stats && (
                <div className="ii-row">
                  {BIOMES.map((b) => {
                    const base = target.def[b] ?? 0;
                    const cur = target.card!.stats![b];
                    return (
                      <span key={b} className={`ii-stat stat-${b}`}>
                        <BiomeGlyph biome={b} /> {cur}
                        {cur !== base && <small> (base {base})</small>}
                      </span>
                    );
                  })}
                </div>
              )}
              {(target.card?.boosts ?? 0) > 0 && <div className="ii-line">Boosts: {target.card!.boosts}</div>}
              {Object.entries(target.card?.counters ?? {}).map(([k, n]) => (
                <div key={k} className="ii-line">
                  {k}: {n}
                </div>
              ))}
              {target.card?.exhausted && <div className="ii-line">Exhausted</div>}
              {target.card?.statuses.map((s) => (
                <div key={s} className="ii-line">
                  <b>{STATUS_INFO[s]?.label ?? s}</b>
                  {STATUS_INFO[s]?.reminder && (
                    <>
                      {' '}
                      — <RichText text={STATUS_INFO[s].reminder} />
                    </>
                  )}
                </div>
              ))}
              {target.mode === 'modal' && target.def.flavor && <div className="ii-flavor">{target.def.flavor}</div>}
              {target.mode === 'modal' && target.def.artist && <div className="ii-artist">Art: {target.def.artist}</div>}
              {target.mode === 'modal' && <div className="ii-close muted">Tap anywhere to close</div>}
            </div>
            )}
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
