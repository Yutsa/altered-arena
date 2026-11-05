import { useEffect, useMemo } from 'react';
import type { PlayerId } from '@protocol';
import { useCatalog } from '../api/catalog';
import { useGame } from '../game/useGame';
import { InspectProvider } from '../game/inspect';
import { GameProvider } from '../game/GameContext';
import { DragProvider } from '../game/drag';
import { Arena } from '../arena/Arena';
import { ShownTargets } from '../arena/Cast';
import { useCastStage } from '../game/useCastStage';
import { useDuskStage } from '../game/useDuskStage';
import { Inspector } from '../components/Inspector';
import { forgetGame, isSoloGame, rememberGame, tokenOf } from './storage';

export function GameScreen({
  gameId,
  player,
  token,
  onLeave,
  onSwitchPlayer,
}: {
  gameId: string;
  /** null: spectator */
  player: PlayerId | null;
  token: string | null;
  onLeave: () => void;
  onSwitchPlayer: (p: PlayerId) => void;
}) {
  const { catalog, error } = useCatalog();
  const game = useGame(gameId, player, token);
  // The opponents' spells are shown beside the mat, then their targets, before their result
  const stage = useCastStage(game.view, player);
  // The end of the day in steps: Dusk, the hero expeditions, the companion ones, the Night
  const view = useDuskStage(stage.view);
  const refs = useMemo(() => (view ? view.cards.map((c) => c.ref).filter((r): r is string => !!r) : []), [view]);

  useEffect(() => {
    if (view && player !== null && view.you !== null && view.phase !== 'ended') rememberGame({ gameId, player, token });
    else if (view?.phase === 'ended') forgetGame(gameId);
  }, [view, gameId, player, token]);

  // A game against oneself (and only that: never a game against someone else, even with their token
  // on this device): the other seat, and the point of view goes to whoever must decide
  const other = view && isSoloGame(gameId) ? view.players.find((p) => p.id !== player && !p.bot && tokenOf(gameId, p.id)) : undefined;
  const otherTurn = !!other && !!view && view.phase !== 'ended' && view.waitingFor.includes(other.id) && !view.waitingFor.includes(player ?? -1);
  useEffect(() => {
    if (!other || !otherTurn) return;
    const t = setTimeout(() => onSwitchPlayer(other.id), 700);
    return () => clearTimeout(t);
  }, [other, otherTurn, onSwitchPlayer]);

  if (game.fatal || error)
    return (
      <div className="center-screen">
        <h2>Cannot open this game</h2>
        <p className="muted">{game.fatal ?? error}</p>
        <button type="button" className="btn primary" onClick={onLeave}>
          Back to lobby
        </button>
      </div>
    );
  if (!catalog || !view)
    return (
      <div className="center-screen">
        <span className="spinner big" />
        <p className="muted">Joining game {gameId}…</p>
      </div>
    );

  // On the opponent's name: take their seat
  const switcher = other ? (
    <button
      type="button"
      className={`pc-switch ${otherTurn ? 'attn' : ''}`}
      onClick={(e) => {
        e.stopPropagation();
        onSwitchPlayer(other.id);
      }}
      aria-label={`Play as ${other.name}`}
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" aria-hidden="true">
        <path d="M4 8h14l-4-4M20 16H6l4 4" />
      </svg>
      Play
    </button>
  ) : null;
  const spectator = view.you === null ? <span className="spectator-tag">Spectator</span> : null;
  // Against bots: back to before the player's last answer (again and again, down to the start)
  const undo = view.canUndo ? { onUndo: () => void game.undo(), disabled: game.sending } : undefined;
  const extra = spectator ?? undefined;

  return (
    <InspectProvider catalog={catalog} preferredRefs={refs}>
      <GameProvider view={view} catalog={catalog} log={game.log} sending={game.sending} send={game.send} undoneTo={game.undoneTo}>
        <DragProvider>
          <ShownTargets.Provider value={stage.targets}>
            <Arena onLeave={onLeave} connected={game.connected} extraTop={extra} oppAction={switcher} undo={undo} />
          </ShownTargets.Provider>
        </DragProvider>
      </GameProvider>
      <Inspector />
    </InspectProvider>
  );
}
