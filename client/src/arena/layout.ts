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
  vars: Record<string, string>;
}

const WIDE: ArenaLayout = {
  mode: 'wide',
  W: 1440,
  H: 900,
  plane: { w: 1860, h: 1000, left: -210, top: -40, perspective: 1800, tilt: 30 },
  rows: [190, 210, 110, 210, 280],
  trackH: 96,
  vars: {
    '--mat-card-w': '112px',
    '--hero-w': '124px',
    '--zone-card-w': '82px',
    '--card-hand-w': '150px',
    '--marker-size': '46px',
    '--tot-icon': '32px',
    '--tot-num': '22px',
    '--chip-font': '14px',
  },
};

const PHONE: ArenaLayout = {
  mode: 'phone',
  W: 844,
  H: 390,
  plane: { w: 1080, h: 470, left: -118, top: -28, perspective: 1000, tilt: 30 },
  rows: [70, 120, 56, 120, 104],
  trackH: 50,
  vars: {
    '--mat-card-w': '62px',
    '--hero-w': '68px',
    '--zone-card-w': '34px',
    '--card-hand-w': '74px',
    '--marker-size': '26px',
    '--tot-icon': '18px',
    '--tot-num': '13px',
    '--chip-font': '11px',
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
  return { height: l.rows[i], ['--bgw' as string]: `${l.plane.w}px`, ['--bgh' as string]: `${imgH}px`, ['--bgy' as string]: `${-f0 * imgH}px` };
}

/**
 * The board image draws a deck in both player zones; this patch covers it with the art found a
 * bit further right (our own piles are drawn elsewhere).
 */
export function deckPatch(l: ArenaLayout, i: 0 | 4): CSSProperties {
  const [f0, f1] = SLICES[i];
  const imgH = l.rows[i] / (f1 - f0);
  const [t0, t1] = i === 0 ? [0.42, 0.97] : [0.02, 0.47];
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

function pick(): { layout: ArenaLayout; scale: number; portrait: boolean } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const portrait = vh > vw;
  const layout = (vh < 560 && !portrait) || vw < 760 ? PHONE : WIDE;
  const scale = Math.min(vw / layout.W, vh / layout.H);
  return { layout, scale, portrait };
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
