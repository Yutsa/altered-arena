import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import type { CardCatalog, CardDef, CardView, Decision, Destination, GameView, LogEntry, PlayerId, PlayerView, Zone } from '@protocol';
import { affordances, emptyUI, hasActions, togglePick, type Affordance, type UIState } from './decisions';
import type { AnswerBody } from './useGame';
import { useInspect } from './inspect';
import { cardDef } from '../api/catalog';

export interface GameCtxValue {
  view: GameView;
  catalog: CardCatalog;
  log: LogEntry[];
  you: PlayerId | null;
  me: PlayerView | undefined;
  opp: PlayerView | undefined;
  byId: Map<number, CardView>;
  zoneCards: (owner: PlayerId, zone: Zone) => CardView[];
  def: (c: CardView | undefined | null) => CardDef | null;
  decision: Decision | null;
  sending: boolean;
  /** read-only (replays): clicks only inspect */
  readOnly: boolean;
  send: (a: AnswerBody) => Promise<boolean>;
  ui: UIState;
  setUI: (f: (u: UIState) => UIState) => void;
  aff: Map<number, Affordance>;
  clickCard: (id: number) => void;
  inspectCard: (id: number, mode?: 'hover' | 'modal', x?: number) => void;
  play: (cardId: number, destination: Destination) => Promise<boolean>;
}

const Ctx = createContext<GameCtxValue | null>(null);

export function useGameCtx(): GameCtxValue {
  const v = useContext(Ctx);
  if (!v) throw new Error('useGameCtx outside GameProvider');
  return v;
}

export function GameProvider({
  view,
  catalog,
  log,
  sending,
  send,
  readOnly = false,
  children,
}: {
  view: GameView;
  catalog: CardCatalog;
  log: LogEntry[];
  sending: boolean;
  send: (a: AnswerBody) => Promise<boolean>;
  readOnly?: boolean;
  children: ReactNode;
}) {
  const decision = readOnly ? null : view.decision;
  const [ui, setUIState] = useState<UIState>(() => emptyUI(decision?.id ?? null));
  const inspect = useInspect();

  // A new decision resets the interaction state
  const decisionId = decision?.id ?? null;
  const current = ui.decisionId === decisionId ? ui : emptyUI(decisionId);
  useEffect(() => {
    if (ui.decisionId !== decisionId) setUIState(emptyUI(decisionId));
  }, [decisionId, ui.decisionId]);
  const setUI = useCallback(
    (f: (u: UIState) => UIState) => setUIState((u) => f(u.decisionId === decisionId ? u : emptyUI(decisionId))),
    [decisionId],
  );

  // Cards move when a new view arrives: a hover preview would point at a card that left
  const { hide } = inspect;
  useEffect(() => hide(true), [view.seq, hide]);

  const byId = useMemo(() => new Map(view.cards.map((c) => [c.id, c])), [view.cards]);
  const zones = useMemo(() => {
    const m = new Map<string, CardView[]>();
    for (const c of view.cards) {
      const owner = c.controller ?? c.owner;
      const k = `${owner}:${c.zone}`;
      const l = m.get(k);
      if (l) l.push(c);
      else m.set(k, [c]);
    }
    for (const l of m.values()) l.sort((a, b) => a.position - b.position || a.id - b.id);
    return m;
  }, [view.cards]);
  const zoneCards = useCallback((owner: PlayerId, zone: Zone) => zones.get(`${owner}:${zone}`) ?? [], [zones]);
  const def = useCallback((c: CardView | undefined | null) => cardDef(catalog, c), [catalog]);
  const aff = useMemo(() => affordances(decision, current), [decision, current]);

  const you = view.you;
  const me = view.players.find((p) => p.id === you) ?? (you === null ? view.players[0] : undefined);
  const opp = view.players.find((p) => p.id !== me?.id);

  const inspectCard = useCallback(
    (id: number, mode: 'hover' | 'modal' = 'modal', x?: number) => {
      const c = byId.get(id);
      const d = def(c);
      if (d) inspect.show({ def: d, card: c, mode, x });
    },
    [byId, def, inspect],
  );

  const clickCard = useCallback(
    (id: number) => {
      const a = aff.get(id);
      if (decision && !sending) {
        if (decision.kind === 'turn' && hasActions(a)) {
          setUI((u) => ({ ...u, selected: u.selected === id ? null : id }));
          return;
        }
        if ((decision.kind === 'select' || decision.kind === 'cleanup' || decision.kind === 'exchange') && a?.selectable) {
          setUI((u) => togglePick(decision, u, id));
          return;
        }
      }
      inspectCard(id, 'modal');
    },
    [aff, decision, sending, setUI, inspectCard],
  );

  const play = useCallback(
    (cardId: number, destination: Destination) => {
      setUI((u) => ({ ...u, selected: null }));
      return send({ type: 'play', cardId, destination });
    },
    [send, setUI],
  );

  const value: GameCtxValue = {
    view,
    catalog,
    log,
    you,
    me,
    opp,
    byId,
    zoneCards,
    def,
    decision,
    sending,
    readOnly,
    send,
    ui: current,
    setUI,
    aff,
    clickCard,
    inspectCard,
    play,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
