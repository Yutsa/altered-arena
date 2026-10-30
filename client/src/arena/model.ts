/**
 * Pure helpers of the arena table: which Tumult card each pair of regions shows, and where each
 * expedition is expected to be after Dusk. Kept free of React so they can be tested alone.
 */
import type { Biome, GameView, PlayerView, RegionView, Side } from '@protocol';

/** Biomes of the two sides of each Tumult card, as in the engine (V1.php STORM_CARDS). */
const STORM_CARDS: Biome[][][] = [
  [[], ['forest', 'mountain', 'ocean']],
  [['forest'], ['mountain', 'ocean']],
  [['mountain'], ['forest', 'ocean']],
  [['ocean'], ['mountain', 'forest']],
  [['forest', 'mountain', 'ocean'], []],
];
/** Sprite index of the face-down Tumult card in storm.jpg */
export const STORM_BACK = 6;

export interface StormTile {
  /** sprite index in storm.jpg (0 hero region, 1-3 Tumult, 4 companion region, 6 face down) */
  id: number;
  /** rotated by 180°: its two regions are swapped */
  flipped: boolean;
}

const sameSet = (a: Biome[], b: Biome[]) => a.length === b.length && a.every((x) => b.includes(x));

/** The Tumult card shown at each of the 5 card slots of the track (0: hero region ... 4: companion region). */
export function stormTiles(regions: RegionView[]): StormTile[] {
  const byIndex = new Map(regions.map((r) => [r.index, r]));
  const tiles: StormTile[] = [{ id: 0, flipped: false }];
  for (let k = 1; k <= 3; k++) {
    const left = byIndex.get(2 * k - 1)?.biomes;
    const right = byIndex.get(2 * k)?.biomes;
    let tile: StormTile = { id: STORM_BACK, flipped: false };
    if (left && right) {
      for (let id = 1; id <= 3 && tile.id === STORM_BACK; id++) {
        const [a, b] = STORM_CARDS[id];
        if (sameSet(a, left) && sameSet(b, right)) tile = { id, flipped: false };
        else if (sameSet(b, left) && sameSet(a, right)) tile = { id, flipped: true };
      }
    }
    tiles.push(tile);
  }
  tiles.push({ id: 4, flipped: false });
  return tiles;
}

export interface Forecast {
  /** the expedition moves forward at Dusk if nothing changes */
  advances: boolean;
  /** biomes of its region where it beats the opposing expedition */
  winning: Biome[];
  /** biomes of the region it stands in (null: face down) */
  region: Biome[] | null;
  /** region it would reach */
  next: number;
}

/**
 * Dusk forecast: an expedition moves forward when, in at least one biome of its region, its
 * statistic is greater than the opposing expedition's (same side), or equal to it (and not 0) when
 * it is Ascended. Blocked expeditions stay. The server's answer (`advances`, the engine's own
 * computation, card effects included) wins over this one.
 */
export function forecast(view: GameView, player: PlayerView, side: Side): Forecast {
  const exp = player.expeditions[side];
  const other = view.players.find((p) => p.id !== player.id)?.expeditions[side];
  // A terrain marker (Nadir) gives the region its biome only
  const r = view.regions.find((x) => x.index === exp.position);
  const region = (r?.marker ? [r.marker.biome] : r?.biomes) ?? null;
  const winning = (region ?? []).filter((b) => {
    const mine = exp.stats[b];
    const theirs = other?.stats[b] ?? 0;
    return mine > theirs || (!!exp.ascended && mine === theirs && mine >= 1);
  });
  const advances = (exp.advances ?? (!exp.blocked && winning.length > 0)) && view.phase !== 'ended';
  return { advances, winning, region, next: exp.position + (side === 'hero' ? 1 : -1) };
}

/**
 * Track geometry: 11 equal units, [cap | r0 | r1 r2 | r3 | button | r4 | r5 r6 | r7 | cap].
 * Center of region i, in units from the left.
 */
export const TRACK_UNITS = 11;
export function regionCenter(i: number): number {
  return i <= 3 ? i + 1.5 : i + 2.5;
}

