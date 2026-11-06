import { useEffect, useState, type ReactNode } from 'react';
import type { Decision, GameView, TurnDecision } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { choiceKey, cleanupCounts, confirmAnswer, effectQueue, exchangePicks, pickerCardIds, primaryAction, type PrimaryAction } from '../game/decisions';
import { RichText } from './RichText';
import { DeckGrid, SelectPicker } from './Picker';
import { Modal } from './Modal';
import { useDecks } from '../api/catalog';
import './DecisionBar.css';

function Btn({
  children,
  onClick,
  kind = '',
  disabled,
}: {
  children: ReactNode;
  onClick: () => void;
  kind?: string;
  disabled?: boolean;
}) {
  const { sending } = useGameCtx('sending');
  return (
    <button type="button" className={`btn ${kind}`} onClick={onClick} disabled={disabled || sending}>
      {children}
    </button>
  );
}

/** Pass and Undo, common to every decision kind */
function CommonButtons({ d, hidePass }: { d: Decision; hidePass?: boolean }) {
  const { send } = useGameCtx('send');
  return (
    <>
      {d.canPass && !hidePass && <Btn onClick={() => void send({ type: 'pass' })}>{d.passLabel ?? 'Pass'}</Btn>}
    </>
  );
}

function ConfirmButton({ d, label = 'Confirm', hidden }: { d: Decision; label?: string; hidden?: boolean }) {
  const { ui, send } = useGameCtx('ui', 'send');
  if (hidden) return null;
  const a = confirmAnswer(d, ui);
  return (
    <Btn kind="primary" disabled={!a} onClick={() => a && void send(a)}>
      {label}
    </Btn>
  );
}

function TurnControls({ d, hide }: { d: TurnDecision; hide?: PrimaryAction['kind'] }) {
  const { send } = useGameCtx('send');
  const canAct = d.play.length + d.support.length + d.tap.length > 0;
  // A tapped card shows its actions above itself (arena/CardActions.tsx), its zones on the mat
  return (
    <>
      <span className="hint">{canAct ? 'Drag a glowing card to a zone, or tap it.' : 'No card can be played.'}</span>
      <div className="db-buttons">
        <CommonButtons d={d} hidePass={hide === 'pass'} />
        {d.canEndTurn && hide !== 'endTurn' && (
          <Btn
            kind={canAct ? '' : 'primary'}
            onClick={() => void send({ type: 'endTurn' })}
          >
            End turn
          </Btn>
        )}
      </div>
    </>
  );
}

function DeckChooser({ d }: { d: Extract<Decision, { kind: 'deck' }> }) {
  const { catalog, send } = useGameCtx('catalog', 'send');
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
  const { view, ui, send, byId, def } = useGameCtx('view', 'ui', 'send', 'byId', 'def');
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
    case 'number': {
      const die = /die/i.test(d.prompt);
      return (
        <div className={`db-buttons wrap ${die ? 'db-dice' : ''}`}>
          {d.values.map((v) => (
            <Btn key={v} kind={die ? 'primary die-pick' : 'primary'} onClick={() => void send({ type: 'number', value: v })}>
              {v}
            </Btn>
          ))}
          <CommonButtons d={d} hidePass={hidePass} />
        </div>
      );
    }
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
      // The only button is on the track button: the bar shows the prompt alone
      if (hide === 'button') return d.canPass && !hidePass ? <div className="db-buttons"><CommonButtons d={d} /></div> : null;
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
  const { send, repeatChoice } = useGameCtx('send', 'repeatChoice');
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
  const { send, setUI } = useGameCtx('send', 'setUI');
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

/** Zones of the mat where a location is picked (arena/Mat.tsx) */
const BOARD_ZONES = ['reserve', 'landmark', 'heroExpedition', 'companionExpedition'];

/** A decision whose options are all drawn on the board, where they are picked */
function pickedOnBoard(d: Decision): boolean {
  if (d.kind === 'expedition' || d.kind === 'player') return true;
  if (d.kind === 'location') return d.locations.every((l) => l.player !== undefined && BOARD_ZONES.includes(l.zone as string));
  return false;
}

/**
 * The bar has nothing to click: the prompt only tells what to do (the cards are picked on the
 * board or in the hand, the main action is on the track button). An information, not a trigger.
 */
function infoOnly(view: GameView, d: Decision, hide: PrimaryAction['kind'] | undefined): boolean {
  if (d.canPass && hide !== 'pass') return false;
  switch (d.kind) {
    case 'cleanup':
    case 'exchange':
    case 'confirm':
      return hide === 'confirm';
    case 'select':
      return hide === 'confirm' && !pickerCardIds(view, d).length;
    case 'buttons':
      return !!d.markRegion || hide === 'button';
    default:
      return false;
  }
}

/**
 * The prompt of the current decision and its controls; a waiting indicator otherwise.
 * primaryOnTrack: the main action (end turn, confirm, pass) is on another button, not here.
 * only: show the bar only when it is an information (nothing to click in it) or only when it holds
 * controls; the arena draws the first on the track, the second above the hand.
 */
export function DecisionBar({ primaryOnTrack = false, before, only }: { primaryOnTrack?: boolean; before?: ReactNode; only?: 'info' | 'controls' }) {
  const { view, decision, byId, def, sending, inspectCard, auto } = useGameCtx('view', 'decision', 'byId', 'def', 'sending', 'inspectCard', 'auto');
  const [flash, setFlash] = useState(false);
  const decisionId = decision?.id;
  useEffect(() => {
    if (!decisionId) return;
    setFlash(true);
    const t = setTimeout(() => setFlash(false), 700);
    return () => clearTimeout(t);
  }, [decisionId]);

  if (view.phase === 'ended') return null;
  // Nothing to decide: no bar (the track button says whose turn it is). Effects to resolve: the
  // effects queue beside End turn
  if (!decision || effectQueue(decision)) return null;
  // My turn: the glowing cards say it, End turn and Undo have their own buttons beside the hand
  if (primaryOnTrack && decision.kind === 'turn') return null;
  // Picked on the board (expeditions, players, places): nothing here, unless a Pass has no other button
  if (primaryOnTrack && pickedOnBoard(decision) && !(decision.canPass && primaryAction(decision)?.kind !== 'pass')) return null;
  // Cards from hand to mana: the glowing hand says it, Confirm and Pass are on the track button
  if (primaryOnTrack && decision.kind === 'select' && decision.toMana && !pickerCardIds(view, decision).length) return null;
  const hide = primaryOnTrack ? primaryAction(decision)?.kind : undefined;
  const info = (!!auto && !auto.countdown) || (primaryOnTrack && infoOnly(view, decision, hide));
  if (only && (only === 'info') !== info) return null;
  const source = decision.sourceCardId !== undefined ? byId.get(decision.sourceCardId) : undefined;
  const sourceDef = def(source);
  // Answered for the player (no real choice): nothing to click
  if (auto && !auto.countdown)
    return (
      <div className="decision-bar waiting info" aria-live="polite">
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
          <button type="button" className="db-source" onClick={() => inspectCard(source!.id)} aria-label={sourceDef.name}>
            {sourceDef.name}
          </button>
        )}
        <RichText text={auto?.countdown ? 'Your turn ends… Undo to take it back' : decision.prompt} />
      </div>
      {!(primaryOnTrack && primaryAction(decision)?.kind === 'button' && !decision.canPass) && (
        <div className="db-controls">
          <Controls d={decision} hide={hide} />
        </div>
      )}
    </div>
  );
}
