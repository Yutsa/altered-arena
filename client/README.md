# Altered Arena web client

React + TypeScript single page app (Vite) for Altered Arena: lobby, game table, every decision kind of
protocol v1, drag and drop, animations, phone/tablet/desktop layouts, installable PWA.

The client talks only through `../protocol/protocol.ts` (imported as `@protocol`), the single source of
truth for the types. It works with both servers: the PHP oracle (port 8080) and the Rust server (port 8090).

## Run

```sh
cd client
npm install
npm run dev                                  # http://localhost:5173, proxies /api and /assets to :8080
ARENA_API=http://localhost:8090 npm run dev  # same, against the Rust server
```

The game server must be running (`docker compose up -d` in `altered-arena/` for the PHP one,
`tools/restart_rust.sh` for the Rust one).

URLs:

- `/` lobby: play against the bot, find an opponent (matchmaking), two players (hot seat), join.
- `/?game=ID&player=PID&token=T` a seat of a game. The token is the seat secret returned by
  `POST /games` or matchmaking; tokens are also kept in localStorage, so `?game=ID&player=PID` reopens
  a seat on the device that created it. Without a token the view is a spectator view.
- `/?game=ID` watch a game as a spectator.
- `/?replay` (dev only) step through recorded GameViews (`?replay=/file.json` loads a file from
  `public/`). Answers are not sent but shown as a toast, to check the renderers without a server.

## Build

```sh
npm run build      # tsc --noEmit, then vite build -> client/dist (served by the server at /)
npm run typecheck  # app + node-side files (vite config, scripts)
npm run preview    # serve dist on :4173 with the same proxy
```

The build registers a service worker (vite-plugin-pwa, Workbox): app shell precached, card art cached
on first use (CacheFirst), `/api/v1/cards` and `/api/v1/decks` stale-while-revalidate. Game requests
are never cached.

## Checks

```sh
npm run check:decisions -- --games 5                          # vs bot, PHP server
npm run check:decisions -- --games 5 --server http://localhost:8090
npm run check:decisions -- --games 2 --hotseat --record /tmp/views.json
```

`scripts/check-decisions.ts` plays whole games through the client's decision logic
(`src/game/decisions.ts`: affordances, `togglePick`, `confirmAnswer`...), checks that every decision
kind has a renderer and that every answer is accepted, and reports unknown cards and unknown markup.
`--record` writes the views for the replay page.

`scripts/ui-autoplay.js` drives the real UI in a browser (DOM events, random choices) until the game
ends: paste it in the console of a game page.

## Structure

```
src/
  api/          client.ts (HTTP, ApiError, tokens, matchmaking), catalog.ts (cards/decks cache, art URL)
  game/
    useGame.ts      GameView state: first fetch, long poll with since, answers, log accumulation
    decisions.ts    pure decision logic (what is clickable, picks, the Answer to send)
    GameContext.tsx view + interaction state shared by the table components
    drag.tsx        pointer-event drag and drop (mouse, touch, pen), drop zones via data-drop
    useCardPointer.ts  click / drag / long press / hover of a card
    inspect.tsx     large card view state (hover preview, modal), card name resolution
    keywords.ts     effect keywords and reminders
    toasts.ts       error toasts
  components/
    Card            card drawn in CSS (sizes mini/small/hand/zoom, all in container units)
    BoardCard       interactive card, animated between zones (motion layoutId per card id)
    Hand, Board (expedition rows, reserve, landmarks), Tumult (regions, markers, cast zone)
    PlayerBar, DecisionBar (all decision kinds), Picker (select picker, deck grid), Inspector,
    Log, PhaseIndicator, EndScreen, Toasts, Modal, RichText (effect markup), Glyph (icon font)
  screens/      Lobby, GameScreen, storage (name, deck, last game, seat tokens)
  dev/          ReplayScreen (dev only, not in production builds)
public/         fonts, icons, Tumult tokens and card back taken from AlteredBGA/img, PWA icons
```

Interaction model:

- Turn: playable cards glow green. Drag one onto a highlighted zone (expedition, Scout area,
  landmarks, reserve, the cast circle over the Tumult for spells), or tap it and pick a zone or a
  button (destinations with their cost, support, exhaust ability). End turn asks for a second tap
  while cards can still be played.
- Other decisions: the prompt and its buttons are in the decision bar; selectable cards pulse on the
  table; a picker opens when the cards are not on the table (mana, discard, deck) or `showInPicker`.
  Expedition and location decisions can also be answered by clicking an expedition row or marker.
- Hover (mouse) previews a card; tap (or long press on touch) opens the large version with its
  statuses, boosts and counters explained.
