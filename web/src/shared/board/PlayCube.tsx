import { cubeImage, type PieceSet } from './piece-artwork';

// The recessed faces share one material image per set, at every cube value.
const cubeBodies = import.meta.glob<string>('./cubes/*.webp', {
  eager: true,
  import: 'default',
  query: '?url&no-inline',
});

export interface CubeInteraction {
  activate?: () => void;
  label: string;
  descriptionId?: string;
}

export default function PlayCube({
  label,
  value,
  owner,
  pending,
  transform,
  bounds,
  interaction,
  artwork,
  onError,
}: {
  label: string;
  value: number;
  owner: number | null;
  pending: boolean;
  transform?: string;
  bounds: { x: number; y: number; width: number; height: number };
  interaction?: CubeInteraction;
  artwork: PieceSet;
  onError?: () => void;
}) {
  const face = cubeImage(artwork.archive, value, owner, pending);
  const number = String(value === 1 ? 64 : value);
  const body =
    cubeBodies[`./cubes/${artwork.id}.webp`] ??
    cubeBodies['./cubes/slate.webp'];
  return (
    <g
      transform={transform}
      role={interaction ? 'button' : undefined}
      aria-label={
        interaction?.activate ? interaction.label + ' · ' + label : label
      }
      aria-disabled={interaction ? !interaction.activate : undefined}
      aria-describedby={interaction?.descriptionId}
      tabIndex={interaction?.activate ? 0 : undefined}
      onClick={interaction?.activate}
      onKeyDown={(event) => {
        if (!interaction?.activate || !['Enter', ' '].includes(event.key))
          return;
        event.preventDefault();
        interaction.activate();
      }}
      data-cube={value}
      data-cube-owner={owner ?? 'centered'}
      data-cube-pending={pending}
    >
      <title>{interaction ? interaction.label + ' · ' + label : label}</title>
      {interaction?.activate && (
        <rect {...bounds} fill="transparent" aria-hidden="true" />
      )}
      <g className="play-cube-artwork" data-cube-theme={artwork.id}>
        {face ? (
          <image href={face.src} {...bounds} onError={onError} />
        ) : (
          <svg
            {...bounds}
            viewBox="0 0 100 100"
            className={`theme-${artwork.id}`}
          >
            <image href={body} width="100" height="100" onError={onError} />
            <text
              className="cube-numeral"
              x="50"
              y="49"
              dominantBaseline="central"
              textAnchor="middle"
              fontSize={Math.min(62, 144 / number.length)}
            >
              {number}
            </text>
          </svg>
        )}
      </g>
    </g>
  );
}
