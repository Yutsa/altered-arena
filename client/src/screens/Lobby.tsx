import { useState } from 'react';
import type { PlayerId } from '@protocol';
import { useCatalog, useDecks } from '../api/catalog';
import { api, ApiError } from '../api/client';
import { InspectProvider } from '../game/inspect';
import { DeckGrid } from '../components/Picker';
import { Inspector } from '../components/Inspector';
import { toast } from '../game/toasts';
import { lastGame, saveDeck, savedDeck, savedName, saveName } from './storage';
import './Lobby.css';

type Mode = 'bot' | 'hotseat' | 'join';

export function Lobby({ onJoin }: { onJoin: (gameId: string, player: PlayerId) => void }) {
  const { catalog, error: catErr } = useCatalog();
  const { decks, error: deckErr } = useDecks();
  const [mode, setMode] = useState<Mode>('bot');
  const [name, setName] = useState(savedName() || 'Player');
  const [name2, setName2] = useState('Player 2');
  const [deck, setDeck] = useState<string | null>(savedDeck());
  const [busy, setBusy] = useState(false);
  const [hotseat, setHotseat] = useState<{ gameId: string; players: { id: PlayerId; name: string }[] } | null>(null);
  const [joinId, setJoinId] = useState('');
  const [joinPlayer, setJoinPlayer] = useState<PlayerId>(2001);
  const last = lastGame();

  const fail = (e: unknown) => toast(e instanceof ApiError ? e.message : String(e));

  async function startBot() {
    if (!deck) return;
    setBusy(true);
    saveName(name);
    saveDeck(deck);
    try {
      const g = await api.createGame({ players: [{ name: name || 'Player', deck }, { name: 'Bot', bot: true }] });
      onJoin(g.gameId, g.players[0].id);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function startHotseat() {
    setBusy(true);
    saveName(name);
    try {
      const g = await api.createGame({ players: [{ name: name || 'Player 1' }, { name: name2 || 'Player 2' }] });
      setHotseat({ gameId: g.gameId, players: g.players });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  const link = (gameId: string, p: PlayerId) => `${location.origin}${location.pathname}?game=${encodeURIComponent(gameId)}&player=${p}`;
  const selectedDeck = decks?.find((d) => d.id === deck);

  return (
    <InspectProvider catalog={catalog}>
      <div className="lobby-scroll">
      <div className="lobby">
        <header className="lobby-head">
          <h1>
            Altered <span>Arena</span>
          </h1>
          {last && (
            <button type="button" className="btn" onClick={() => onJoin(last.gameId, last.player)}>
              Resume game {last.gameId}
            </button>
          )}
        </header>

        <div className="lobby-tabs" role="tablist">
          {(
            [
              ['bot', 'Play vs Bot'],
              ['hotseat', 'Two players'],
              ['join', 'Join a game'],
            ] as [Mode, string][]
          ).map(([m, label]) => (
            <button key={m} type="button" role="tab" aria-selected={mode === m} className={`tab ${mode === m ? 'on' : ''}`} onClick={() => setMode(m)}>
              {label}
            </button>
          ))}
        </div>

        {(catErr || deckErr) && <p className="lobby-error">Server unreachable: {catErr ?? deckErr}</p>}

        {mode === 'bot' && (
          <section className="lobby-section">
            <label className="field">
              <span>Your name</span>
              <input value={name} onChange={(e) => setName(e.target.value)} maxLength={24} />
            </label>
            <h2>Choose a starter deck</h2>
            {decks && catalog ? (
              <DeckGrid decks={decks} catalog={catalog} selected={deck} onSelect={(d) => setDeck(d.id)} />
            ) : (
              <p className="muted">Loading decks…</p>
            )}
            <div className="lobby-cta">
              <span className="muted">{selectedDeck ? `Deck: ${selectedDeck.name}` : 'Pick a deck'}</span>
              <button
                type="button"
                className="btn"
                disabled={!decks}
                onClick={() => decks && setDeck(decks[Math.floor(Math.random() * decks.length)].id)}
              >
                Random
              </button>
              <button type="button" className="btn primary big" disabled={!deck || busy} onClick={startBot}>
                {busy ? 'Starting…' : 'Play'}
              </button>
            </div>
          </section>
        )}

        {mode === 'hotseat' && (
          <section className="lobby-section narrow">
            <p className="muted">
              Creates a game between two human players. Each player chooses a deck in game. Open each player in its own tab or device, or
              switch between them on one device.
            </p>
            <label className="field">
              <span>Player 1</span>
              <input value={name} onChange={(e) => setName(e.target.value)} maxLength={24} />
            </label>
            <label className="field">
              <span>Player 2</span>
              <input value={name2} onChange={(e) => setName2(e.target.value)} maxLength={24} />
            </label>
            <button type="button" className="btn primary" disabled={busy} onClick={startHotseat}>
              Create game
            </button>
            {hotseat && (
              <div className="hotseat-links">
                <h3>Game {hotseat.gameId}</h3>
                {hotseat.players.map((p) => (
                  <div key={p.id} className="hotseat-row">
                    <b>{p.name}</b>
                    <input readOnly value={link(hotseat.gameId, p.id)} onFocus={(e) => e.currentTarget.select()} />
                    <button
                      type="button"
                      className="btn"
                      onClick={() => {
                        void navigator.clipboard?.writeText(link(hotseat.gameId, p.id));
                        toast('Link copied', 'info', 1500);
                      }}
                    >
                      Copy
                    </button>
                    <button type="button" className="btn ghost" onClick={() => window.open(link(hotseat.gameId, p.id), '_blank')}>
                      New tab
                    </button>
                    <button type="button" className="btn primary" onClick={() => onJoin(hotseat.gameId, p.id)}>
                      Play here
                    </button>
                  </div>
                ))}
              </div>
            )}
          </section>
        )}

        {mode === 'join' && (
          <section className="lobby-section narrow">
            <label className="field">
              <span>Game id</span>
              <input value={joinId} onChange={(e) => setJoinId(e.target.value.trim())} placeholder="e.g. 12" />
            </label>
            <label className="field">
              <span>Seat</span>
              <select value={joinPlayer} onChange={(e) => setJoinPlayer(Number(e.target.value))}>
                <option value={2001}>Player 1 (2001)</option>
                <option value={2002}>Player 2 (2002)</option>
              </select>
            </label>
            <button type="button" className="btn primary" disabled={!joinId} onClick={() => onJoin(joinId, joinPlayer)}>
              Join
            </button>
            <p className="muted">You can also open a link of the form ?game=ID&amp;player=2001.</p>
          </section>
        )}
      </div>
      </div>
      <Inspector />
    </InspectProvider>
  );
}
