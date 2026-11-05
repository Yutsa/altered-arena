import { useEffect, useState, type CSSProperties } from 'react';

/**
 * The table is drawn on a fixed-size stage (virtual pixels) scaled to the window. Two stages: a
 * wide one (desktop, tablet) and a short one for phones held in landscape. The tilted mat is
 * wider than the stage so that its edges leave the screen instead of showing empty sides.
 */
export interface ArenaLayout {
  mode: 'wide' | 'phone';
  /** stage size */
  W: number;
  H: number;
  /** tilted mat: size, position on the stage, perspective, tilt */
  plane: { w: number; h: number; left: number; top: number; perspective: number; tilt: number };
  /** heights of the mat rows, top to bottom: opponent zone, opponent band, track, my band, my zone */
  rows: [number, number, number, number, number];
  trackH: number;
  /** each player's tokens on a line of their own (the opponent's above), as on BGA; else one line */
  twoLanes: boolean;
  vars: Record<string, string>;
}

const WIDE: ArenaLayout = {
  mode: 'wide',
  W: 1440,
  H: 900,
  plane: { w: 1860, h: 1000, left: -210, top: -40, perspective: 1800, tilt: 30 },
  rows: [180, 192, 164, 192, 272],
  trackH: 150,
  twoLanes: true,
  vars: {
    '--mat-card-w': '112px',
    '--hero-w': '124px',
    '--zone-card-w': '90px',
    '--my-zone-card-w': '106px',
    '--hand-shift': '60px',
    '--end-dock-top': '502px',
    '--card-hand-w': '150px',
    '--marker-size': '46px',
    '--tot-icon': '32px',
    '--post-cell': '36px',
    '--post-row': '32px',
    '--post-num': '22px',
    '--post-gap': '4px',
    '--chip-font': '14px',
    '--mana-w': '50px',
    '--mana-slot-h': '14px',
    '--mana-gap': '4px',
    '--opp-card-w': '96px',
    '--opp-hand-x': '50%',
  },
};

const PHONE: ArenaLayout = {
  mode: 'phone',
  W: 844,
  H: 390,
  plane: { w: 1080, h: 470, left: -118, top: -28, perspective: 1000, tilt: 30 },
  rows: [70, 120, 56, 120, 104],
  trackH: 50,
  twoLanes: false,
  vars: {
    '--mat-card-w': '62px',
    '--hero-w': '68px',
    '--zone-card-w': '38px',
    '--my-zone-card-w': '52px',
    '--hand-shift': '28px',
    '--end-dock-top': '236px',
    '--card-hand-w': '74px',
    '--marker-size': '26px',
    '--tot-icon': '18px',
    '--post-cell': '17px',
    '--post-row': '13px',
    '--post-num': '11px',
    '--post-gap': '2px',
    '--chip-font': '11px',
    '--mana-w': '30px',
    '--mana-slot-h': '7px',
    '--mana-gap': '2px',
    '--opp-card-w': '46px',
    '--opp-hand-x': '50%',
  },
};

/** Portion of board.jpg (top..bottom, fractions of its height) drawn behind each mat row */
const SLICES: [number, number][] = [
  [0, 0.252],
  [0.252, 0.383],
  [0.383, 0.592],
  [0.592, 0.74],
  [0.74, 1],
];

/** Background of mat row i: the board image stretched in width only, so rows keep their look. */
export function rowBackground(l: ArenaLayout, i: number): CSSProperties {
  const [f0, f1] = SLICES[i];
  const imgH = l.rows[i] / (f1 - f0);
  return { height: l.rows[i], '--bgw': `${l.plane.w}px`, '--bgh': `${imgH}px`, '--bgy': `${-f0 * imgH}px` };
}

/**
 * The board image draws a deck in both player zones; this patch covers it with the art found a
 * bit further right (our own piles are drawn elsewhere).
 */
export function deckPatch(l: ArenaLayout, i: 0 | 4): CSSProperties {
  const [f0, f1] = SLICES[i];
  const imgH = l.rows[i] / (f1 - f0);
  const [t0, t1] = i === 0 ? [0.4, 1] : [0, 0.58];
  const top = t0 * l.rows[i];
  const left = 0.165 * l.plane.w;
  return {
    left,
    top,
    width: 0.1 * l.plane.w,
    height: (t1 - t0) * l.rows[i],
    backgroundSize: `${l.plane.w}px ${imgH}px`,
    backgroundPosition: `${-(left + 0.14 * l.plane.w)}px ${-f0 * imgH - top}px`,
  };
}

/**
 * Centre of the hero cards, as a fraction of the mat width: the band columns (arena.css,
 * .mat-band) are 19.75% wide on the left and 15% on the right, around two equal expedition
 * columns, following the board image. The Tumult track puts its button there.
 */
export const HERO_X = 0.1975 + (1 - 0.1975 - 0.15) / 2;

/** Safe areas of the screen (notch, rounded corners, home indicator), from the CSS env() values */
export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}
let probe: HTMLDivElement | null = null;
function safeInsets(): Insets {
  if (!probe) {
    probe = document.createElement('div');
    probe.style.cssText =
      'position:fixed;visibility:hidden;pointer-events:none;padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)';
    document.body.appendChild(probe);
  }
  const cs = getComputedStyle(probe);
  return { top: parseFloat(cs.paddingTop) || 0, right: parseFloat(cs.paddingRight) || 0, bottom: parseFloat(cs.paddingBottom) || 0, left: parseFloat(cs.paddingLeft) || 0 };
}

/**
 * The stage fits inside the safe area, but for its top (a phone in landscape has its notch on one
 * side and the home indicator at the bottom: nothing to play there, and a swipe up from the bottom
 * edge leaves the app). `center`: where the stage's centre is, in window px.
 */
function pick(): { layout: ArenaLayout; scale: number; portrait: boolean; center: { x: number; y: number } } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const portrait = vh > vw;
  const i = safeInsets();
  // Both sides keep the same margin, so the table stays centred whatever side the notch is on. The
  // top inset is not kept: in landscape it is the status bar (Android in full screen reports one),
  // over the strip of sky above the mat, and keeping it pushed the whole table down
  const side = Math.max(i.left, i.right);
  const w = vw - 2 * side;
  const h = vh - i.bottom;
  const layout = (vh < 560 && !portrait) || vw < 760 ? PHONE : WIDE;
  const scale = Math.min(w / layout.W, h / layout.H);
  return { layout, scale, portrait, center: { x: vw / 2, y: h / 2 } };
}

export function useArenaLayout() {
  const [s, setS] = useState(pick);
  useEffect(() => {
    const f = () => setS(pick());
    window.addEventListener('resize', f);
    window.addEventListener('orientationchange', f);
    return () => {
      window.removeEventListener('resize', f);
      window.removeEventListener('orientationchange', f);
    };
  }, []);
  return s;
}
