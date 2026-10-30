import { useEffect, useState } from 'react';
import { useGameCtx } from '../game/GameContext';
import { confirmAnswer, primaryAction } from '../game/decisions';

/**
 * The round button in the middle of the track: the main action of the current decision (end
 * the turn, confirm, pass), or who the game is waiting for.
 */
export function TrackButton() {
  const { view, decision, ui, send, sending, you, auto } = useGameCtx();
  const [armed, setArmed] = useState(false);
  const decisionId = decision?.id;
  useEffect(() => setArmed(false), [decisionId]);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 2500);
    return () => clearTimeout(t);
  }, [armed]);

  if (view.phase === 'ended') return <span className="track-btn idle">End</span>;
  if (!decision || auto) {
    const waitingOnOther = view.waitingFor.some((p) => p !== you);
    return (
      <span className="track-btn idle" aria-live="polite">
        <span className="tb-ring" />
        <span className="tb-small">{waitingOnOther ? 'Opponent' : 'Resolving'}</span>
      </span>
    );
  }
  const p = primaryAction(decision);
  let label = 'Choose';
  let run: (() => void) | null = null;
  let hot = false;
  if (p?.kind === 'endTurn' && decision.kind === 'turn') {
    const canAct = decision.play.length + decision.support.length + decision.tap.length > 0;
    label = armed ? 'Sure?' : 'End turn';
    hot = !canAct;
    run = () => {
      if (canAct && !armed) setArmed(true);
      else void send({ type: 'endTurn' });
    };
  } else if (p?.kind === 'confirm') {
    const a = confirmAnswer(decision, ui);
    label = p.label;
    hot = !!a;
    if (a) run = () => void send(a);
  } else if (p?.kind === 'pass') {
    label = p.label;
    run = () => void send({ type: 'pass' });
  }
  return (
    <button
      type="button"
      className={`track-btn ${hot ? 'hot' : ''} ${armed ? 'armed' : ''} ${decision.kind === 'turn' ? 'your-turn' : ''}`}
      disabled={!run || sending}
      onClick={run ?? undefined}
    >
      {decision.kind === 'turn' && <span className="tb-small">Your turn</span>}
      <span className="tb-label">{label}</span>
    </button>
  );
}
