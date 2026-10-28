import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import type { CardCatalog, CardDef, CardView } from '@protocol';

export interface InspectTarget {
  def: CardDef;
  card?: CardView;
  /** hover = transient preview (desktop), modal = explicit (tap, long press, click) */
  mode: 'hover' | 'modal';
  /** screen x of the pointer, to show the preview on the other side */
  x?: number;
}

interface InspectCtx {
  target: InspectTarget | null;
  show: (t: InspectTarget) => void;
  hide: (onlyHover?: boolean) => void;
  catalog: CardCatalog | null;
  /** Finds a card definition by its displayed name (log entries and prompts name cards) */
  resolveName: (name: string) => CardDef | null;
}

const Ctx = createContext<InspectCtx>({
  target: null,
  show: () => {},
  hide: () => {},
  catalog: null,
  resolveName: () => null,
});

export function InspectProvider({
  catalog,
  preferredRefs,
  children,
}: {
  catalog: CardCatalog | null;
  /** refs to prefer when several cards share a name (cards of the current game) */
  preferredRefs?: string[];
  children: ReactNode;
}) {
  const [target, setTarget] = useState<InspectTarget | null>(null);
  const byName = useMemo(() => {
    const m = new Map<string, CardDef[]>();
    if (catalog)
      for (const d of Object.values(catalog)) {
        const k = d.name.toLowerCase();
        const l = m.get(k);
        if (l) l.push(d);
        else m.set(k, [d]);
      }
    return m;
  }, [catalog]);
  const preferred = useMemo(() => new Set(preferredRefs ?? []), [preferredRefs]);
  const resolveName = useCallback(
    (name: string) => {
      const l = byName.get(name.toLowerCase());
      if (!l) return null;
      return l.find((d) => preferred.has(d.ref)) ?? l.find((d) => d.rarity === 'common') ?? l[0];
    },
    [byName, preferred],
  );
  const show = useCallback((t: InspectTarget) => {
    setTarget((cur) => (cur?.mode === 'modal' && t.mode === 'hover' ? cur : t));
  }, []);
  const hide = useCallback((onlyHover?: boolean) => {
    setTarget((cur) => (onlyHover && cur?.mode === 'modal' ? cur : null));
  }, []);
  const value = useMemo(() => ({ target, show, hide, catalog, resolveName }), [target, show, hide, catalog, resolveName]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export const useInspect = () => useContext(Ctx);
