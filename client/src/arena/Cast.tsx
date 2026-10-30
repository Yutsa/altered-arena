import { useEffect, useRef, useState } from 'react';
import { useGameCtx } from '../game/GameContext';
import { useCardPointer } from '../game/useCardPointer';
import { Card } from '../components/Card';

/**
 * The card being cast (in limbo), shown large beside the mat while its effects wait for their
 * targets, as on BGA. When it leaves limbo, the arena flies it from here to its new place.
 */
export function CastFocus() {
  const { view, def, decision, you } = useGameCtx();
  const limbo = view.cards.filter((c) => c.zone === 'limbo');
  const card = limbo.find((c) => c.id === decision?.sourceCardId) ?? limbo.at(-1);
  if (!card) return null;
  const owner = card.controller ?? card.owner;
  const who = owner === you ? 'You cast' : `${view.players.find((p) => p.id === owner)?.name ?? 'Opponent'} casts`;
  return (
    <div className="cast-focus" key={card.id}>
      <span className="cast-who">{who}</span>
      <FocusCard id={card.id} />
      <span className="cast-name">{def(card)?.name}</span>
    </div>
  );
}

function FocusCard({ id }: { id: number }) {
  const { byId, def } = useGameCtx();
  const { handlers } = useCardPointer(id);
  const card = byId.get(id);
  return (
    <div className="cast-card" data-card-id={id} {...handlers}>
      <Card def={def(card)} card={card} size="hand" />
    </div>
  );
}

interface Arrow {
  key: string;
  d: string;
  /** the arrowhead, at the end of the curve */
  head: string;
  x: number;
  y: number;
  /** picked (solid) or only hovered (a preview, fainter) */
  picked: boolean;
}

/** Where an arrow leaves the card being cast and where it ends on the target: a curve rising over the table */
function arrowPath(from: DOMRect, to: DOMRect, size: number): { d: string; head: string; x: number; y: number } {
  const tx = to.left + to.width / 2;
  const ty = to.top + to.height / 2;
  const right = tx > from.left + from.width / 2;
  const sx = right ? from.right - 6 : from.left + 6;
  const sy = from.top + from.height * 0.42;
  // Stop at the edge of the target, not on its centre
  const ang = Math.atan2(ty - sy, tx - sx);
  const stop = Math.min(to.width, to.height) * 0.3;
  const ex = tx - Math.cos(ang) * stop;
  const ey = ty - Math.sin(ang) * stop;
  const dist = Math.hypot(ex - sx, ey - sy);
  const cx = (sx + ex) / 2;
  const cy = Math.min(sy, ey) - Math.min(160, dist * 0.28);
  // The head points along the end of the curve (from the control point to the end)
  const a = Math.atan2(ey - cy, ex - cx);
  const pt = (r: number, da: number) => `${(ex + Math.cos(a + da) * r).toFixed(1)},${(ey + Math.sin(a + da) * r).toFixed(1)}`;
  const head = `${pt(size * 0.4, 0)} ${pt(size, Math.PI - 0.5)} ${pt(size * 0.45, Math.PI)} ${pt(size, Math.PI + 0.5)}`;
  // The line stops inside the head, so its round cap does not show past the tip
  const lx = ex - Math.cos(a) * size * 0.4;
  const ly = ey - Math.sin(a) * size * 0.4;
  const f = (n: number) => n.toFixed(1);
  return { d: `M ${f(sx)} ${f(sy)} Q ${f(cx)} ${f(cy)} ${f(lx)} ${f(ly)}`, head, x: ex, y: ey };
}

/** The element standing for a target on the table: a card, or an expedition */
function targetEl(target: { card?: number; exp?: string }): HTMLElement | null {
  if (target.card !== undefined) return document.querySelector<HTMLElement>(`.arena-stage [data-card-id="${target.card}"]:not(.cast-card)`);
  return document.querySelector<HTMLElement>(`[data-exp-key="${CSS.escape(target.exp!)}"]`);
}

/**
 * Arrows from the card being cast to its targets: solid to the picked ones, fainter to the one under
 * the pointer (what a click would pick). Drawn over the whole window and measured every frame, so
 * they follow the cards (hover, flights, resize).
 */
export function TargetArrows() {
  const { view, decision, ui } = useGameCtx();
  const casting = view.cards.some((c) => c.zone === 'limbo');
  const kind = decision?.kind;
  const active = casting && (kind === 'select' || kind === 'expedition');
  const [arrows, setArrows] = useState<Arrow[]>([]);
  const state = useRef({ decision, ui });
  state.current = { decision, ui };

  useEffect(() => {
    if (!active) {
      setArrows([]);
      return;
    }
    let hover: { card?: number; exp?: string } | null = null;
    const onMove = (e: PointerEvent) => {
      hover = null;
      const { decision: d } = state.current;
      for (const el of document.elementsFromPoint(e.clientX, e.clientY)) {
        const h = el as HTMLElement;
        if (d?.kind === 'select' && h.dataset.cardId && d.cardIds.includes(Number(h.dataset.cardId)) && !h.classList.contains('cast-card')) {
          hover = { card: Number(h.dataset.cardId) };
          break;
        }
        if (d?.kind === 'expedition' && h.dataset.expKey) {
          hover = { exp: h.dataset.expKey };
          break;
        }
      }
    };
    window.addEventListener('pointermove', onMove);
    let last = '';
    let frame = 0;
    const tick = () => {
      frame = requestAnimationFrame(tick);
      const { decision: d, ui: u } = state.current;
      const from = document.querySelector<HTMLElement>('.cast-card')?.getBoundingClientRect();
      if (!from || !d) return;
      // Sized on the card being cast, so that it follows the scale of the table
      const size = Math.max(16, from.width * 0.14);
      const targets: { card?: number; exp?: string; picked: boolean }[] =
        d.kind === 'select'
          ? u.picked.filter((id) => d.cardIds.includes(id)).map((card) => ({ card, picked: true }))
          : d.kind === 'expedition'
            ? u.keys.map((exp) => ({ exp, picked: true }))
            : [];
      if (hover && !targets.some((t) => t.card === hover!.card && t.exp === hover!.exp)) targets.push({ ...hover, picked: false });
      const next: Arrow[] = [];
      for (const t of targets) {
        const r = targetEl(t)?.getBoundingClientRect();
        if (!r || r.width < 2) continue;
        next.push({ key: `${t.card ?? t.exp}`, picked: t.picked, ...arrowPath(from, r, size) });
      }
      const sig = next.map((a) => a.key + a.d + a.picked).join('|');
      if (sig !== last) {
        last = sig;
        setArrows(next);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('pointermove', onMove);
    };
  }, [active]);

  if (!active || arrows.length === 0) return null;
  return (
    <svg className="target-arrows" aria-hidden="true">
      {arrows.map((a) => (
        <g key={a.key} className={a.picked ? 'picked' : 'preview'}>
          <path className="ta-glow" d={a.d} />
          <path className="ta-line" d={a.d} />
          <polygon className="ta-head" points={a.head} />
          {a.picked && <circle className="ta-ring" cx={a.x} cy={a.y} r={18} />}
        </g>
      ))}
    </svg>
  );
}
