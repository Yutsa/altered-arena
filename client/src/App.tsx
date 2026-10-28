import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import type { PlayerId } from '@protocol';
import { Lobby } from './screens/Lobby';
import { GameScreen } from './screens/GameScreen';
import { Toasts } from './components/Toasts';
import { saveToken, tokenOf } from './screens/storage';

// Dev-only page replaying recorded GameViews (excluded from production builds)
const ReplayScreen = import.meta.env.DEV ? lazy(() => import('./dev/ReplayScreen')) : null;

type Route = { name: 'lobby' } | { name: 'game'; gameId: string; player: PlayerId | null; token: string | null } | { name: 'replay' };

/**
 * URL of a seat: ?game=ID&player=PID. Its secret token stays on this device (localStorage); share
 * links carry it in the fragment (#token=T), which browsers never send to the server.
 */
export function gameSearch(gameId: string, player: PlayerId, token?: string | null): string {
  saveToken(gameId, player, token);
  return `?game=${encodeURIComponent(gameId)}&player=${player}`;
}

function parse(): Route {
  const q = new URLSearchParams(location.search);
  const game = q.get('game');
  const player = Number(q.get('player'));
  if (game && player) {
    const fromLink = new URLSearchParams(location.hash.slice(1)).get('token') ?? q.get('token');
    if (fromLink) {
      saveToken(game, player, fromLink);
      // Keep the secret out of the address bar and history
      history.replaceState(history.state, '', `${location.pathname}?game=${encodeURIComponent(game)}&player=${player}`);
    }
    const token = tokenOf(game, player);
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
