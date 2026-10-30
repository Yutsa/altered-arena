import { useEffect, useState, type ReactNode } from 'react';
import type { Decision, TurnDecision } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { choiceKey, cleanupCounts, confirmAnswer, DESTINATION_LABELS, exchangePicks, pickerCardIds, playCost, primaryAction, type PrimaryAction } from '../game/decisions';
import { RichText } from './RichText';
import { DeckGrid, SelectPicker } from './Picker';
import { Modal } from './Modal';
import { ManaIcon } from './Glyph';
import { useDecks } from '../api/catalog';
import './DecisionBar.css';

function Btn({
  children,
  onClick,
  kind = '',
  disabled,
  title,
}: {
  children: ReactNode;
  onClick: () => void;
  kind?: string;
  disabled?: boolean;
  title?: string;
}) {
  const { sending } = useGameCtx();
  return (
    <button type="button" className={`btn ${kind}`} onClick={onClick} disabled={disabled || sending} title={title}>
      {children}
    </button>
  );
}

/** Pass and Undo, common to every decision kind */
function CommonButtons({ d, hidePass }: { d: Decision; hidePass?: boolean }) {
  const { send } = useGameCtx();
  return (
    <>
      {d.canUndo && (
        <Btn kind="ghost" onClick={() => void send({ type: 'undo' })}>
          Undo
        </Btn>
      )}
      {d.canPass && !hidePass && <Btn onClick={() => void send({ type: 'pass' })}>{d.passLabel ?? 'Pass'}</Btn>}
    </>
  );
}

function ConfirmButton({ d, label = 'Confirm', hidden }: { d: Decision; label?: string; hidden?: boolean }) {
  const { ui, send } = useGameCtx();
  if (hidden) return null;
  const a = confirmAnswer(d, ui);
  return (
    <Btn kind="primary" disabled={!a} onClick={() => a && void send(a)}>
      {label}
    </Btn>
  );
}

/** Actions of the card the player tapped during a turn */
function SelectedCardActions({ d }: { d: TurnDecision }) {
  const { ui, setUI, byId, def, aff, play, send, inspectCard } = useGameCtx();
  if (ui.selected === null) return null;
  const card = byId.get(ui.selected);
  const cd = def(card);
  const a = aff.get(ui.selected);
  const id = ui.selected;
  return (
    <div className="selected-actions">
      <div className="sa-title">
        <b>{cd?.name ?? 'Card'}</b>
        <span className="muted">{card?.zone === 'reserve' ? 'from Reserve' : card?.zone === 'hand' ? 'from hand' : ''}</span>
      </div>
      <div className="sa-buttons">
        {a?.destinations?.map((dest) => {
          const cost = playCost(d, card, cd, dest);
          return (
            <Btn key={dest} kind="primary" onClick={() => play(id, dest)}>
              {DESTINATION_LABELS[dest]}
              {cost !== undefined && (
                <>
                  {' '}
                  <ManaIcon value={cost} />
                </>
              )}
            </Btn>
          );
        })}
        {a?.support && (
          <Btn kind="primary" onClick={() => void send({ type: 'support', cardId: id })}>
            Use support
          </Btn>
        )}
        {a?.tap && (
          <Btn kind="primary" onClick={() => void send({ type: 'tap', cardId: id })}>
            Exhaust ability
          </Btn>
        )}
        <Btn kind="ghost" onClick={() => inspectCard(id)}>
          Zoom
        </Btn>
        <Btn kind="ghost" onClick={() => setUI((u) => ({ ...u, selected: null }))}>
          Cancel
        </Btn>
      </div>
    </div>
  );
}

function TurnControls({ d, hide }: { d: TurnDecision; hide?: PrimaryAction['kind'] }) {
  const { send, ui } = useGameCtx();
  const canAct = d.play.length + d.support.length + d.tap.length > 0;
  if (ui.selected !== null) return <SelectedCardActions d={d} />;
  return (
    <>
      <span className="hint">{canAct ? 'Drag a glowing card to a zone, or tap it.' : 'No card can be played.'}</span>
      <div className="db-buttons">
        <CommonButtons d={d} hidePass={hide === 'pass'} />
        {d.canEndTurn && hide !== 'endTurn' && (
          <Btn
            kind={canAct ? '' : 'primary'}
            onClick={() => void send({ type: 'endTurn' })}
            title="Pass for the rest of the Afternoon"
          >
            End turn
          </Btn>
        )}
      </div>
    </>
  );
}

function DeckChooser({ d }: { d: Extract<Decision, { kind: 'deck' }> }) {
  const { catalog, send } = useGameCtx();
  const [picked, setPicked] = useState<string | null>(null);
  const { decks } = useDecks();
  const list = d.decks.length ? d.decks : (decks ?? []);
  return (
    <Modal
      title={<RichText text={d.prompt} />}
      wide
      footer={
        <Btn kind="primary" disabled={!picked} onClick={() => picked && void send({ type: 'deck', deck: picked })}>
          Play this deck
        </Btn>
      }
    >
      <DeckGrid decks={list} catalog={catalog} selected={picked} onSelect={(x) => setPicked(x.id)} />
    </Modal>
  );
}

function Controls({ d, hide }: { d: Decision; hide?: PrimaryAction['kind'] }) {
  const { view, ui, send, byId, def } = useGameCtx();
  const hideConfirm = hide === 'confirm';
  const hidePass = hide === 'pass';
  switch (d.kind) {
    case 'turn':
      return <TurnControls d={d} hide={hide} />;
    case 'select': {
      const inPicker = pickerCardIds(view, d);
      return (
        <>
          <span className="hint">
            {ui.picked.length} / {d.min === d.max ? d.max : `${d.min}–${d.max}`} selected
            {ui.picked.length > 0 && ': ' + ui.picked.map((id) => def(byId.get(id))?.name ?? `#${id}`).join(', ')}
          </span>
          <div className="db-buttons">
            <CommonButtons d={d} hidePass={hidePass} />
            <ConfirmButton d={d} hidden={hideConfirm} />
          </div>
          {inPicker.length > 0 && <SelectPicker decision={d} cardIds={inPicker} />}
        </>
      );
    }
    case 'choice':
      return <ChoiceButtons d={d} hidePass={hidePass} />;
    case 'confirm':
      return (
        <div className="db-buttons">
          <CommonButtons d={d} hidePass={hidePass} />
          {!hideConfirm && (
            <Btn kind="primary" onClick={() => void send({ type: 'confirm' })}>
              Confirm
            </Btn>
          )}
        </div>
      );
    case 'expedition': {
      const players = new Map(view.players.map((p) => [p.id, p]));
      const single = d.min === 1 && d.max === 1;
      return (
        <div className="db-buttons wrap">
          {d.expeditions.map((o) => {
            const p = players.get(o.player);
            const label = `${o.player === view.you ? 'Your' : `${p?.name ?? 'Opponent'}'s`} ${o.side} expedition`;
            const picked = ui.keys.includes(o.key);
            return (
              <ExpeditionButton key={o.key} d={d} optKey={o.key} label={label} picked={picked} single={single} />
            );
          })}
          <CommonButtons d={d} hidePass={hidePass} />
          {!single && <ConfirmButton d={d} hidden={hideConfirm} />}
        </div>
      );
    }
    case 'player':
      return (
        <div className="db-buttons wrap">
          {d.players.map((pid) => (
            <Btn key={pid} kind="primary" onClick={() => void send({ type: 'player', player: pid })}>
              {pid === view.you ? 'You' : (view.players.find((p) => p.id === pid)?.name ?? `Player ${pid}`)}
            </Btn>
          ))}
          <CommonButtons d={d} hidePass={hidePass} />
        </div>
      );
    case 'number':
      return (
        <div className="db-buttons wrap">
          {d.values.map((v) => (
            <Btn key={v} kind="primary" onClick={() => void send({ type: 'number', value: v })}>
              {v}
            </Btn>
          ))}
          <CommonButtons d={d} hidePass={hidePass} />
        </div>
      );
    case 'location':
      return (
        <div className="db-buttons wrap">
          {d.locations.map((l) => (
            <Btn key={l.key} kind="primary" onClick={() => void send({ type: 'location', key: l.key })}>
              <RichText text={l.label} />
            </Btn>
          ))}
          <CommonButtons d={d} hidePass={hidePass} />
        </div>
      );
    case 'exchange': {
      const p = exchangePicks(d, ui);
      return (
        <>
          <span className="hint">
            Hand: {p.hand !== undefined ? (def(byId.get(p.hand))?.name ?? '✓') : '—'} · Reserve:{' '}
            {p.reserve !== undefined ? (def(byId.get(p.reserve))?.name ?? '✓') : '—'}
          </span>
          <div className="db-buttons">
            <CommonButtons d={d} hidePass={hidePass} />
            <ConfirmButton d={d} label="Exchange" hidden={hideConfirm} />
          </div>
        </>
      );
    }
    case 'cleanup': {
      const c = cleanupCounts(d, ui);
      return (
        <>
          <span className="hint">
            {c.needR > 0 && `Reserve: ${c.reserve.length}/${c.needR}`}
            {c.needR > 0 && c.needL > 0 && ' · '}
            {c.needL > 0 && `Landmarks: ${c.landmarks.length}/${c.needL}`}
          </span>
          <div className="db-buttons">
            <CommonButtons d={d} hidePass={hidePass} />
            <ConfirmButton d={d} label="Discard" hidden={hideConfirm} />
          </div>
        </>
      );
    }
    case 'deck':
      return <DeckChooser d={d} />;
    case 'buttons':
      // Noon of Nadir: the markers are dragged on the table, not picked from a button per region
      if (d.markRegion)
        return (
          <div className="db-buttons wrap">
            <span className="hint">Drag a terrain marker from your hero onto a visible region (or tap it, then the region).</span>
            <CommonButtons d={d} hidePass={hidePass} />
          </div>
        );
      return (
        <div className="db-buttons wrap">
          {d.buttons.map((b) => (
            <Btn key={b.key} kind="primary" onClick={() => void send({ type: 'button', key: b.key })}>
              <RichText text={b.label} />
            </Btn>
          ))}
          <CommonButtons d={d} hidePass={hidePass} />
        </div>
      );
    default: {
      const unknown = d as Decision;
      return (
        <div className="db-buttons">
          <span className="hint">Unsupported decision “{(unknown as { kind: string }).kind}”.</span>
          <CommonButtons d={unknown} />
        </div>
      );
    }
  }
}

/** The effects to resolve; identical ones (The Hunger's boosts...) are one button that resolves them all */
function ChoiceButtons({ d, hidePass }: { d: Extract<Decision, { kind: 'choice' }>; hidePass: boolean }) {
  const { send, repeatChoice } = useGameCtx();
  const groups = new Map<string, typeof d.choices>();
  for (const c of d.choices) {
    const k = c.enabled ? choiceKey(c) : `off|${c.id}`;
    groups.set(k, [...(groups.get(k) ?? []), c]);
  }
  return (
    <div className="db-buttons wrap">
      {[...groups].map(([k, cs]) => (
        <Btn
          key={k}
          kind={cs[0].enabled ? 'primary' : ''}
          disabled={!cs[0].enabled}
          title={cs.length > 1 ? `Resolve all ${cs.length}` : undefined}
          onClick={() => {
            repeatChoice(cs.length > 1 ? k : null);
            void send({ type: 'choice', id: cs[0].id });
          }}
        >
          <RichText text={cs[0].label} />
          {cs.length > 1 && <span className="db-times">×{cs.length}</span>}
        </Btn>
      ))}
      <CommonButtons d={d} hidePass={hidePass} />
    </div>
  );
}

function ExpeditionButton({ d, optKey, label, picked, single }: { d: Extract<Decision, { kind: 'expedition' }>; optKey: string; label: string; picked: boolean; single: boolean }) {
  const { send, setUI } = useGameCtx();
  return (
    <Btn
      kind={picked ? 'primary picked' : 'primary'}
      onClick={() => {
        if (single) void send({ type: 'expedition', keys: [optKey] });
        else setUI((u) => ({ ...u, keys: u.keys.includes(optKey) ? u.keys.filter((k) => k !== optKey) : [...u.keys, optKey].slice(-d.max) }));
      }}
    >
      {label}
    </Btn>
  );
}

/**
 * The prompt of the current decision and its controls; a waiting indicator otherwise.
 * primaryOnTrack: the main action (end turn, confirm, pass) is on another button, not here.
 */
export function DecisionBar({ primaryOnTrack = false, before }: { primaryOnTrack?: boolean; before?: ReactNode }) {
  const { view, decision, byId, def, sending, inspectCard, auto } = useGameCtx();
  const [flash, setFlash] = useState(false);
  const decisionId = decision?.id;
  useEffect(() => {
    if (!decisionId) return;
    setFlash(true);
    const t = setTimeout(() => setFlash(false), 700);
    return () => clearTimeout(t);
  }, [decisionId]);

  if (view.phase === 'ended') return null;
  if (!decision) {
    const names = view.waitingFor
      .filter((p) => p !== view.you)
      .map((p) => view.players.find((x) => x.id === p)?.name ?? `Player ${p}`);
    return (
      <div className="decision-bar waiting" aria-live="polite">
        <span className="spinner" />
        <span>{names.length ? `Waiting for ${names.join(', ')}…` : 'Resolving…'}</span>
      </div>
    );
  }
  const source = decision.sourceCardId !== undefined ? byId.get(decision.sourceCardId) : undefined;
  const sourceDef = def(source);
  // Answered for the player (no real choice): nothing to click
  if (auto && !auto.countdown)
    return (
      <div className="decision-bar waiting" aria-live="polite">
        <span className="spinner" />
        {sourceDef && <b>{sourceDef.name}</b>}
        <RichText text={decision.prompt} />
      </div>
    );
  return (
    <div className={`decision-bar kind-${decision.kind} ${flash ? 'flash' : ''} ${sending ? 'sending' : ''}`} aria-live="polite">
      {/* Sent when the bar runs out, unless the player undoes first */}
      {auto?.countdown && !sending && <span key={decision.id} className="db-countdown" style={{ animationDuration: `${auto.delay}ms` }} />}
      <div className="db-prompt">
        {before}
        {sourceDef && (
          <button type="button" className="db-source" onClick={() => inspectCard(source!.id)} title={sourceDef.name}>
            {sourceDef.name}
          </button>
        )}
        <RichText text={auto?.countdown ? 'Your turn ends… Undo to take it back' : decision.prompt} />
      </div>
      <div className="db-controls">
        <Controls d={decision} hide={primaryOnTrack ? primaryAction(decision)?.kind : undefined} />
      </div>
    </div>
  );
}
