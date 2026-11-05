import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { createStore, useStore, type StoreApi } from 'zustand';
import { useShallow } from 'zustand/react/shallow';
import type { CardCatalog, CardDef, CardView, Decision, Destination, GameView, LogEntry, PlayerId, PlayerView, Zone } from '@protocol';
import { affordances, autoAnswer, emptyUI, hasActions, togglePick, type Affordance, type ResolveIntent, type UIState } from './decisions';
import type { AnswerBody } from './useGame';
import { useInspect } from './inspect';
import { cardDef } from '../api/catalog';
import { confirmTurnPref } from '../screens/storage';
import { shareMap, shareView } from './share';

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
  /** Cards of my hand sent to the Mana zone that are still in hand (the engine waits for the other player) */
  manaPending: Set<number>;
  /** The card whose effect is pointed at in the effects queue: it is lit on the board */
  focusSource: number | null;
  setFocusSource: (id: number | null) => void;
  /** An effect picked in the queue whose last step (its "you may") is answered for the player */
  setResolveIntent: (intent: ResolveIntent | null) => void;
}

/**
 * The game state lives in a store (zustand), not in a React context value: a component subscribes
 * to the parts it reads and is drawn again only when one of them changes. The provider computes
 * the state from the view and writes it to the store; unchanged cards, zones and affordances keep
 * their identity from one view to the next (structural sharing), so a new view redraws only the
 * cards that changed.
 */
const StoreCtx = createContext<StoreApi<GameCtxValue> | null>(null);

/** The game store itself: event handlers read it with getState(), without subscribing */
export function useGameStore(): StoreApi<GameCtxValue> {
  const s = useContext(StoreCtx);
  if (!s) throw new Error('useGameCtx outside GameProvider');
  return s;
}

/** One value derived from the game state: the component is drawn again only when it changes (Object.is) */
export function useGame<T>(select: (s: GameCtxValue) => T): T {
  return useStore(useGameStore(), select);
}

/** Some fields of the game state: the component is drawn again only when one of them changes */
export function useGameCtx<K extends keyof GameCtxValue>(...keys: K[]): Pick<GameCtxValue, K> {
  return useStore(
    useGameStore(),
    useShallow((s) => {
      const o = {} as Pick<GameCtxValue, K>;
      for (const k of keys) o[k] = s[k];
      return o;
    }),
  );
}

const NO_CARDS: CardView[] = [];
/** One card of the view (the same object while it does not change) */
export const useCard = (id: number | null | undefined) => useGame((s) => (id == null ? undefined : s.byId.get(id)));
/** What the current decision offers for this card */
export const useAff = (id: number) => useGame((s) => s.aff.get(id));
/** The cards of a zone, in order (the same array while they do not change) */
export const useZone = (owner: PlayerId | undefined, zone: Zone) => useGame((s) => (owner === undefined ? NO_CARDS : s.zoneCards(owner, zone)));

/** A function whose identity never changes and which always calls the latest `fn` */
function useStableFn<A extends unknown[], R>(fn: (...a: A) => R): (...a: A) => R {
  const ref = useRef(fn);
  useLayoutEffect(() => {
    ref.current = fn;
  });
  return useCallback((...a: A) => ref.current(...a), []);
}

/** `value`, or the previous one where they are deep-equal (see share.ts) */
function useShared<T>(value: T, by: (prev: T | null, next: T) => T): T {
  const prev = useRef<T | null>(null);
  const out = by(prev.current, value);
  prev.current = out;
  return out;
}

export function GameProvider({
  view: viewIn,
  catalog,
  log,
  sending,
  send: sendRaw,
  undoneTo = null,
  readOnly = false,
  children,
}: {
  view: GameView;
  catalog: CardCatalog;
  log: LogEntry[];
  sending: boolean;
  send: (a: AnswerBody) => Promise<boolean>;
  /** The decision an undo led back to: the player answers it (it may be one answered for them) */
  undoneTo?: { id: string; n: number } | null;
  readOnly?: boolean;
  children: ReactNode;
}) {
  const view = useShared(viewIn, shareView);
  const decision = readOnly ? null : view.decision;
  const [ui, setUIState] = useState<UIState>(() => emptyUI(decision?.id ?? null));
  const inspect = useInspect();

  // A new decision resets the interaction state
  const decisionId = decision?.id ?? null;
  const current = ui.decisionId === decisionId ? ui : emptyUI(decisionId);
  useEffect(() => {
    if (ui.decisionId !== decisionId) setUIState(emptyUI(decisionId));
  }, [decisionId, ui.decisionId]);
  const setUI = useStableFn((f: (u: UIState) => UIState) => setUIState((u) => f(u.decisionId === decisionId ? u : emptyUI(decisionId))));

  // Cards move when a new view arrives: a hover preview would point at a card that left
  const { hide } = inspect;
  useEffect(() => hide(true), [view.seq, hide]);

  const byId = useMemo(() => new Map(view.cards.map((c) => [c.id, c])), [view.cards]);
  const zonesNow = useMemo(() => {
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
  const zones = useShared(zonesNow, shareMap);
  const zoneCards = useCallback((owner: PlayerId, zone: Zone) => zones.get(`${owner}:${zone}`) ?? NO_CARDS, [zones]);
  const def = useCallback((c: CardView | undefined | null) => cardDef(catalog, c), [catalog]);
  const aff = useShared(
    useMemo(() => affordances(decision, current), [decision, current]),
    shareMap,
  );

  // Cards sent to the Mana zone stay face down until they get there: at the setup, the engine moves
  // them only once the other player chose theirs. A new decision of mine ends it.
  const [focusSource, setFocusSource] = useState<number | null>(null);
  const [toMana, setToMana] = useState<{ decisionId: string; cardIds: number[] } | null>(null);
  const send = useStableFn(
    async (a: AnswerBody) => {
      const sent = a.type === 'select' && decision?.kind === 'select' && decision.toMana ? { decisionId: decision.id, cardIds: a.cardIds } : null;
      if (sent) setToMana(sent);
      const ok = await sendRaw(a);
      if (!ok && sent) setToMana(null);
      return ok;
    },
  );
  const manaPending = useMemo(
    () =>
      new Set(
        toMana && (!decision || decision.id === toMana.decisionId) ? toMana.cardIds.filter((id) => byId.get(id)?.zone === 'hand') : [],
      ),
    [toMana, decision, byId],
  );

  // Decisions without a real choice are answered for the player (once each: not again after an error)
  const [repeat, setRepeat] = useState<string | null>(null);
  const [resolveIntent, setResolveIntent] = useState<ResolveIntent | null>(null);
  const auto = useMemo(
    () => autoAnswer(decision, repeat, confirmTurnPref(), resolveIntent, (id) => byId.get(id)?.ref),
    [decision, repeat, resolveIntent, byId],
  );
  // The intent lasts until a decision that is not one of its steps (another choice of the same card)
  useEffect(() => {
    if (!resolveIntent || !decision) return;
    const ofIt = decision.kind === 'choice' && decision.choices.some((c) => c.sourceCardId === resolveIntent.source);
    if (!ofIt) setResolveIntent(null);
  }, [decision, resolveIntent]);
  const autoDone = useRef<string | null>(null);
  const [autoFailed, setAutoFailed] = useState<string | null>(null);
  // Shown as resolving until the answer is in (a late view may still carry the decision)
  const shownAuto = auto && decisionId !== autoFailed ? auto : null;
  useEffect(() => {
    if (!shownAuto || !decisionId || sending || autoDone.current === decisionId) return;
    const t = setTimeout(() => {
      // An undo may have led back here since the timer started
      if (autoDone.current === decisionId) return;
      autoDone.current = decisionId;
      // Tagged: the server's undo goes back past it, to the player's last choice
      void send({ ...shownAuto.answer, auto: true }).then((ok) => ok || setAutoFailed(decisionId));
    }, shownAuto.delay);
    return () => clearTimeout(t);
  }, [shownAuto, decisionId, sending, send]);
  // Back by an undo: the decision there is not answered for the player (else the undo is played
  // again at once), nor the effects they asked to resolve before
  useEffect(() => {
    if (!undoneTo) return;
    autoDone.current = undoneTo.id;
    setRepeat(null);
    setResolveIntent(null);
  }, [undoneTo]);
  // The player answers the current decision: its repeat must not answer it again
  const repeatChoice = useStableFn((key: string | null) => {
    autoDone.current = decisionId;
    setRepeat(key);
  });
  // The repeated choice ends with the first decision that does not offer it
  useEffect(() => {
    if (repeat !== null && decision && !auto) setRepeat(null);
  }, [repeat, decision, auto]);

  const you = view.you;
  const setFocus = useStableFn(setFocusSource);
  const setIntent = useStableFn(setResolveIntent);
  const me = view.players.find((p) => p.id === you) ?? (you === null ? view.players[0] : undefined);
  const opp = view.players.find((p) => p.id !== me?.id);

  const inspectCard = useStableFn((id: number, mode: 'hover' | 'modal' = 'modal', x?: number) => {
    const c = byId.get(id);
    const d = def(c);
    if (d) inspect.show({ def: d, card: c, mode, x });
  });

  const clickCard = useStableFn((id: number) => {
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
  });

  const play = useStableFn((cardId: number, destination: Destination) => {
    setUI((u) => ({ ...u, selected: null }));
    return send({ type: 'play', cardId, destination });
  });

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
    manaPending,
    focusSource,
    setFocusSource: setFocus,
    setResolveIntent: setIntent,
  };
  const [store] = useState(() => createStore<GameCtxValue>(() => value));
  // Before paint: the components that read a changed part are drawn again in the same frame
  useLayoutEffect(() => store.setState(value, true));
  return <StoreCtx.Provider value={store}>{children}</StoreCtx.Provider>;
}
