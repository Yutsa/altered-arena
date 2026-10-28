import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import type { PlayerId } from '@protocol';
import { Lobby } from './screens/Lobby';
import { GameScreen } from './screens/GameScreen';
import { Toasts } from './components/Toasts';
import { saveToken, tokenOf } from './screens/storage';

// Dev-only page replaying recorded GameViews (excluded from production builds)
const ReplayScreen = import.meta.env.DEV ? lazy(() => import('./dev/ReplayScreen')) : null;

type Route = { name: 'lobby' } | { name: 'game'; gameId: string; player: PlayerId | null; token: string | null } | { name: 'replay' };

/** URL of a seat: ?game=ID&player=PID&token=T (the token is the seat's secret; without it the view is a spectator view) */
export function gameSearch(gameId: string, player: PlayerId, token?: string | null): string {
  return `?game=${encodeURIComponent(gameId)}&player=${player}${token ? `&token=${encodeURIComponent(token)}` : ''}`;
}

function parse(): Route {
  const q = new URLSearchParams(location.search);
  const game = q.get('game');
  const player = Number(q.get('player'));
  if (game && player) {
    const token = q.get('token') ?? tokenOf(game, player);
    saveToken(game, player, token);
    return { name: 'game', gameId: game, player, token };
  }
  if (game) return { name: 'game', gameId: game, player: null, token: null }; // spectator
  if (q.has('replay') && ReplayScreen) return { name: 'replay' };
  return { name: 'lobby' };
}

export function App() {
  const [route, setRoute] = useState<Route>(parse);
  useEffect(() => {
    const f = () => setRoute(parse());
    window.addEventListener('popstate', f);
    return () => window.removeEventListener('popstate', f);
  }, []);
  const go = useCallback((search: string, replace = false) => {
    const url = `${location.pathname}${search}`;
    if (replace) history.replaceState(null, '', url);
    else history.pushState(null, '', url);
    setRoute(parse());
  }, []);

  let screen;
  if (route.name === 'game')
    screen = (
      <GameScreen
        key={`${route.gameId}:${route.player}`}
        gameId={route.gameId}
        player={route.player}
        token={route.token}
        onLeave={() => go('')}
        onSwitchPlayer={(p) => go(gameSearch(route.gameId, p, tokenOf(route.gameId, p)), true)}
      />
    );
  else if (route.name === 'replay' && ReplayScreen)
    screen = (
      <Suspense fallback={null}>
        <ReplayScreen />
      </Suspense>
    );
  else
    screen = (
      <Lobby
        onJoin={(g, p, t) => {
          saveToken(g, p, t);
          go(gameSearch(g, p, t ?? tokenOf(g, p)));
        }}
      />
    );

  return (
    <>
      {screen}
      <Toasts />
    </>
  );
}
