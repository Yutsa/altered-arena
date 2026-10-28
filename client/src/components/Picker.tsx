import { useState } from 'react';
import type { DeckSummary, SelectDecision } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { togglePick, confirmAnswer } from '../game/decisions';
import { useInspect } from '../game/inspect';
import { Modal } from './Modal';
import { Card } from './Card';
import { RichText } from './RichText';
import { FACTION_NAMES } from '../api/catalog';
import type { CardCatalog, Faction } from '@protocol';
import './Picker.css';

const ZONE_LABELS: Record<string, string> = {
  hand: 'Hand',
  deck: 'Deck',
  reserve: 'Reserve',
  landmark: 'Landmarks',
  heroExpedition: 'Hero expedition',
  companionExpedition: 'Companion expedition',
  discard: 'Discard',
  mana: 'Mana',
  limbo: 'In play',
  hero: 'Hero',
};

/** Read-only list of cards (discard pile, mana...) */
export function CardListModal({ title, cardIds, onClose }: { title: string; cardIds: number[]; onClose: () => void }) {
  const { byId, def, inspectCard } = useGameCtx();
  return (
    <Modal title={title} onClose={onClose} wide>
      <div className="picker-grid">
        {cardIds.map((id) => {
          const c = byId.get(id);
          return (
            <button type="button" key={id} className="picker-item" onClick={() => inspectCard(id)}>
              <Card def={def(c)} card={c} size="small" />
            </button>
          );
        })}
        {cardIds.length === 0 && <p className="muted">No cards.</p>}
      </div>
    </Modal>
  );
}

/** A select decision over cards that are not on the table (or showInPicker). */
export function SelectPicker({ decision, cardIds }: { decision: SelectDecision; cardIds: number[] }) {
  const { byId, def, ui, setUI, send, sending, inspectCard } = useGameCtx();
  const [hidden, setHidden] = useState(false);
  const answer = confirmAnswer(decision, ui);
  if (hidden)
    return (
      <button type="button" className="btn primary picker-reopen" onClick={() => setHidden(false)}>
        Show choices ({ui.picked.length}/{decision.max})
      </button>
    );
  return (
    <Modal
      title={<RichText text={decision.prompt} />}
      wide
      footer={
        <>
          <span className="picker-count">
            {ui.picked.length} selected · {decision.min === decision.max ? decision.min : `${decision.min}–${decision.max}`}
          </span>
          <button type="button" className="btn ghost" onClick={() => setHidden(true)}>
            View board
          </button>
          {decision.canUndo && (
            <button type="button" className="btn ghost" disabled={sending} onClick={() => void send({ type: 'undo' })}>
              Undo
            </button>
          )}
          {decision.canPass && (
            <button type="button" className="btn" disabled={sending} onClick={() => void send({ type: 'pass' })}>
              {decision.passLabel ?? 'Pass'}
            </button>
          )}
          <button type="button" className="btn primary" disabled={!answer || sending} onClick={() => answer && void send(answer)}>
            Confirm
          </button>
        </>
      }
    >
      <div className="picker-grid">
        {cardIds.map((id) => {
          const c = byId.get(id);
          const picked = ui.picked.includes(id);
          const cost = decision.costs?.[id];
          return (
            <div key={id} className={`picker-item selectable ${picked ? 'picked' : ''}`}>
              <button type="button" className="picker-card" onClick={() => setUI((u) => togglePick(decision, u, id))} aria-pressed={picked}>
                <Card def={def(c)} card={c} size="small" extraCost={cost} />
              </button>
              <div className="picker-meta">
                {c ? <span>{ZONE_LABELS[c.zone] ?? c.zone}</span> : <span>Unknown card #{id}</span>}
                {c?.ref && (
                  <button type="button" className="btn ghost tiny" onClick={() => inspectCard(id)}>
                    Zoom
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </Modal>
  );
}

const FACTION_ORDER: Faction[] = ['AX', 'BR', 'LY', 'MU', 'OD', 'YZ'];

/** Starter decks grouped by faction, each shown with its hero card. */
export function DeckGrid({
  decks,
  catalog,
  selected,
  onSelect,
}: {
  decks: DeckSummary[];
  catalog: CardCatalog;
  selected?: string | null;
  onSelect: (d: DeckSummary) => void;
}) {
  const { show } = useInspect();
  return (
    <div className="deck-grid">
      {FACTION_ORDER.map((f) => {
        const list = decks.filter((d) => d.faction === f);
        if (!list.length) return null;
        return (
          <section key={f} className={`deck-faction f-${f}`}>
            <h3>{FACTION_NAMES[f]}</h3>
            <div className="deck-row">
              {list.map((d) => {
                const hero = catalog[d.heroRef];
                return (
                  <button
                    type="button"
                    key={d.id}
                    className={`deck-item ${selected === d.id ? 'picked' : ''}`}
                    onClick={() => onSelect(d)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      if (hero) show({ def: hero, mode: 'modal' });
                    }}
                    aria-pressed={selected === d.id}
                  >
                    <Card def={hero} size="small" />
                    <span className="deck-name">{d.name}</span>
                  </button>
                );
              })}
            </div>
          </section>
        );
      })}
    </div>
  );
}
