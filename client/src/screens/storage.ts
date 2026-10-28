import type { PlayerId } from '@protocol';

const KEY_GAME = 'arena.lastGame';
const KEY_NAME = 'arena.name';
const KEY_DECK = 'arena.deck';

function read<T>(k: string): T | null {
  try {
    const v = localStorage.getItem(k);
    return v ? (JSON.parse(v) as T) : null;
  } catch {
    return null;
  }
}
function write(k: string, v: unknown) {
  try {
    localStorage.setItem(k, JSON.stringify(v));
  } catch {
    // storage unavailable (private mode): nothing to remember
  }
}

export interface LastGame {
  gameId: string;
  player: PlayerId;
  token?: string | null;
}
export const rememberGame = (g: LastGame) => write(KEY_GAME, g);
export const lastGame = () => read<LastGame>(KEY_GAME);
export const savedName = () => read<string>(KEY_NAME) ?? '';
export const saveName = (n: string) => write(KEY_NAME, n);
export const savedDeck = () => read<string>(KEY_DECK);
export const saveDeck = (d: string) => write(KEY_DECK, d);
export const forgetGame = (gameId: string) => {
  if (lastGame()?.gameId === gameId) {
    try {
      localStorage.removeItem(KEY_GAME);
    } catch {
      // ignore
    }
  }
};

const KEY_TOKENS = 'arena.tokens';
type Tokens = Record<string, Record<string, string>>;

/** Remembers the secret token of a seat, so that the game can be reopened (or the seat switched in hot seat) */
export function saveToken(gameId: string, player: PlayerId, token: string | null | undefined) {
  if (!token) return;
  const all = read<Tokens>(KEY_TOKENS) ?? {};
  all[gameId] = { ...(all[gameId] ?? {}), [player]: token };
  // keep the 30 most recent games
  const keys = Object.keys(all);
  for (const k of keys.slice(0, Math.max(0, keys.length - 30))) delete all[k];
  write(KEY_TOKENS, all);
}
export function tokenOf(gameId: string, player: PlayerId): string | null {
  return read<Tokens>(KEY_TOKENS)?.[gameId]?.[player] ?? null;
}
