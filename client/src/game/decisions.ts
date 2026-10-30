/**
 * Pure decision logic: what the player may click for the current decision, and the Answer the
 * current UI state produces. Kept free of React (and of runtime imports) so that
 * scripts/check-decisions.ts can drive a whole game through it.
 */
import type {
  Answer,
  CardDef,
  ChoiceOption,
  CardView,
  CleanupDecision,
  Decision,
  Destination,
  ExchangeDecision,
  ExpeditionDecision,
  GameView,
  SelectDecision,
  TurnDecision,
  Zone,
} from '@protocol';

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
/** An Answer without its decisionId (added when it is sent) */
export type AnswerBody = DistributiveOmit<Answer, 'decisionId'>;

export interface UIState {
  decisionId: string | null;
  /** turn: the card the player tapped (its actions are offered) */
  selected: number | null;
  /** select / cleanup / exchange: picked cards */
  picked: number[];
  /** expedition: picked option keys */
  keys: string[];
}

export const emptyUI = (decisionId: string | null = null): UIState => ({ decisionId, selected: null, picked: [], keys: [] });

export interface Affordance {
  /** turn: destinations the card can be played to */
  destinations?: Destination[];
  support?: boolean;
  tap?: boolean;
  /** select / cleanup / exchange: the card can be picked */
  selectable?: boolean;
  picked?: boolean;
  extraCost?: number;
  /** the card at the origin of the decision */
  source?: boolean;
}

/** Zones whose cards are drawn on the table (a select over other zones uses the picker) */
const BOARD_ZONES: Zone[] = ['hero', 'hand', 'reserve', 'landmark', 'heroExpedition', 'companionExpedition', 'limbo'];

export function isOnBoard(card: CardView | undefined, you: number | null): boolean {
  if (!card || card.ref === null) return false;
  if (card.zone === 'hand' && card.owner !== you) return false;
  return BOARD_ZONES.includes(card.zone);
}

export function affordances(d: Decision | null, ui: UIState): Map<number, Affordance> {
  const m = new Map<number, Affordance>();
  const get = (id: number) => {
    let a = m.get(id);
    if (!a) m.set(id, (a = {}));
    return a;
  };
  if (!d) return m;
  if (d.sourceCardId !== undefined) get(d.sourceCardId).source = true;
  const picked = new Set(ui.picked);
  switch (d.kind) {
    case 'turn':
      for (const p of d.play) get(p.cardId).destinations = p.destinations;
      for (const id of d.support) get(id).support = true;
      for (const id of d.tap) get(id).tap = true;
      break;
    case 'select':
      for (const id of d.cardIds) {
        const a = get(id);
        a.selectable = true;
        a.picked = picked.has(id);
        if (d.costs?.[id]) a.extraCost = d.costs[id];
      }
      break;
    case 'cleanup':
      for (const id of [...d.reserve.cardIds, ...d.landmarks.cardIds]) {
        const a = get(id);
        a.selectable = true;
        a.picked = picked.has(id);
      }
      break;
    case 'exchange':
      for (const id of [...d.hand, ...d.reserve]) {
        const a = get(id);
        a.selectable = true;
        a.picked = picked.has(id);
      }
      break;
    case 'choice':
      for (const c of d.choices) if (c.sourceCardId !== undefined && c.enabled) get(c.sourceCardId).source = true;
      break;
  }
  return m;
}

export function hasActions(a: Affordance | undefined): boolean {
  return !!a && (!!a.destinations?.length || !!a.support || !!a.tap);
}

/** Toggles a card in the picked list, respecting the decision limits. */
export function togglePick(d: Decision, ui: UIState, id: number): UIState {
  const has = ui.picked.includes(id);
  if (has) return { ...ui, picked: ui.picked.filter((x) => x !== id) };
  switch (d.kind) {
    case 'select': {
      if (!d.cardIds.includes(id)) return ui;
      let picked = [...ui.picked, id];
      if (picked.length > d.max) picked = picked.slice(picked.length - d.max);
      return { ...ui, picked };
    }
    case 'cleanup': {
      const group = d.reserve.cardIds.includes(id) ? d.reserve : d.landmarks.cardIds.includes(id) ? d.landmarks : null;
      if (!group || group.n === 0) return ui;
      let inGroup = ui.picked.filter((x) => group.cardIds.includes(x));
      const others = ui.picked.filter((x) => !group.cardIds.includes(x));
      inGroup = [...inGroup, id];
      if (inGroup.length > group.n) inGroup = inGroup.slice(inGroup.length - group.n);
      return { ...ui, picked: [...others, ...inGroup] };
    }
    case 'exchange': {
      const group = d.hand.includes(id) ? d.hand : d.reserve.includes(id) ? d.reserve : null;
      if (!group) return ui;
      return { ...ui, picked: [...ui.picked.filter((x) => !group.includes(x)), id] };
    }
    default:
      return ui;
  }
}

export function toggleKey(d: ExpeditionDecision, ui: UIState, key: string): UIState {
  if (ui.keys.includes(key)) return { ...ui, keys: ui.keys.filter((k) => k !== key) };
  let keys = [...ui.keys, key];
  if (keys.length > d.max) keys = keys.slice(keys.length - d.max);
  return { ...ui, keys };
}

export function cleanupCounts(d: CleanupDecision, ui: UIState) {
  const reserve = ui.picked.filter((x) => d.reserve.cardIds.includes(x));
  const landmarks = ui.picked.filter((x) => d.landmarks.cardIds.includes(x));
  // The player must discard n cards of each group (or all of them if there are fewer)
  const needR = Math.min(d.reserve.n, d.reserve.cardIds.length);
  const needL = Math.min(d.landmarks.n, d.landmarks.cardIds.length);
  return { reserve, landmarks, needR, needL };
}

export function exchangePicks(d: ExchangeDecision, ui: UIState) {
  return {
    hand: ui.picked.find((x) => d.hand.includes(x)),
    reserve: ui.picked.find((x) => d.reserve.includes(x)),
  };
}

/** The answer the Confirm button sends, or null when the current picks are not valid. */
export function confirmAnswer(d: Decision, ui: UIState) {
  switch (d.kind) {
    case 'select':
      if (ui.picked.length < d.min || ui.picked.length > d.max) return null;
      return { type: 'select' as const, cardIds: ui.picked };
    case 'cleanup': {
      const c = cleanupCounts(d, ui);
      if (c.reserve.length !== c.needR || c.landmarks.length !== c.needL) return null;
      return { type: 'cleanup' as const, reserve: c.reserve, landmarks: c.landmarks };
    }
    case 'exchange': {
      const p = exchangePicks(d, ui);
      if (p.hand === undefined || p.reserve === undefined) return null;
      return { type: 'exchange' as const, hand: p.hand, reserve: p.reserve };
    }
    case 'expedition':
      if (ui.keys.length < d.min || ui.keys.length > d.max) return null;
      return { type: 'expedition' as const, keys: ui.keys };
    case 'confirm':
      return { type: 'confirm' as const };
    default:
      return null;
  }
}

/**
 * The main action of a decision, shown on its own button (the round button of the arena track):
 * end the turn, confirm the picks, or pass an optional step.
 */
export type PrimaryAction = { kind: 'endTurn' } | { kind: 'confirm'; label: string } | { kind: 'pass'; label: string };

export function primaryAction(d: Decision | null): PrimaryAction | null {
  if (!d) return null;
  switch (d.kind) {
    case 'turn':
      return d.canEndTurn ? { kind: 'endTurn' } : d.canPass ? { kind: 'pass', label: d.passLabel ?? 'Pass' } : null;
    case 'select':
    case 'confirm':
      return { kind: 'confirm', label: 'Confirm' };
    case 'cleanup':
      return { kind: 'confirm', label: 'Discard' };
    case 'exchange':
      return { kind: 'confirm', label: 'Exchange' };
    case 'expedition':
      if (!(d.min === 1 && d.max === 1)) return { kind: 'confirm', label: 'Confirm' };
      break;
    case 'deck':
      return null;
  }
  return d.canPass ? { kind: 'pass', label: d.passLabel ?? 'Pass' } : null;
}

/** Cards of a select decision that are not drawn on the table: they are shown in a picker. */
export function pickerCardIds(view: GameView, d: SelectDecision): number[] {
  if (d.showInPicker) return d.cardIds;
  const byId = new Map(view.cards.map((c) => [c.id, c]));
  const off = d.cardIds.filter((id) => !isOnBoard(byId.get(id), view.you));
  // When some targets are off the table, show them all in the picker so the choice is in one place
  return off.length ? d.cardIds : [];
}

export const DESTINATION_LABELS: Record<Destination, string> = {
  heroExpedition: 'Hero expedition',
  companionExpedition: 'Companion expedition',
  landmark: 'Landmarks',
  reserve: 'Reserve',
  spell: 'Cast',
  heroExpeditionScout: 'Scout to hero expedition',
  companionExpeditionScout: 'Scout to companion expedition',
};

/** Mana a play costs: PlayOption.cost when the server gives it, else the card cost for its zone. */
export function playCost(d: TurnDecision, card: CardView | undefined, def: CardDef | null | undefined, dest: Destination): number | undefined {
  const opt = d.play.find((p) => p.cardId === card?.id);
  if (opt?.cost !== undefined) return opt.cost;
  if (!card) return undefined;
  if (dest === 'heroExpeditionScout' || dest === 'companionExpeditionScout') {
    const m = def ? /SCOUT_(\d+)/.exec(def.effect) : null;
    return m ? Number(m[1]) : undefined;
  }
  return card.zone === 'reserve' ? card.costReserve : card.costHand;
}

/** The id of the Pass option of a choice (Engine PASS) */
export const PASS_CHOICE = 99;

/** Options of a choice that do the same thing (The Hunger's boosts...): same effect, same source */
export const choiceKey = (c: ChoiceOption) => `${c.sourceCardId ?? ''}|${c.label}`;

/** How long a target picked for the player is shown (focus and arrow) before it is sent */
export const AUTO_TARGET_MS = 600;

/**
 * The answer sent for the player when a decision leaves no real choice: one effect to resolve
 * (maybe several times), a "you may" effect that can still be passed after it is chosen, the only
 * possible targets of a mandatory effect, nothing to pick... `repeat`: the choice key the player
 * asked to resolve every time it comes back ("all" on a group of identical effects).
 */
export function autoAnswer(d: Decision | null, repeat: string | null = null): { answer: AnswerBody; delay: number } | null {
  if (!d) return null;
  const now = (answer: AnswerBody) => ({ answer, delay: 0 });
  const shown = (answer: AnswerBody) => ({ answer, delay: AUTO_TARGET_MS });
  switch (d.kind) {
    case 'choice': {
      const on = d.choices.filter((c) => c.enabled);
      const pass = on.find((c) => c.id === PASS_CHOICE);
      const effects = on.filter((c) => c.id !== PASS_CHOICE);
      const again = repeat !== null && effects.find((c) => choiceKey(c) === repeat);
      if (again) return now({ type: 'choice', id: again.id });
      // Every effect is impossible: only Pass is left
      if (!effects.length) return pass ? now({ type: 'choice', id: pass.id }) : null;
      // One effect, maybe several times: the order does not matter
      if (!pass && !d.canPass && effects.every((c) => choiceKey(c) === choiceKey(effects[0]))) return now({ type: 'choice', id: effects[0].id });
      // "You may": the effect is chosen, the player passes its next step if they wish
      if (pass && effects.length === 1 && effects[0].declinable) return now({ type: 'choice', id: effects[0].id });
      return null;
    }
    case 'select':
      if (!d.cardIds.length) return d.min === 0 ? now({ type: 'select', cardIds: [] }) : d.canPass ? now({ type: 'pass' }) : null;
      // Mandatory, and every candidate must be picked (the only valid target...); not when it costs more
      if (d.canPass || d.min === 0 || d.cardIds.length > d.min || d.cardIds.some((id) => d.costs?.[id])) return null;
      return shown({ type: 'select', cardIds: d.cardIds });
    case 'expedition':
      if (d.canPass || d.min === 0 || d.expeditions.length > d.min) return null;
      return shown({ type: 'expedition', keys: d.expeditions.map((o) => o.key) });
    case 'player':
      return !d.canPass && d.players.length === 1 ? now({ type: 'player', player: d.players[0] }) : null;
    case 'location':
      return !d.canPass && d.locations.length === 1 ? now({ type: 'location', key: d.locations[0].key }) : null;
    case 'number':
      return !d.canPass && d.values.length === 1 ? now({ type: 'number', value: d.values[0] }) : null;
    case 'exchange':
      return !d.canPass && d.hand.length === 1 && d.reserve.length === 1 ? now({ type: 'exchange', hand: d.hand[0], reserve: d.reserve[0] }) : null;
    case 'cleanup': {
      const r = d.reserve.cardIds.length <= d.reserve.n;
      const l = d.landmarks.cardIds.length <= d.landmarks.n;
      if (d.canPass || !r || !l || d.reserve.n + d.landmarks.n === 0) return null;
      return now({ type: 'cleanup', reserve: d.reserve.cardIds, landmarks: d.landmarks.cardIds });
    }
    default:
      return null;
  }
}
