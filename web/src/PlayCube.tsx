import Tooltip from './Tooltip';
import type { ReactNode } from 'react';
export interface CubeInteraction {
  enabled: boolean;
  label: string;
  activate: () => void;
}
/** Keep the actual cube artwork as the target for pointer and keyboard input. */
export default function PlayCube({
  interaction,
  label,
  value,
  owner,
  pending,
  transform,
  hitArea,
  children,
}: {
  interaction?: CubeInteraction;
  label: string;
  value: number;
  owner: number | null;
  pending: boolean;
  transform?: string;
  hitArea: { x: number; y: number; width: number; height: number };
  children: ReactNode;
}) {
  function activate() {
    if (interaction?.enabled) interaction.activate();
  }
  return (
    <Tooltip
      as="g"
      text={interaction ? `${interaction.label} · ${label}` : undefined}
      transform={transform}
      pointerEvents={interaction && !interaction.enabled ? 'none' : undefined}
      className={interaction ? 'play-cube' : undefined}
      role={interaction ? 'button' : undefined}
      tabIndex={interaction?.enabled ? 0 : -1}
      aria-disabled={interaction ? !interaction.enabled : undefined}
      aria-label={interaction ? `${interaction.label} · ${label}` : label}
      data-cube={value}
      data-cube-owner={owner ?? 'centered'}
      data-cube-pending={pending}
      onClick={interaction ? activate : undefined}
      onKeyDown={
        interaction
          ? (event) => {
              if (interaction.enabled && ['Enter', ' '].includes(event.key)) {
                event.preventDefault();
                activate();
              }
            }
          : undefined
      }
    >
      {!interaction && <title>{label}</title>}
      {interaction?.enabled && <rect {...hitArea} fill="transparent" />}
      {children}
    </Tooltip>
  );
}
