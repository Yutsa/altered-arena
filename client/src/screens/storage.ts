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
