import type { Biome } from '@protocol';

/** Glyphs of the Altered icon font (public/fonts/icomoon.woff). Layers are drawn on top of each other. */
const GLYPHS: Record<string, { c: string; white?: boolean }[]> = {
  played: [{ c: '' }],
  hand: [{ c: '' }],
  tap: [{ c: '' }],
  forest: [{ c: '' }],
  mountain: [{ c: '' }],
  ocean: [{ c: '' }],
  anchored: [{ c: '' }],
  fleeting: [{ c: '' }],
  sleep: [{ c: '' }],
  charge: [{ c: '' }],
  spell: [{ c: '' }],
  permanent: [{ c: '' }],
  card: [{ c: '' }],
  discard: [{ c: '' }, { c: '', white: true }],
  infinity: [{ c: '' }, { c: '' }],
  reserve: [
    { c: '' },
    { c: '' },
    { c: '' },
    { c: '' },
    { c: '' },
    { c: '' },
    { c: '', white: true },
    { c: '' },
  ],
};

export type GlyphName = keyof typeof GLYPHS;

export function Glyph({ name, title, className }: { name: string; title?: string; className?: string }) {
  const layers = GLYPHS[name];
  if (!layers) return null;
  return (
    <span className={`glyph glyph-${name} ${className ?? ''}`} title={title} aria-label={title ?? name} role="img">
      {layers.map((l, i) => (
        <span key={i} className={i === 0 ? 'glyph-base' : l.white ? 'glyph-layer glyph-white' : 'glyph-layer'}>
          {l.c}
        </span>
      ))}
    </span>
  );
}

export function BiomeGlyph({ biome, className }: { biome: Biome; className?: string }) {
  return <Glyph name={biome} title={biome} className={`biome-${biome} ${className ?? ''}`} />;
}

/** Mana cost as a round token, e.g. {2} */
export function ManaIcon({ value }: { value: string | number }) {
  return (
    <span className="mana-icon" aria-label={`${value} mana`}>
      {value}
    </span>
  );
}
