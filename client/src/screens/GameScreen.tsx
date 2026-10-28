import { useEffect, useMemo } from 'react';
import type { PlayerId } from '@protocol';
import { useCatalog } from '../api/catalog';
import { useGame } from '../game/useGame';
import { InspectProvider } from '../game/inspect';
import { GameProvider } from '../game/GameContext';
import { DragProvider } from '../game/drag';
import { Table } from '../components/Table';
import { Inspector } from '../components/Inspector';
import { forgetGame, rememberGame } from './storage';

export function GameScreen({
  gameId,
  player,
  onLeave,
  onSwitchPlayer,
}: {
  gameId: string;
  player: PlayerId;
  onLeave: () => void;
  onSwitchPlayer: (p: PlayerId) => void;
}) {
  const { catalog, error } = useCatalog();
  const game = useGame(gameId, player);
  const { view } = game;
  const refs = useMemo(() => (view ? view.cards.map((c) => c.ref).filter((r): r is string => !!r) : []), [view]);

  useEffect(() => {
    if (view && view.phase !== 'ended') rememberGame({ gameId, player });
    else if (view) forgetGame(gameId);
  }, [view, gameId, player]);

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

  // Hot seat on one device: switch to the other human player
  const other = view.players.find((p) => p.id !== player && !p.bot);
  const otherWaiting = other && view.waitingFor.includes(other.id);
  const switcher = other ? (
    <button
      type="button"
      className={`btn ghost switch-player ${otherWaiting ? 'attn' : ''}`}
      onClick={() => onSwitchPlayer(other.id)}
      title={`Play as ${other.name}`}
    >
      ⇄ {other.name}
    </button>
  ) : null;

  return (
    <InspectProvider catalog={catalog} preferredRefs={refs}>
      <GameProvider view={view} catalog={catalog} log={game.log} sending={game.sending} send={game.send}>
        <DragProvider>
          <Table onLeave={onLeave} connected={game.connected} extraTop={switcher} />
        </DragProvider>
      </GameProvider>
      <Inspector />
    </InspectProvider>
  );
}
