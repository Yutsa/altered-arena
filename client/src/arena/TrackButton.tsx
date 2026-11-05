import { useGameCtx } from '../game/GameContext';
import { confirmAnswer, effectQueue, primaryAction } from '../game/decisions';

/**
 * The pill in the bottom-right corner, by the hand: the main action of the current decision (end
 * the turn, confirm, pass), or who the game is waiting for.
 */
export function TrackButton() {
  const { view, decision, ui, send, sending, you, auto } = useGameCtx('view', 'decision', 'ui', 'send', 'sending', 'you', 'auto');

  if (view.phase === 'ended') return <span className="track-btn idle">End</span>;
  // Answered for the player; the end of a turn keeps its button (confirm now). Nothing to do: a
  // greyed button that says whose turn it is
  if (!decision || (auto && !auto.countdown)) {
    const waitingOnOther = view.waitingFor.some((p) => p !== you);
    return (
      <button type="button" className="track-btn" disabled aria-live="polite">
        <span className="tb-label">{waitingOnOther ? "Opponent's turn" : 'Resolving'}</span>
      </button>
    );
  }
  // Effects to resolve: they are picked in the effects queue
  if (effectQueue(decision))
    return (
      <button type="button" className="track-btn" disabled>
        <span className="tb-small">Effects</span>
        <span className="tb-label">Pick one</span>
      </button>
    );
  const p = primaryAction(decision);
  let label = 'Choose';
  let run: (() => void) | null = null;
  let hot = false;
  if (p?.kind === 'endTurn' && decision.kind === 'turn') {
    const canAct = decision.play.length + decision.support.length + decision.tap.length > 0;
    label = 'End turn';
    hot = !canAct;
    run = () => void send({ type: 'endTurn' });
  } else if (p?.kind === 'confirm' && decision.kind === 'select' && decision.toMana && decision.canPass && !ui.picked.length) {
    // Cards to mana have no bar (DecisionBar): nothing picked yet, the button passes
    label = decision.passLabel ?? 'Pass';
    run = () => void send({ type: 'pass' });
  } else if (p?.kind === 'confirm') {
    const a = confirmAnswer(decision, ui);
    label = p.label;
    hot = !!a;
    if (a) run = () => void send(a);
  } else if (p?.kind === 'button') {
    label = p.label;
    hot = true;
    run = () => void send({ type: 'button', key: p.key });
  } else if (p?.kind === 'pass') {
    label = p.label;
    run = () => void send({ type: 'pass' });
  }
  return (
    <button
      type="button"
      className={`track-btn ${hot ? 'hot' : ''} ${decision.kind === 'turn' ? 'your-turn' : ''}`}
      disabled={!run || sending}
      onClick={run ?? undefined}
    >
      {decision.kind === 'turn' && <span className="tb-small">Your turn</span>}
      <span className="tb-label">{label}</span>
    </button>
  );
}
