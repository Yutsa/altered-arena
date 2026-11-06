import { useMemo, useState, type FormEvent } from 'react';
import type { CardCatalog } from '@protocol';
import { Modal } from './Modal';
import './DebugDialog.css';

const POSITIONS = ['HAND', 'STORMLEFT', 'STORMRIGHT', 'RESERVE', 'LANDMARK', 'MANA'] as const;
export type DebugPosition = (typeof POSITIONS)[number];

export type DebugCard = { ref: string; location: DebugPosition };

const POS = new Set<string>(POSITIONS);

/** One card per line: `LY_Common_DoubleCross HAND` (order of the two tokens does not matter). */
export function parseSetup(text: string): DebugCard[] {
  const out: DebugCard[] = [];
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line || line.startsWith('#')) continue;
    const parts = line.split(/[\s,;:]+/).filter(Boolean);
    if (parts.length < 2) throw new Error(`Line ${i + 1}: expected CARD_ID and POSITION`);
    const locAt = parts.findIndex((p) => POS.has(p.toUpperCase()));
    if (locAt < 0) throw new Error(`Line ${i + 1}: unknown position (use ${POSITIONS.join(', ')})`);
    const location = parts[locAt].toUpperCase() as DebugPosition;
    const ref = parts.filter((_, j) => j !== locAt).join('_');
    if (!ref) throw new Error(`Line ${i + 1}: missing card id`);
    out.push({ ref, location });
  }
  if (!out.length) throw new Error('No cards to add');
  return out;
}

export function DebugDialog({
  catalog,
  busy,
  disabled,
  onClose,
  onAdd,
}: {
  catalog: CardCatalog;
  busy: boolean;
  disabled?: string;
  onClose: () => void;
  onAdd: (cards: DebugCard[]) => Promise<boolean>;
}) {
  const refs = useMemo(() => Object.keys(catalog).sort(), [catalog]);
  const [cardId, setCardId] = useState('');
  const [position, setPosition] = useState<DebugPosition>('HAND');
  const [setup, setSetup] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const add = async (cards: DebugCard[]) => {
    setError(null);
    setMessage(null);
    const ok = await onAdd(cards);
    if (ok) setMessage(cards.length === 1 ? `Added ${cards[0].ref} to ${cards[0].location}` : `Added ${cards.length} cards`);
  };

  const addOne = (e: FormEvent) => {
    e.preventDefault();
    const ref = cardId.trim();
    if (!ref) {
      setError('Card id is required');
      return;
    }
    void add([{ ref, location: position }]);
  };

  const addSetup = (e: FormEvent) => {
    e.preventDefault();
    try {
      void add(parseSetup(setup));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <Modal title="Debug" onClose={onClose}>
      {disabled ? (
        <p className="muted">{disabled}</p>
      ) : (
        <div className="debug-dialog">
          <section className="debug-section">
            <h3>Add card</h3>
            <form className="debug-add" onSubmit={addOne}>
              <label className="field">
                <span>Card id</span>
                <input
                  list="debug-card-refs"
                  value={cardId}
                  onChange={(e) => setCardId(e.target.value)}
                  placeholder="LY_Common_DoubleCross"
                  autoComplete="off"
                  spellCheck={false}
                />
                <datalist id="debug-card-refs">
                  {refs.map((r) => (
                    <option key={r} value={r} />
                  ))}
                </datalist>
              </label>
              <label className="field">
                <span>Position</span>
                <select value={position} onChange={(e) => setPosition(e.target.value as DebugPosition)}>
                  {POSITIONS.map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
                </select>
              </label>
              <button type="submit" className="btn primary" disabled={busy}>
                Add
              </button>
            </form>
          </section>
          <section className="debug-section">
            <h3>Setup</h3>
            <form className="debug-setup" onSubmit={addSetup}>
              <label className="field">
                <span>Cards by id and position (one per line)</span>
                <textarea
                  value={setup}
                  onChange={(e) => setSetup(e.target.value)}
                  placeholder={'LY_Common_DoubleCross HAND\nAX_Common_Oddball STORMLEFT'}
                  spellCheck={false}
                  rows={8}
                />
              </label>
              <button type="submit" className="btn primary" disabled={busy}>
                Add all
              </button>
            </form>
          </section>
          {error && <p className="debug-error">{error}</p>}
          {message && <p className="debug-ok">{message}</p>}
        </div>
      )}
    </Modal>
  );
}
