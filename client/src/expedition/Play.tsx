import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { AiSummary, CardDef, PlayerId } from '@protocol';
import { artUrl, useCatalog, useDecks } from '../api/catalog';
import { api, ApiError } from '../api/client';
import { InspectProvider } from '../game/inspect';
import { Inspector } from '../components/Inspector';
import { toast } from '../game/toasts';
import { markSoloGame, saveToken } from '../screens/storage';
import { Shell } from './Shell';
import { useDeckChoice } from './deckChoice';
import { FACTIONS_FR, heroIndex } from './heroes';
import './play.css';

export type PlayMode = 'match' | 'friend' | 'ai' | 'self';

const AI_KINDS: [AiSummary['kind'], string, string][] = [
  ['imitation', "IA d'imitation", 'joue comme un joueur de tournoi'],
  ['deck', 'IA de deck', 'maîtrise une liste précise'],
  ['simple', 'Bot simple', 'pour découvrir le jeu'],
];

/** The AIs of the server, read once per page (the hub opens again without waiting for them) */
let aisCache: AiSummary[] | null = null;

const fail = (e: unknown) => toast(e instanceof ApiError ? e.message : String(e));

/** A card of the stage: a hero's art, a card back, or an empty seat */
function StageCard({ def, back, children }: { def?: CardDef | null; back?: boolean; children?: ReactNode }) {
  if (back || !def)
    return (
      <span className={`stage-card ${back ? 'back' : 'empty'}`}>
        {back && <img src="/img/card-back.jpg" alt="" />}
        {children}
      </span>
    );
  return (
    <span className="stage-card">
      <img src={artUrl(def)} alt={def.name} />
      {children}
    </span>
  );
}

/** The play hub: matchmaking, a private room for a friend, a game against an AI */
export function Play({
  initialMode,
  onJoin,
  onRoom,
}: {
  initialMode: PlayMode;
  onJoin: (gameId: string, player: PlayerId, token?: string | null) => void;
  onRoom: (gameId: string) => void;
}) {
  const { catalog, error: catErr } = useCatalog();
  const { decks, error: deckErr } = useDecks();
  const [mode, setMode] = useState<PlayMode>(initialMode);
  const { name, chosen, hero: myHero, deckName, deckFaction, remember, open: pickDeck, dialog } = useDeckChoice(catalog, decks);
  // The other side of a game against oneself: its own saved name and deck
  const second = useDeckChoice(catalog, decks, undefined, { slot: '.self2', defaultName: 'Joueur 2', title: 'Deck du second joueur' });
  const [busy, setBusy] = useState(false);
  const [ticket, setTicket] = useState<string | null>(null);
  const ticketRef = useRef<string | null>(null);
  // The pending long poll of the queue, cut short when the search is cancelled
  const pollRef = useRef<AbortController | null>(null);
  // Number of the current search: a cancelled search that ends late leaves the next one alone
  const searchRef = useRef(0);
  const [ais, setAis] = useState<AiSummary[] | null>(aisCache);
  const [aiKind, setAiKind] = useState<AiSummary['kind']>('imitation');
  const [aiId, setAiId] = useState<string | null>(null);

  useEffect(() => {
    if (aisCache) return;
    api.ais().then(
      (a) => setAis((aisCache = a)),
      () => setAis([]),
    );
  }, []);
  // Leave the queue when the screen closes
  useEffect(
    () => () => {
      pollRef.current?.abort();
      if (ticketRef.current) void api.leaveQueue(ticketRef.current);
    },
    [],
  );

  const heroes = catalog ? heroIndex(catalog) : null;
  const aiList = (ais ?? []).filter((a) => a.kind === aiKind);
  const ai = aiList.find((a) => a.id === aiId) ?? aiList[0] ?? null;
  const aiHero = ai?.hero && heroes ? (heroes.byName.get(ai.hero) ?? null) : null;

  async function findOpponent() {
    if (!chosen) return pickDeck();
    remember();
    const search = ++searchRef.current;
    setBusy(true);
    try {
      let t = await api.matchmake(name, chosen);
      if (search !== searchRef.current) {
        // Cancelled while joining the queue: leave it at once
        void api.leaveQueue(t.ticket);
        return;
      }
      ticketRef.current = t.ticket;
      setTicket(t.ticket);
      while (t.status !== 'matched' && ticketRef.current === t.ticket) {
        const ctrl = new AbortController();
        pollRef.current = ctrl;
        try {
          t = await api.ticket(t.ticket, ctrl.signal);
        } catch (e) {
          // Cancelled: the search ends here, without an error
          if (ticketRef.current !== t.ticket) break;
          if (e instanceof ApiError && e.user) throw e;
          await new Promise((r) => setTimeout(r, 2000)); // network hiccup: retry
        }
      }
      if (t.status === 'matched' && t.gameId && t.player && ticketRef.current === t.ticket) {
        ticketRef.current = null;
        onJoin(t.gameId, t.player, t.token);
      }
    } catch (e) {
      if (search === searchRef.current) fail(e);
    } finally {
      if (search === searchRef.current) {
        pollRef.current = null;
        setBusy(false);
        setTicket(null);
      }
    }
  }
  function leaveQueue() {
    searchRef.current++;
    const t = ticketRef.current;
    ticketRef.current = null;
    pollRef.current?.abort();
    pollRef.current = null;
    setTicket(null);
    setBusy(false);
    if (t) void api.leaveQueue(t);
  }

  /** A private room: the friend opens the link, chooses a deck, and the game starts once both are ready */
  async function createRoom() {
    if (!chosen) return pickDeck();
    remember();
    setBusy(true);
    try {
      const r = await api.createRoom({ name, ...chosen });
      for (const p of r.players) saveToken(r.roomId, p.id, p.token);
      onRoom(r.roomId);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  async function playAi() {
    if (!chosen) return pickDeck();
    if (!ai) return;
    remember();
    setBusy(true);
    try {
      const g = await api.createGame({ players: [{ name, ...chosen }, { name: 'Bot', bot: true, ai: ai.id }] });
      onJoin(g.gameId, g.players[0].id, g.players[0].token);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  /** Both seats on this device: the player plays one deck, then the other, switching point of view */
  async function playSelf() {
    if (!chosen) return pickDeck();
    if (!second.chosen) return second.open();
    remember();
    second.remember();
    setBusy(true);
    try {
      const g = await api.createGame({ players: [{ name, ...chosen }, { name: second.name === name ? `${second.name} 2` : second.name, ...second.chosen }] });
      for (const p of g.players) saveToken(g.gameId, p.id, p.token);
      markSoloGame(g.gameId);
      onJoin(g.gameId, g.players[0].id, g.players[0].token);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }

  function pick(m: PlayMode) {
    if (ticket) leaveQueue();
    setMode(m);
    history.replaceState(null, '', m === 'match' ? '?play' : `?play=${m}`);
  }

  const tiles: { m: PlayMode; tag: string; title: string; text: string; badge?: boolean }[] = [
    { m: 'match', tag: 'En ligne', title: "Recherche d'adversaire", text: 'Un joueur réel vous attend.' },
    { m: 'friend', tag: 'Privé', title: 'Défier un ami', text: 'Un salon, un lien à envoyer.' },
    { m: 'ai', tag: 'Entraînement', title: "Contre l'IA", text: 'Des IA qui jouent comme en tournoi.' },
    { m: 'self', tag: 'Solo', title: 'Contre moi-même', text: 'Deux decks, vous jouez les deux camps.' },
  ];

  return (
    <InspectProvider catalog={catalog}>
      <Shell current="play">
        <div className="xp-head">
          <h1>Jouer</h1>
          <button type="button" className="xp-chip" onClick={pickDeck} aria-label="Pseudo et deck">
            {myHero ? <img className="xp-por" src={artUrl(myHero)} alt="" width={40} height={40} /> : <span className="xp-por play-por-empty" />}
            <span>
              <strong>{name}</strong>
              <small>{deckName ? `Deck : ${deckName}` : 'Aucun deck choisi'}</small>
            </span>
          </button>
        </div>
        {(catErr || deckErr) && <p className="play-error">Serveur injoignable : {catErr ?? deckErr}</p>}

        <div className="play-grid">
          <div className="play-modes" role="tablist" aria-label="Modes de jeu">
            {tiles.map((t) => (
              <button
                key={t.m}
                type="button"
                role="tab"
                aria-selected={mode === t.m}
                className={`play-mode ${mode === t.m ? 'on' : ''}`}
                onClick={() => pick(t.m)}
              >
                <span className="play-mini" aria-hidden="true">
                  {myHero ? <img src={artUrl(myHero)} alt="" /> : <span className="mini-empty" />}
                  {t.m === 'match' && <img src="/img/card-back.jpg" alt="" />}
                  {t.m === 'friend' && (
                    <span className="mini-slot">
                      <LinkIcon />
                    </span>
                  )}
                  {t.m === 'ai' && (aiHero ? <img src={artUrl(aiHero)} alt="" /> : <img src="/img/card-back.jpg" alt="" />)}
                  {t.m === 'self' && (second.hero ? <img src={artUrl(second.hero)} alt="" /> : <span className="mini-empty" />)}
                  {t.m === 'self' && (
                    <span className="mini-badge">
                      <SwapIcon />
                    </span>
                  )}
                  {t.m === 'ai' && (
                    <span className="mini-badge">
                      <BotIcon />
                    </span>
                  )}
                </span>
                <span className="play-mode-text">
                  <span className="xp-tag">{t.tag}</span>
                  <span className="play-mode-title">{t.title}</span>
                  <span className="xp-muted">{t.text}</span>
                </span>
              </button>
            ))}
          </div>

          <section className="play-panel" aria-live="polite">
            {/* Against an AI, the room goes to the list of AIs: no stage */}
            {mode !== 'ai' && (
              <div className="play-stage">
                <figure>
                  <StageCard def={myHero} />
                  <figcaption>
                    <strong>{name}</strong>
                    <span>{myHero ? myHero.name : 'Choisissez un deck'}</span>
                  </figcaption>
                </figure>
                <span className="stage-vs">contre</span>
                <figure>
                  {mode === 'match' && (
                    <>
                      <StageCard back>{ticket && <span className="stage-spin" />}</StageCard>
                      <figcaption>
                        <strong>{ticket ? 'Recherche…' : 'Adversaire'}</strong>
                        <span>{ticket ? 'un joueur en ligne' : 'inconnu'}</span>
                      </figcaption>
                    </>
                  )}
                  {mode === 'self' && (
                    <>
                      <StageCard def={second.hero} />
                      <figcaption>
                        <strong>{second.name}</strong>
                        <span>{second.hero ? second.hero.name : 'Choisissez un deck'}</span>
                      </figcaption>
                    </>
                  )}
                  {mode === 'friend' && (
                    <>
                      <StageCard>
                        <LinkIcon size={40} />
                        <span className="stage-free">Place libre</span>
                      </StageCard>
                      <figcaption>
                        <strong>Votre ami</strong>
                        <span>rejoint par le lien</span>
                      </figcaption>
                    </>
                  )}
                </figure>
              </div>
            )}

            <div className="play-controls">
              <div className="play-deck-row">
                <button type="button" className="xp-btn play-deck" onClick={pickDeck}>
                  {myHero ? <img className="xp-por" src={artUrl(myHero)} alt="" width={40} height={40} /> : null}
                  <span className="play-deck-name">
                    <strong>{deckName ?? 'Choisir un deck'}</strong>
                    <small>{deckFaction ? FACTIONS_FR[deckFaction] : 'Starter ou Altered Reunion'}</small>
                  </span>
                  <span className="play-deck-change">Changer de deck</span>
                </button>
                {mode === 'match' && <span className="xp-muted">{ticket ? 'En attente d’un autre joueur…' : 'Un joueur en ligne, une partie complète.'}</span>}
                {mode === 'self' && (
                  <button type="button" className="xp-btn play-deck" onClick={second.open}>
                    {second.hero ? <img className="xp-por" src={artUrl(second.hero)} alt="" width={40} height={40} /> : null}
                    <span className="play-deck-name">
                      <strong>{second.deckName ?? 'Choisir le second deck'}</strong>
                      <small>{second.deckFaction ? `${second.name} · ${FACTIONS_FR[second.deckFaction]}` : 'Le camp adverse'}</small>
                    </span>
                    <span className="play-deck-change">Changer de deck</span>
                  </button>
                )}
                {mode === 'friend' && (
                  <span className="xp-muted">Votre ami ouvre le lien et choisit son deck ; la partie démarre quand vous êtes prêts tous les deux. Aucun compte requis.</span>
                )}
              </div>

              {mode === 'ai' && (
                <div className="play-ai">
                  <div className="play-ai-kinds" role="radiogroup" aria-label="Type d'IA">
                    {AI_KINDS.filter(([k]) => ais?.some((a) => a.kind === k)).map(([k, label, hint]) => (
                      <button
                        key={k}
                        type="button"
                        role="radio"
                        aria-checked={aiKind === k}
                        className={`play-opt ${aiKind === k ? 'on' : ''}`}
                        onClick={() => {
                          setAiKind(k);
                          setAiId(null);
                        }}
                      >
                        <strong>{label}</strong>
                        <small>{hint}</small>
                      </button>
                    ))}
                  </div>
                  {aiKind !== 'simple' && (
                    <div className="play-ai-list" role="radiogroup" aria-label="Adversaire">
                      {aiList.map((a) => {
                        const h = a.hero && heroes ? heroes.byName.get(a.hero) : null;
                        return (
                          <button
                            key={a.id}
                            type="button"
                            role="radio"
                            aria-checked={ai?.id === a.id}
                            className={`play-ai-item ${ai?.id === a.id ? 'on' : ''}`}
                            aria-label={a.label}
                            onClick={() => setAiId(a.id)}
                          >
                            {h ? <img className="xp-por" src={artUrl(h)} alt="" width={44} height={44} /> : <span className="xp-por play-por-empty" />}
                            <span>{a.label}</span>
                          </button>
                        );
                      })}
                      {ais === null && <span className="xp-muted">Chargement des IA…</span>}
                    </div>
                  )}
                </div>
              )}

              {mode === 'match' &&
                (ticket ? (
                  <button type="button" className="xp-btn big" onClick={leaveQueue}>
                    Annuler la recherche
                  </button>
                ) : (
                  <button type="button" className="xp-btn gold big" disabled={busy} onClick={findOpponent}>
                    Lancer la recherche
                  </button>
                ))}
              {mode === 'friend' && (
                <button type="button" className="xp-btn gold big" disabled={busy} onClick={createRoom}>
                  {busy ? 'Création…' : 'Créer le salon'}
                </button>
              )}
              {mode === 'self' && (
                <>
                  <span className="xp-muted play-self-hint">Vous jouez les deux camps sur cet appareil : le point de vue passe au joueur qui doit décider.</span>
                  <button type="button" className="xp-btn gold big" disabled={busy} onClick={playSelf}>
                    {busy ? 'Lancement…' : 'Lancer la partie'}
                  </button>
                </>
              )}
              {mode === 'ai' && (
                <button type="button" className="xp-btn gold big" disabled={busy || !ai} onClick={playAi}>
                  {busy ? 'Lancement…' : "Affronter l'IA"}
                </button>
              )}
            </div>
          </section>
        </div>
      </Shell>

      {dialog}
      {second.dialog}
      <Inspector />
    </InspectProvider>
  );
}

function SwapIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <path d="M4 8h14l-4-4M20 16H6l4 4" />
    </svg>
  );
}

export function LinkIcon({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
      <path d="M10 14a4 4 0 0 0 5.6 0l3-3a4 4 0 0 0-5.6-5.6l-1 1" />
      <path d="M14 10a4 4 0 0 0-5.6 0l-3 3a4 4 0 0 0 5.6 5.6l1-1" />
    </svg>
  );
}

function BotIcon() {
  return (
    <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <rect x="5" y="7" width="14" height="12" rx="3" />
      <path d="M12 3v4M9 12h.01M15 12h.01" />
    </svg>
  );
}
