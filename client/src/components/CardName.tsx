import { useInspect } from '../game/inspect';

/** A card name in a log line or prompt: hover (desktop) or tap shows the card. */
export function CardName({ name }: { name: string }) {
  const { resolveName, show, hide } = useInspect();
  const def = resolveName(name);
  if (!def) return <span className="rt-name">{name}</span>;
  return (
    <span
      className="rt-name rt-name-link"
      role="button"
      tabIndex={0}
      onPointerEnter={(e) => e.pointerType === 'mouse' && show({ def, mode: 'hover', x: e.clientX })}
      onPointerLeave={(e) => e.pointerType === 'mouse' && hide(true)}
      onClick={(e) => {
        e.stopPropagation();
        show({ def, mode: 'modal' });
      }}
    >
      {name}
    </span>
  );
}
