import type { ReactNode } from 'react';

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
  children,
}: {
  label: string;
  value: number;
  owner: number | null;
  pending: boolean;
  transform?: string;
  bounds: { x: number; y: number; width: number; height: number };
  interaction?: CubeInteraction;
  children: ReactNode;
}) {
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
      <g className="play-cube-artwork">{children}</g>
    </g>
  );
}
