import type { Phase } from '@protocol';

const PHASES: { phase: Phase; label: string; icon: string }[] = [
  { phase: 'morning', label: 'Morning', icon: 'dawn' },
  { phase: 'noon', label: 'Noon', icon: 'noon' },
  { phase: 'afternoon', label: 'Afternoon', icon: 'afternoon' },
  { phase: 'dusk', label: 'Dusk', icon: 'dusk' },
  { phase: 'night', label: 'Night', icon: 'night' },
];

/** Day number and the five phases of the day, the current one highlighted. */
export function PhaseIndicator({ day, phase }: { day: number; phase: Phase }) {
  const label =
    phase === 'setup' ? 'Setup' : phase === 'tiebreaker' ? 'Tiebreaker' : phase === 'ended' ? 'Game over' : PHASES.find((p) => p.phase === phase)?.label;
  return (
    <div className="phase-indicator" title={`Day ${day} — ${label}`}>
      <span className="pi-day">Day {day}</span>
      <span className="pi-phases">
        {PHASES.map((p) => (
          <img key={p.phase} src={`/img/icons/${p.icon}.png`} alt={p.label} className={p.phase === phase ? 'current' : ''} />
        ))}
      </span>
      <span className="pi-label">{label}</span>
    </div>
  );
}
