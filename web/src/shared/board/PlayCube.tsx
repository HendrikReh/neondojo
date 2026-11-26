import {cubeImage, type PieceSet} from './piece-artwork';

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
        {face ? <image href={face.src} {...bounds} onError={onError} /> : (
          <svg {...bounds} viewBox="0 0 24 24" className={`theme-${artwork.id}`}>
            <rect x=".5" y=".5" width="23" height="23" rx="3" fill="var(--white)" stroke="var(--black)" />
            <text x="12" y="16.5" textAnchor="middle" fontSize={value >= 100 ? 9 : 13} fill="var(--black)" fontWeight="600">
              {value === 1 ? '64' : value}
            </text>
          </svg>
        )}
      </g>
    </g>
  );
}
