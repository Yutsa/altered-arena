import { useState } from 'react';
import type { PlayerView } from '@protocol';
import { useGameCtx } from '../game/GameContext';
import { artUrl, FACTION_NAMES } from '../api/catalog';
import { OpponentHand } from './Hand';
import { Glyph } from './Glyph';
import { CardListModal } from './Picker';
import './PlayerBar.css';

/** Player summary: hero, name, mana, deck, discard (and hand backs for the opponent). */
export function PlayerBar({ player, mine }: { player: PlayerView; mine: boolean }) {
  const { view, byId, def, inspectCard, zoneCards, decision, send } = useGameCtx();
  const [pile, setPile] = useState<'discard' | 'mana' | null>(null);
  const hero = player.heroCardId !== null ? byId.get(player.heroCardId) : undefined;
  const heroDef = def(hero);
  const active = view.activePlayer === player.id;
  const waiting = view.waitingFor.includes(player.id);
  const discard = zoneCards(player.id, 'discard');
  const mana = zoneCards(player.id, 'mana');
  const pickable = decision?.kind === 'player' && decision.players.includes(player.id);
  return (
    <div
      className={`player-bar ${mine ? 'mine' : 'theirs'} ${active ? 'active' : ''} ${pickable ? 'selectable' : ''}`}
      data-faction={player.faction ?? 'NE'}
      onClick={pickable ? () => void send({ type: 'player', player: player.id }) : undefined}
    >
      <button
        type="button"
        className="pb-hero"
        onClick={(e) => {
          if (pickable) return;
          e.stopPropagation();
          if (hero) inspectCard(hero.id);
        }}
        title={heroDef ? heroDef.name : 'Hero'}
      >
        {heroDef ? <img src={artUrl(heroDef)} alt="" draggable={false} /> : <span>?</span>}
      </button>
      <div className="pb-id">
        <div className="pb-name">
          {player.name}
          {player.bot && <span className="pb-tag">Bot</span>}
          {view.firstPlayer === player.id && <img className="pb-first" src="/img/icons/first-player.png" alt="First player" title="First player" />}
        </div>
        <div className="pb-sub">
          {heroDef ? heroDef.name : player.faction ? FACTION_NAMES[player.faction] : 'Choosing a deck…'}
          {player.passed && <span className="pb-passed">Passed</span>}
          {waiting && !mine && <span className="pb-thinking">thinking…</span>}
        </div>
      </div>
      <div className="pb-stats">
        <button
          type="button"
          className="pb-mana"
          title={`Mana: ${player.manaAvailable} available / ${player.manaTotal}`}
          onClick={(e) => {
            e.stopPropagation();
            if (mana.length) setPile('mana');
          }}
        >
          <b>{player.manaAvailable}</b>
          <small>/{player.manaTotal}</small>
        </button>
        {!mine && <OpponentHand count={player.handCount} />}
        <span className="pb-pile" title={`${player.deckCount} cards in deck`}>
          <Glyph name="card" /> {player.deckCount}
        </span>
        <button
          type="button"
          className="pb-pile"
          title={`${player.discardCount} cards in discard`}
          onClick={(e) => {
            e.stopPropagation();
            if (discard.length) setPile('discard');
          }}
        >
          <Glyph name="discard" /> {player.discardCount}
        </button>
      </div>
      {pile && (
        <CardListModal
          title={`${player.name}: ${pile === 'mana' ? 'Mana' : 'Discard'}`}
          cardIds={(pile === 'mana' ? mana : discard).map((c) => c.id)}
          onClose={() => setPile(null)}
        />
      )}
    </div>
  );
}
