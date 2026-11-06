/**
 * Altered Arena client/server protocol, version 1.
 *
 * The server is authoritative. A client fetches a GameView (everything it may see, plus the
 * decision it must take, if any) and answers decisions with an Answer. Both game servers produce
 * this protocol: the Rust server (production) and the PHP oracle (the original BGA engine, used to
 * check that the new engine plays by the same rules).
 *
 * HTTP API (JSON):
 *   GET  /api/v1/cards                       -> CardCatalog (static data of every card)
 *   GET  /api/v1/decks                       -> DeckSummary[] (starter decks)
 *   GET  /api/v1/reunion-decks/{id}          -> ReunionDeckSummary (an Altered Reunion deck, checked by the engine)
 *   POST /api/v1/games       NewGame         -> GameCreated
 *   GET  /api/v1/games/{id}?player=P&token=T[&since=seq] -> GameView (with since: waits up to 20 s for a newer seq)
 *   POST /api/v1/games/{id}/answer?player=P&token=T  Answer -> GameView | ErrorResponse
 *   POST /api/v1/games/{id}/undo?player=P&token=T    -> GameView | ErrorResponse (takes back the
 *        player's last answer, games against bots only: the same draws follow the same answers)
 *   POST /api/v1/matchmaking  {name, deck, reunionDeck} -> MatchTicket (status 'waiting' or 'matched')
 *   GET  /api/v1/matchmaking/{ticket}         -> MatchTicket (long poll, up to 20 s)
 *   DELETE /api/v1/matchmaking/{ticket}       leave the queue
 *   POST /api/v1/rooms        NewRoom         -> RoomCreated (a private room: a game against a friend)
 *   GET  /api/v1/rooms/{id}?player=P[&since=seq] -> RoomView (opening it joins the seat; with since:
 *        waits up to 20 s for a newer seq)
 *   POST /api/v1/rooms/{id}/seat?player=P  SeatUpdate -> RoomView (the game starts, under the room's
 *        id and with the room's tokens, once both players are ready)
  * Authentication: each human seat gets a secret `token` when the game is created (GameCreated)
 * or matched (MatchTicket). Send it as `Authorization: Bearer <token>` (`token=` query parameter
 * accepted for tools; keep secrets out of URLs). Without a
 * token, a GameView is a spectator view (no hidden cards, no decision) and answers are refused.
 * Card art: /assets/cards/{art} (`art` from the catalog; absent when there is no art file).
 */

export type PlayerId = number;
export type Biome = 'forest' | 'mountain' | 'ocean';
export type Side = 'hero' | 'companion';

/** Where a card is. Expeditions are the two lines of characters a player sends into the Tumult. */
export type Zone =
  | 'hero' // the player's Hero card
  | 'hand'
  | 'deck'
  | 'reserve'
  | 'landmark'
  | 'heroExpedition'
  | 'companionExpedition'
  | 'discard'
  | 'mana'
  | 'limbo'; // a spell being cast, or a card being moved

export type Phase = 'setup' | 'morning' | 'noon' | 'afternoon' | 'dusk' | 'night' | 'tiebreaker' | 'ended';

// ---------------------------------------------------------------------------
// Static card data
// ---------------------------------------------------------------------------

export type CardType = 'hero' | 'character' | 'spell' | 'permanent' | 'token';
export type Faction = 'AX' | 'BR' | 'LY' | 'MU' | 'OD' | 'YZ' | 'NE'; // OD = Ordis

export interface CardDef {
  ref: string; // engine class name, e.g. "AX_Common_AbySapCourier"
  name: string;
  type: CardType;
  typeline: string; // "Character - Engineer Messenger"
  subtypes: string[];
  faction: Faction;
  rarity: 'common' | 'rare' | 'unique' | 'exalted';
  asset: string; // art file name, without extension
  /** Path of the art under /assets/cards/ (its folder is not always the card's faction) */
  art?: string;
  costHand?: number;
  costReserve?: number;
  forest?: number;
  mountain?: number;
  ocean?: number;
  /** Rules text. Markup: {J} played, {R} reserve, {H} hand, {T} exhaust, {D} discard, {1} mana cost,
   * <KEYWORD>, $<KEYWORD> (with reminder), $[KEYWORD], <KEYWORD:arg> keywords; #...# delimits text
   * specific to the rare version. Clients render it as they like. */
  effect: string;
  support?: string; // text of the support ability (in Reserve)
  flavor?: string;
  artist?: string;
  keywords: string[]; // 'fleeting', 'anchored', 'asleep', 'gigantic', 'tough', 'defender', 'eternal', 'seasoned', 'scout'...
  reserveSlots?: number; // heroes
  landmarkSlots?: number; // heroes
}

export type CardCatalog = Record<string, CardDef>;

export interface DeckSummary {
  id: string; // e.g. "ax_1_sierra"
  name: string; // hero name
  faction: Faction;
  heroRef: string;
  size: number;
}

/** An Altered Reunion deck (decks-api id), as the server reads it and the engine checks it */
export interface ReunionDeckSummary {
  id: string;
  name: string;
  faction: Faction | null;
  /** Catalog key of the hero (null when the engine does not know it) */
  heroRef: string | null;
  size: number;
  /** decks-api's verdict */
  legal: boolean;
  /** The engine plays it (a game can be created with it) */
  playable: boolean;
  /** Why the engine refuses it */
  problem?: string;
}

// ---------------------------------------------------------------------------
// Game state as seen by one player
// ---------------------------------------------------------------------------

export interface CardView {
  id: number;
  ref: string | null; // null when face down (opponent's mana...)
  owner: PlayerId;
  /** Player who controls it, when not the owner */
  controller?: PlayerId;
  zone: Zone;
  /** Order in its zone (left to right) */
  position: number;
  exhausted: boolean;
  /** Current statistics, boosts included (characters) */
  stats?: Record<Biome, number>;
  /** Current costs, reductions included (cards that can be played) */
  costHand?: number;
  costReserve?: number;
  boosts: number;
  /** Other counters on the card, by name */
  counters: Record<string, number>;
  /** Status keywords currently applying: fleeting, anchored, asleep, tough, defender, gigantic... */
  statuses: string[];
  /** Terrain markers lying on the card (Nadir's, not placed yet) */
  markers?: Biome[];
  /** Token cards are created by effects and disappear when they leave play */
  token?: boolean;
  /** Unique cards only: their printed data. Uniques are not in the catalog (their ref is "Card");
   * effect and support are their generated texts, ref is their unique id (e.g. "ALT_EOLE_B_AX_107_U_1908"). */
  unique?: CardDef;
}

export interface ExpeditionView {
  /** Region index (0..7) on the Tumult track: hero expeditions start at 0 and go up, companion ones start at 7 and go down */
  position: number;
  stats: Record<Biome, number>;
  blocked: boolean;
  /** Ascended (Soar...): until Rest, it moves forward even when the opposing expedition matches it */
  ascended?: boolean;
  /** At Dusk, as things stand, it moves forward: it wins a biome of its region */
  advances?: boolean;
}

export interface PlayerView {
  id: PlayerId;
  name: string;
  bot: boolean;
  faction: Faction | null;
  heroCardId: number | null;
  handCount: number;
  deckCount: number;
  discardCount: number;
  manaTotal: number;
  manaAvailable: number;
  passed: boolean;
  reserveSlots: number;
  landmarkSlots: number;
  expeditions: Record<Side, ExpeditionView>;
}

export interface RegionView {
  index: number; // 0..7
  /** Biomes of the region; null while the Tumult card is face down */
  biomes: Biome[] | null;
  kind: 'heroRegion' | 'tumult' | 'companionRegion';
  /** A terrain marker (Nadir): the region has this biome only, while the marker stays */
  marker?: { biome: Biome; owner: PlayerId };
}

export interface LogEntry {
  seq: number;
  text: string; // plain text with card names in [[double brackets]]
  player?: PlayerId;
}

/** A card revealed from the top of a deck ("Reveal the top card of your deck..."), seen by both
 * players, and where it is now: still on top of the deck (face up), drawn, in Reserve... */
export interface RevealView {
  seq: number; // the log entry of the reveal
  player: PlayerId; // whose deck
  cardId: number;
  ref: string | null;
  unique?: CardDef;
  to: 'deck' | 'hand' | 'reserve' | 'discard' | 'board';
}

/** A spell cast (not sent by the PHP oracle): its card, the cards its caster targeted for it, and
 * whether it has left limbo. Lets a client show the cast, then its targets, before its result. */
export interface CastView {
  seq: number; // the log entry of the cast
  player: PlayerId;
  cardId: number;
  ref: string | null;
  unique?: CardDef;
  targets: number[];
  resolved: boolean;
}

/** The effect of a card that is not a spell being cast (a character's, a permanent's, a hero's, a
 * reaction...), not sent by the PHP oracle: its source card and the cards it targeted. Lets a
 * client show the card whose effect it is before its result. */
export interface EffectView {
  seq: number; // the first log entry of the effect
  player: PlayerId; // the source card's controller
  cardId: number;
  ref: string | null;
  unique?: CardDef;
  targets: number[];
}

export interface GameView {
  gameId: string;
  you: PlayerId | null; // null for a spectator
  /** Increases with every change of the game */
  seq: number;
  day: number;
  phase: Phase;
  firstPlayer: PlayerId | null;
  activePlayer: PlayerId | null;
  winner: PlayerId | null;
  players: PlayerView[]; // seat order
  /** Every card the viewer may see (opponent's hand and decks are only counts) */
  cards: CardView[];
  regions: RegionView[];
  decision: Decision | null; // what `you` must decide now
  waitingFor: PlayerId[]; // players who must decide
  log: LogEntry[]; // most recent entries (up to 40): clients merge them into their own history
  /** `you` may take back their last answer (a game against bots, not sent by the PHP oracle) */
  canUndo?: boolean;
  /** The latest cards revealed from the top of a deck (not sent by the PHP oracle) */
  reveals?: RevealView[];
  /** The latest spells cast (not sent by the PHP oracle) */
  casts?: CastView[];
  /** The latest effects of other cards (not sent by the PHP oracle) */
  effects?: EffectView[];
}

// ---------------------------------------------------------------------------
// Decisions and answers
// ---------------------------------------------------------------------------

export type Destination =
  | 'heroExpedition'
  | 'companionExpedition'
  | 'landmark'
  | 'reserve'
  | 'spell' // cast a spell
  | 'heroExpeditionScout'
  | 'companionExpeditionScout';

export interface PlayOption {
  cardId: number;
  destinations: Destination[];
  /** Mana it costs (for display) */
  cost?: number;
}

export interface ChoiceOption {
  id: number;
  label: string;
  enabled: boolean;
  sourceCardId?: number;
  /** A "you may" effect (offered with Pass): once chosen, the decision it leads to can still be passed */
  declinable?: boolean;
  /**
   * The choice only leads to more steps of the same card (a check that picks a branch...) and then to
   * this effect, offered with Pass: the label of that effect. GameView only.
   */
  resolvesTo?: string;
}

export interface ExpeditionOption {
  key: string; // opaque, send back as is
  player: PlayerId;
  side: Side;
}

export interface LocationOption {
  key: string; // opaque
  label: string;
  player?: PlayerId;
  zone?: Zone;
}

interface DecisionBase {
  /** Send it back in the answer: answers to a stale decision are refused */
  id: string;
  prompt: string;
  sourceCardId?: number;
  /** An optional step: the player may skip it */
  canPass?: boolean;
  passLabel?: string;
}

/** The Afternoon action: play a card (drag it to a destination), use a support or exhaust ability, or pass */
export interface TurnDecision extends DecisionBase {
  kind: 'turn';
  play: PlayOption[];
  support: number[]; // reserve cards whose support ability can be used
  tap: number[]; // cards whose {T} ability can be used
  canEndTurn: boolean; // "pass" for the rest of the Afternoon
}

/** Pick cards (mana at setup, targets, discards...) */
export interface SelectDecision extends DecisionBase {
  kind: 'select';
  cardIds: number[];
  min: number;
  max: number;
  /** Optional per-card extra cost shown on the card (e.g. Tough) */
  costs?: Record<number, number>;
  /** Cards not on the board (mana, deck, discard...) are shown in a picker */
  showInPicker?: boolean;
  /** The picked cards go to the Mana zone (face down) */
  toMana?: boolean;
}

export interface ChoiceDecision extends DecisionBase {
  kind: 'choice';
  choices: ChoiceOption[];
}

export interface ConfirmDecision extends DecisionBase {
  kind: 'confirm'; // end of turn: confirm
}

export interface ExpeditionDecision extends DecisionBase {
  kind: 'expedition';
  expeditions: ExpeditionOption[];
  min: number;
  max: number;
}

export interface PlayerDecision extends DecisionBase {
  kind: 'player';
  players: PlayerId[];
}

export interface NumberDecision extends DecisionBase {
  kind: 'number';
  values: number[];
  // Die result choice of All In!: Reserve cards that may be discarded to add 2 to the results
  // (answer { type: 'select', cardIds: [id] } instead of a number)
  discardCardIds?: number[];
}

export interface LocationDecision extends DecisionBase {
  kind: 'location';
  locations: LocationOption[];
}

/** Swap a hand card with a reserve card */
export interface ExchangeDecision extends DecisionBase {
  kind: 'exchange';
  hand: number[];
  reserve: number[];
}

/** Night: discard cards in excess of the Reserve and Landmark limits */
export interface CleanupDecision extends DecisionBase {
  kind: 'cleanup';
  reserve: { cardIds: number[]; n: number };
  landmarks: { cardIds: number[]; n: number };
}

export interface DeckDecision extends DecisionBase {
  kind: 'deck';
  decks: DeckSummary[];
}

/** A decision the client cannot render natively: plain buttons */
export interface ButtonsDecision extends DecisionBase {
  kind: 'buttons';
  buttons: { key: string; label: string }[];
  /** Noon of Nadir: put one of these markers on one of these regions (button key "markerId:region") */
  markRegion?: { markers: { id: number; biome: Biome }[]; regions: number[] };
}

export type Decision =
  | TurnDecision
  | SelectDecision
  | ChoiceDecision
  | ConfirmDecision
  | ExpeditionDecision
  | PlayerDecision
  | NumberDecision
  | LocationDecision
  | ExchangeDecision
  | CleanupDecision
  | DeckDecision
  | ButtonsDecision;

export type Answer = {
  decisionId: string;
  /** Sent by the client for the player (a decision without a real choice): the server's undo goes
   * back past it, to before the player's last choice */
  auto?: boolean;
} & (
  | { type: 'play'; cardId: number; destination: Destination }
  | { type: 'support'; cardId: number }
  | { type: 'tap'; cardId: number }
  | { type: 'endTurn' }
  | { type: 'select'; cardIds: number[] }
  | { type: 'choice'; id: number }
  | { type: 'confirm' }
  | { type: 'expedition'; keys: string[] }
  | { type: 'player'; player: PlayerId }
  | { type: 'number'; value: number }
  | { type: 'location'; key: string }
  | { type: 'exchange'; hand: number; reserve: number }
  | { type: 'cleanup'; reserve: number[]; landmarks: number[] }
  | { type: 'deck'; deck: string }
  | { type: 'button'; key: string }
  | { type: 'pass' } // skip an optional step
);

export interface NewGame {
  // deck: DeckSummary id, else chosen in game. ai: for a bot seat, the AiSummary id of the AI that plays it
  // (the simple bot by default); an AI plays its own deck under its own name (deck and name are ignored).
  // reunionDeck: an Altered Reunion deck, its decks-api id (a UUID, the deck must be public or the
  // player's): the server fetches it; a deck the engine refuses (not legal, card or Unique power not
  // implemented) gives an ErrorResponse (400, user: true) and no game
  players: { name: string; bot?: boolean; deck?: string; ai?: string; reunionDeck?: string }[];
  seed?: number;
}

/** GET /api/v1/ais: the AIs a bot seat can play */
export interface AiSummary {
  id: string;
  /** "player — hero" for an imitation AI */
  label: string;
  /** simple: the engine's bot; imitation: imitates a tournament player on their deck; deck: the deck's own AI */
  kind: 'simple' | 'imitation' | 'deck';
  /** The deck the AI plays (tournament deck id) and its hero */
  deck?: string;
  hero?: string;
}

export interface GameCreated {
  gameId: string;
  /** token: secret of each human seat (whoever creates a game holds all its human seats) */
  players: { id: PlayerId; name: string; bot: boolean; token?: string }[];
}

export interface MatchTicket {
  ticket: string;
  status: 'waiting' | 'matched';
  gameId?: string;
  player?: PlayerId;
  token?: string;
}

/** The host's seat (2001) and deck; the host gets both seats' tokens (the guest's goes in the link) */
export interface NewRoom {
  name: string;
  deck?: string;
  reunionDeck?: string;
}

export interface RoomCreated {
  roomId: string;
  players: { id: PlayerId; token: string }[];
}

/** A private room as one seat sees it: the other player's deck is never told */
export interface RoomView {
  roomId: string;
  seq: number;
  you?: PlayerId;
  /** The game exists (id: roomId, same tokens): open it */
  started: boolean;
  seats: { id: PlayerId; name: string; joined: boolean; ready: boolean; hasDeck: boolean }[];
  /** This seat's deck */
  deck: { deck: string } | { reunionDeck: string } | null;
  /** Why the game could not start (both players are no longer ready) */
  error?: string;
}

/** A change of one's seat. A new deck makes the player not ready; ready needs a deck. */
export interface SeatUpdate {
  name?: string;
  deck?: string;
  reunionDeck?: string;
  ready?: boolean;
}

export interface ErrorResponse {
  error: string;
  /** true when the move is illegal (the player can retry), false for server errors */
  user?: boolean;
}
