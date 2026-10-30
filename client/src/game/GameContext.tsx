import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { CardCatalog, CardDef, CardView, Decision, Destination, GameView, LogEntry, PlayerId, PlayerView, Zone } from '@protocol';
import { affordances, autoAnswer, emptyUI, hasActions, togglePick, type Affordance, type UIState } from './decisions';
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
  /** The answer about to be sent for the player: the decision leaves no real choice */
  auto: ReturnType<typeof autoAnswer>;
  /** Resolve this choice (choiceKey) every time it is offered, until it is gone */
  repeatChoice: (key: string | null) => void;
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

  // Decisions without a real choice are answered for the player (once each: not again after an error)
  const [repeat, setRepeat] = useState<string | null>(null);
  const auto = useMemo(() => autoAnswer(decision, repeat), [decision, repeat]);
  const autoDone = useRef<string | null>(null);
  const [autoFailed, setAutoFailed] = useState<string | null>(null);
  // Shown as resolving until the answer is in (a late view may still carry the decision)
  const shownAuto = auto && decisionId !== autoFailed ? auto : null;
  useEffect(() => {
    if (!shownAuto || !decisionId || sending || autoDone.current === decisionId) return;
    const t = setTimeout(() => {
      autoDone.current = decisionId;
      void send(shownAuto.answer).then((ok) => ok || setAutoFailed(decisionId));
    }, shownAuto.delay);
    return () => clearTimeout(t);
  }, [shownAuto, decisionId, sending, send]);
  // The player answers the current decision: its repeat must not answer it again
  const repeatChoice = useCallback(
    (key: string | null) => {
      autoDone.current = decisionId;
      setRepeat(key);
    },
    [decisionId],
  );
  // The repeated choice ends with the first decision that does not offer it
  useEffect(() => {
    if (repeat !== null && decision && !auto) setRepeat(null);
  }, [repeat, decision, auto]);

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
          // An exhaust ability alone ({T}: Manaseed, Akesha...): used at once
          if (a?.tap && !a.support && !a.destinations?.length) void send({ type: 'tap', cardId: id });
          else setUI((u) => ({ ...u, selected: u.selected === id ? null : id }));
          return;
        }
        // A single target to pick: the click picks it (Pass or Confirm with none still decline)
        if (decision.kind === 'select' && a?.selectable && decision.max === 1 && decision.sourceCardId !== undefined && !decision.costs?.[id]) {
          void send({ type: 'select', cardIds: [id] });
          return;
        }
        if ((decision.kind === 'select' || decision.kind === 'cleanup' || decision.kind === 'exchange') && a?.selectable) {
          setUI((u) => togglePick(decision, u, id));
          return;
        }
      }
      inspectCard(id, 'modal');
    },
    [aff, decision, sending, setUI, inspectCard, send],
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
    auto: shownAuto,
    repeatChoice,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
