import { lazy, Suspense, useCallback, useEffect, useState } from 'react';
import type { PlayerId } from '@protocol';
import { Lobby } from './screens/Lobby';
import { GameScreen } from './screens/GameScreen';
import { Toasts } from './components/Toasts';

// Dev-only page replaying recorded GameViews (excluded from production builds)
const ReplayScreen = import.meta.env.DEV ? lazy(() => import('./dev/ReplayScreen')) : null;

type Route = { name: 'lobby' } | { name: 'game'; gameId: string; player: PlayerId } | { name: 'replay' };

function parse(): Route {
  const q = new URLSearchParams(location.search);
  const game = q.get('game');
  const player = Number(q.get('player'));
  if (game && player) return { name: 'game', gameId: game, player };
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
        onLeave={() => go('')}
        onSwitchPlayer={(p) => go(`?game=${encodeURIComponent(route.gameId)}&player=${p}`, true)}
      />
    );
  else if (route.name === 'replay' && ReplayScreen)
    screen = (
      <Suspense fallback={null}>
        <ReplayScreen />
      </Suspense>
    );
  else screen = <Lobby onJoin={(g, p) => go(`?game=${encodeURIComponent(g)}&player=${p}`)} />;

  return (
    <>
      {screen}
      <Toasts />
    </>
  );
}
