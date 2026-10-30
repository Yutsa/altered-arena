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
```

The game server must be running (`cargo run --release -p arena-server` in `altered-arena/`, or
`docker compose up -d` for the PHP oracle, which speaks the same protocol).

URLs:

- `/` lobby: play against the bot, find an opponent (matchmaking), two players (hot seat), join.
- `/?game=ID&player=PID` a seat of a game. The seat's secret token (returned by `POST /games` or
  matchmaking) is kept in localStorage and sent in the `Authorization` header, never in URLs. Share
  links carry it in the fragment (`#token=T`, never sent to the server), stored then removed from the
  address bar. Without a token the view is a spectator view.
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
  arena/        the game table
    Arena.tsx       stage scaled to the window, the mat tilted in CSS 3D, HUD, flights
    layout.ts       the two stages (wide 1440x900, phone landscape 844x390), mat slices of board.jpg
    Mat.tsx         mat rows: player zones (piles, Reserve, Landmarks), expedition bands (totals,
                    expeditions, hero card), the Tumult track (storm.jpg tiles, markers, forecast)
    MatCard.tsx     card on the mat: art, statistics column (green when boosted), "+1" effects
    Hud.tsx         player chips, mana gauges, opponent's hand, day/phase, prompt, hand, phase banner
    TrackButton.tsx the round button in the middle of the track (end turn, confirm, pass)
    flights.ts      cards flying between zones (Web Animations, no React state)
    model.ts        pure: Tumult card per region pair, Dusk forecast, track geometry
  components/
    Card            card drawn in CSS (sizes mini/small/hand/zoom, all in container units)
    BoardCard       interactive card of the hand and pickers (motion layoutId per card id)
    DecisionBar (all decision kinds), Picker (select picker, deck grid), Inspector,
    Log, EndScreen, Toasts, Modal, RichText (effect markup), Glyph (icon font)
  screens/      Lobby, GameScreen, storage (name, deck, last game, seat tokens)
  dev/          ReplayScreen (dev only, not in production builds)
public/         fonts, icons, Tumult tokens, card back, and img/arena (mat, Tumult cards, sky,
                landmark slot) taken from AlteredBGA/img, PWA icons
```

Interaction model:

- Turn: playable cards glow green. Drag one onto a highlighted zone (expedition, Scout area,
  landmarks, reserve, the Tumult track for spells), or tap it and pick a zone or a button
  (destinations with their cost, support, exhaust ability). The round button in the middle of the
  track ends the turn (a second tap is asked while cards can still be played), confirms picks or
  passes an optional step.
- Totals at both ends of each expedition band show its statistics; biomes of its region are lit,
  and a lit arrow marks the biomes where it beats the opposing expedition. A dashed circle on the
  track shows where a marker goes at Dusk. Phones play in landscape.
- Other decisions: the prompt and its buttons are in the decision bar; selectable cards pulse on the
  table; a picker opens when the cards are not on the table (mana, discard, deck) or `showInPicker`.
  Expedition and location decisions can also be answered by clicking an expedition row or marker.
- Hover (mouse) previews a card; tap (or long press on touch) opens the large version with its
  statuses, boosts and counters explained.
