import { useEffect, useState } from 'react';
import type { GameView } from '@protocol';
import type { DiceRoll } from '../game/useDiceStage';
import './dice.css';

/** Pip positions on a 3×3 grid (0..8, row by row) for each face */
const PIPS: Record<number, number[]> = {
  1: [4],
  2: [2, 6],
  3: [2, 4, 6],
  4: [0, 2, 6, 8],
  5: [0, 2, 4, 6, 8],
  6: [0, 2, 3, 5, 6, 8],
};

function Face({ value }: { value: number }) {
  return (
    <span className="die-face">
      {Array.from({ length: 9 }, (_, i) => (
        <span key={i} className={PIPS[value]?.includes(i) ? 'pip on' : 'pip'} />
      ))}
    </span>
  );
}

/** One die: random faces while it tumbles, then its value */
function Die({ value, settled, index }: { value: number; settled: boolean; index: number }) {
  const [face, setFace] = useState(() => 1 + Math.floor(Math.random() * 6));
  useEffect(() => {
    if (settled) return;
    const t = setInterval(() => setFace((f) => 1 + ((f + Math.floor(Math.random() * 5)) % 6)), 90);
    return () => clearInterval(t);
  }, [settled]);
  return (
    <span className={`die ${settled ? 'settled' : 'rolling'}`} style={{ animationDelay: `${index * 70}ms` }}>
      <Face value={settled ? value : face} />
    </span>
  );
}

/** A die roll in the middle of the screen: the dice tumble, then show their result */
export function DiceRollOverlay({ roll, view }: { roll: DiceRoll | null; view: GameView | null }) {
  if (!roll) return null;
  const who = view?.players.find((p) => p.id === roll.player);
  const mine = roll.player !== null && roll.player === view?.you;
  return (
    <div className="dice-roll" key={roll.seq} role="status" aria-live="polite">
      <div className="dice-row">
        {roll.values.map((v, i) => (
          <Die key={i} value={v} settled={roll.settled} index={i} />
        ))}
      </div>
      <div className={`dice-caption ${mine ? 'me' : 'opp'}`}>
        {who ? `${mine ? 'Vous lancez' : `${who.name} lance`} ${roll.values.length > 1 ? 'les dés' : 'le dé'}` : 'Lancer de dé'}
        {roll.settled && <strong> : {roll.values.join(', ')}</strong>}
      </div>
    </div>
  );
}
