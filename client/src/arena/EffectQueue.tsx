import { useEffect, useState } from 'react';
import type { CardView, ChoiceOption } from '@protocol';
import { artUrl } from '../api/catalog';
import { useGameCtx } from '../game/GameContext';
import { effectQueue, resolutionOf } from '../game/decisions';
import { RichText } from '../components/RichText';

const WHERE: Partial<Record<string, string>> = {
  hero: 'Hero',
  heroExpedition: 'Hero expedition',
  companionExpedition: 'Companion expedition',
  landmark: 'Landmark',
  reserve: 'Reserve',
  hand: 'Hand',
  limbo: 'Being played',
};

const where = (c: CardView | undefined) => (c && typeof c.zone === 'string' ? (WHERE[c.zone] ?? '') : '');

/** What the effect does: the effect it leads to (after its checks), else its label without the source's name */
const effectText = (c: ChoiceOption, siblings: ChoiceOption[]) =>
  resolutionOf(c, siblings) ?? c.label.replace(/\s*\(\[\[[^\]]+\]\]\)\s*$/, '');

/** The card that set the effect off, when its label names it ("Check reaction to [[Winter Nova]]...") */
const triggeredBy = (c: ChoiceOption, siblings: ChoiceOption[]) =>
  resolutionOf(c, siblings) ? (/\[\[([^\]]+)\]\]/.exec(c.label)?.[1] ?? null) : null;

/**
 * Effects that triggered together, beside End turn: one portrait per effect, the effect under the
 * pointer told above them and its card lit on the board. A click resolves that effect; the next
 * decision brings the effects left. Its minimize button (`onCollapse`) shrinks it into a badge
 * under the main button (EffectQueueBadge, a click on it brings the queue back), the hand
 * comes up to be looked at.
 */
export function EffectQueue({ collapsed, onCollapse }: { collapsed: boolean; onCollapse: () => void }) {
  const { decision, byId, def, send, sending, setFocusSource, inspectCard, setResolveIntent } = useGameCtx('decision', 'byId', 'def', 'send', 'sending', 'setFocusSource', 'inspectCard', 'setResolveIntent');
  const d = decision;
  const q = effectQueue(decision);
  const [hovered, setHovered] = useState<number | null>(null);
  const [picked, setPicked] = useState<number | null>(null);
  const decisionId = decision?.id;
  useEffect(() => {
    setHovered(null);
    setPicked(null);
  }, [decisionId]);

  const focus = q ? (q.effects.find((c) => c.id === hovered) ?? q.effects.find((c) => c.enabled) ?? q.effects[0]) : null;
  const focusCard = focus?.sourceCardId ?? null;
  useEffect(() => {
    setFocusSource(focusCard);
    return () => setFocusSource(null);
  }, [focusCard, setFocusSource]);

  if (!q || !focus) return null;
  const card = (c: ChoiceOption) => byId.get(c.sourceCardId!);
  const focusDef = def(card(focus));
  const n = q.effects.length;
  const to = (c: ChoiceOption) => resolutionOf(c, q.effects);
  // An effect with steps after it: they are answered for the player, its "you may" as they chose
  const resolve = (c: ChoiceOption, accept = true) => {
    if (!c.enabled || sending) return;
    setPicked(c.id);
    const label = to(c);
    setResolveIntent(label && c.sourceCardId !== undefined ? { source: c.sourceCardId, label, accept } : null);
    void send({ type: 'choice', id: c.id });
  };
  const by = triggeredBy(focus, q.effects);

  return (
    <>
      <div className={`effect-queue ${collapsed ? 'collapsed' : ''}`} aria-hidden={collapsed}>
        <div className="eq-detail" aria-live="polite">
          <div className="eq-detail-head">
            <span className="eq-name">{focusDef?.name ?? 'Effect'}</span>
            <span className="eq-where">{where(card(focus))}</span>
            {by && <span className="eq-by">after {by}</span>}
          </div>
          <RichText text={effectText(focus, q.effects)} />
          {to(focus) && (
            <button type="button" className="eq-skip" disabled={!focus.enabled || sending} onClick={() => resolve(focus, false)}>
              Skip this effect
            </button>
          )}
        </div>
        <section className="eq-list" aria-label="Triggered effects">
          <div className="eq-head">
            <span>
              <b>{n === 1 ? '1 effect' : `${n} effects`}</b> · {n === 1 ? 'click to resolve it' : 'click one to resolve it'}
            </span>
            {q.others.map((c) => (
              <button key={c.id} type="button" className="eq-other" disabled={!c.enabled || sending} onClick={() => void send({ type: 'choice', id: c.id })}>
                <RichText text={c.label} />
              </button>
            ))}
            {d?.canPass && (
              <button type="button" className="eq-other" disabled={sending} onClick={() => void send({ type: 'pass' })}>
                {d.passLabel ?? 'Pass'}
              </button>
            )}
            <button type="button" className="eq-min" onClick={onCollapse} aria-label="Minimize the effects and show my hand" title="Show my hand">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M6 9l6 6 6-6" />
              </svg>
            </button>
          </div>
          <div className="eq-items">
            {q.effects.map((c) => {
              const cd = def(card(c));
              return (
                <button
                  key={c.id}
                  type="button"
                  className={`eq-item ${c.id === focus.id ? 'on' : ''} ${picked === c.id ? 'resolving' : ''}`}
                  disabled={!c.enabled || sending}
                  onMouseEnter={() => setHovered(c.id)}
                  onFocus={() => setHovered(c.id)}
                  onClick={() => resolve(c)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    inspectCard(c.sourceCardId!);
                  }}
                  aria-label={`Resolve ${cd?.name ?? 'this effect'}: ${effectText(c, q.effects)}`}
                >
                  {cd ? <img src={artUrl(cd)} alt="" draggable={false} /> : <span className="eq-noart">{n}</span>}
                </button>
              );
            })}
          </div>
        </section>
      </div>
    </>
  );
}

/** The effects queue minimized: a badge right of the main button, a click brings the queue back */
export function EffectQueueBadge({ onExpand }: { onExpand: () => void }) {
  const { decision, byId, def } = useGameCtx('decision', 'byId', 'def');
  const q = effectQueue(decision);
  if (!q) return null;
  const first = q.effects.find((c) => c.enabled) ?? q.effects[0];
  const firstDef = first?.sourceCardId !== undefined ? def(byId.get(first.sourceCardId)) : null;
  const n = q.effects.length;
  return (
    <button type="button" className="eq-badge" onClick={onExpand} aria-label={`Show the ${n} effects to resolve`} title="Show the effects">
      {firstDef ? <img src={artUrl(firstDef)} alt="" draggable={false} /> : null}
      <span>{n}</span>
    </button>
  );
}
