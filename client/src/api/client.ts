import type {
  Answer,
  CardCatalog,
  DeckSummary,
  ErrorResponse,
  GameCreated,
  GameView,
  MatchTicket,
  NewGame,
  PlayerId,
} from '@protocol';

/** An error returned by the server ({error, user}) or a network failure. */
export class ApiError extends Error {
  readonly status: number;
  /** true when the move was refused (the player may retry), false for server or network errors */
  readonly user: boolean;
  constructor(message: string, status: number, user: boolean) {
    super(message);
    this.status = status;
    this.user = user;
  }
}

const BASE = '/api/v1';

/** Query string of a seat: player id and its secret token (without a token the view is a spectator view) */
function seat(player: PlayerId | null, token?: string | null): string {
  const q = new URLSearchParams();
  if (player !== null) q.set('player', String(player));
  if (token) q.set('token', token);
  return q.toString();
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(BASE + path, {
      ...init,
      headers: init?.body ? { 'Content-Type': 'application/json' } : undefined,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') throw e;
    throw new ApiError('Network error: the game server is unreachable', 0, false);
  }
  let body: unknown = null;
  try {
    body = await res.json();
  } catch {
    // non-JSON body
  }
  if (!res.ok || (body && typeof body === 'object' && 'error' in body)) {
    const err = (body ?? {}) as Partial<ErrorResponse>;
    // The protocol's `user` flag tells a refused move from a server failure. When the server omits it,
    // fall back on the HTTP status: 4xx = refused, 5xx = failure.
    const user = err.user ?? (res.status >= 400 && res.status < 500);
    throw new ApiError(err.error ?? `HTTP ${res.status}`, res.status, user);
  }
  return body as T;
}

export const api = {
  cards: () => request<CardCatalog>('/cards'),
  decks: () => request<DeckSummary[]>('/decks'),
  createGame: (game: NewGame) => request<GameCreated>('/games', { method: 'POST', body: JSON.stringify(game) }),
  /** With `since`, the server waits (up to ~20 s) for a view newer than `since`. */
  view: (gameId: string, player: PlayerId | null, token: string | null | undefined, since?: number, signal?: AbortSignal) =>
    request<GameView>(
      `/games/${encodeURIComponent(gameId)}?${seat(player, token)}${since !== undefined ? `&since=${since}` : ''}`,
      { signal },
    ),
  answer: (gameId: string, player: PlayerId | null, token: string | null | undefined, answer: Answer) =>
    request<GameView>(`/games/${encodeURIComponent(gameId)}/answer?${seat(player, token)}`, {
      method: 'POST',
      body: JSON.stringify(answer),
    }),
  /** Joins the matchmaking queue */
  matchmake: (name: string, deck: string) =>
    request<MatchTicket>('/matchmaking', { method: 'POST', body: JSON.stringify({ name, deck }) }),
  /** Long poll (up to ~20 s) until the ticket is matched */
  ticket: (ticket: string, signal?: AbortSignal) => request<MatchTicket>(`/matchmaking/${encodeURIComponent(ticket)}`, { signal }),
  leaveQueue: (ticket: string) => request<unknown>(`/matchmaking/${encodeURIComponent(ticket)}`, { method: 'DELETE' }).catch(() => null),
};
