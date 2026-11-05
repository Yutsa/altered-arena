import type { CardView, GameView } from '@protocol';

const isPlain = (o: unknown): o is Record<string, unknown> => !!o && typeof o === 'object' && Object.getPrototypeOf(o) === Object.prototype;

/**
 * `next`, with every part deep-equal to the same part of `prev` replaced by the `prev` one
 * (structural sharing). Unchanged parts keep their identity: a component that reads one of them
 * is not drawn again. Plain objects and arrays only (JSON data).
 */
export function share<T>(prev: unknown, next: T): T {
  if (prev === next) return prev as T;
  if (Array.isArray(prev) && Array.isArray(next)) {
    let same = prev.length === next.length;
    const out = next.map((v, i) => {
      const s = share(prev[i], v);
      if (s !== prev[i]) same = false;
      return s;
    });
    return (same ? prev : out) as T;
  }
  if (isPlain(prev) && isPlain(next)) {
    const pk = Object.keys(prev);
    const nk = Object.keys(next);
    let same = pk.length === nk.length;
    const out: Record<string, unknown> = {};
    for (const k of nk) {
      const s = share(prev[k], next[k]);
      out[k] = s;
      if (s !== prev[k] || !(k in prev)) same = false;
    }
    return (same ? prev : out) as T;
  }
  return next;
}

/** Cards matched by id (they change order between views), then each one shared */
function shareCards(prev: CardView[], next: CardView[]): CardView[] {
  const byId = new Map(prev.map((c) => [c.id, c]));
  let same = prev.length === next.length;
  const out = next.map((c, i) => {
    const s = share(byId.get(c.id), c);
    if (s !== prev[i]) same = false;
    return s;
  });
  return same ? prev : out;
}

/** A new GameView sharing every unchanged card, player, decision... of the previous one */
export function shareView(prev: GameView | null, next: GameView): GameView {
  if (!prev || prev === next) return next;
  const out: GameView = { ...share({ ...prev, cards: [] }, { ...next, cards: [] }), cards: shareCards(prev.cards, next.cards) };
  const keys = Object.keys(out) as (keyof GameView)[];
  const same = keys.length === Object.keys(prev).length && keys.every((k) => out[k] === prev[k]);
  return same ? prev : out;
}

/** `next` with the entries deep-equal to those of `prev` replaced by them; `prev` itself when nothing changed */
export function shareMap<K, V>(prev: Map<K, V> | null, next: Map<K, V>): Map<K, V> {
  if (!prev) return next;
  let same = prev.size === next.size;
  const out = new Map<K, V>();
  for (const [k, v] of next) {
    const s = share(prev.get(k), v);
    if (s !== prev.get(k)) same = false;
    out.set(k, s);
  }
  return same ? prev : out;
}
