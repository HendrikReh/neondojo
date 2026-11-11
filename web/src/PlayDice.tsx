import Tooltip from './Tooltip';
import { useId, type ReactNode } from 'react';
export interface DiceInteraction {
  canSwap: boolean;
  canConfirm?: boolean;
  canRoll?: boolean;
  preRoll?: boolean;
  roll?: () => void;
  confirm?: () => void;
  swap: () => void;
  label: string;
  /** Per physical die: false/0 unused, 0.5 half used for doubles, true/1 used. */
  used: (boolean | number)[];
  active: number | null;
  rolling?: boolean;
  opening?: { values: (number | null)[]; labels: string[]; unrolled: string };
}
export default function PlayDice({
  index,
  width,
  height,
  interaction,
  children,
}: {
  index: number;
  width: number;
  height: number;
  interaction?: DiceInteraction;
  children: ReactNode;
}) {
  const maskId = useId();
  if (!interaction) return <>{children}</>;
  const used = Number(interaction.used[index] ?? 0);
  const halfUsed = used > 0 && used < 1;
  const confirm = Boolean(interaction.canConfirm && interaction.confirm);
  const roll = Boolean(interaction.canRoll && interaction.roll);
  const enabled = roll || confirm || interaction.canSwap;
  function activate() {
    if (roll) interaction?.roll?.();
    else if (confirm) interaction?.confirm?.();
    else if (interaction?.canSwap) interaction.swap();
  }
  return (
    <Tooltip
      as="g"
      text={interaction.label}
      className={`play-die ${interaction.rolling ? 'play-die-rolling' : ''}`}
      data-rolling={Boolean(interaction.rolling)}
      role="button"
      tabIndex={enabled ? 0 : -1}
      aria-label={interaction.label}
      aria-disabled={!enabled}
      data-dice-action={
        roll
          ? 'roll'
          : confirm
            ? 'confirm'
            : interaction.canSwap
              ? 'swap'
              : 'none'
      }
      data-play-die-index={index}
      data-used={used >= 1}
      data-used-fraction={used}
      onClick={activate}
      onKeyDown={(event) => {
        if (enabled && ['Enter', ' '].includes(event.key)) {
          event.preventDefault();
          activate();
        }
      }}
    >
      <rect
        className="play-die-hit"
        x="0"
        y="0"
        width={width}
        height={height + 8}
      />
      {halfUsed && (
        <defs>
          <mask
            id={maskId}
            maskUnits="userSpaceOnUse"
            x="0"
            y="0"
            width={width}
            height={height}
          >
            <rect
              width={width / 2}
              height={height}
              fill="white"
              fillOpacity="0.28"
            />
            <rect
              x={width / 2}
              width={width / 2}
              height={height}
              fill="white"
            />
          </mask>
        </defs>
      )}
      <g
        className="play-die-artwork"
        data-used-fraction={used}
        mask={halfUsed ? `url(#${maskId})` : undefined}
      >
        {children}
      </g>
    </Tooltip>
  );
}
