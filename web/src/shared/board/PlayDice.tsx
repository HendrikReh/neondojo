import Tooltip from '../ui/Tooltip';
import { useId, type ReactNode } from 'react';
export interface DiceInteraction {
  canSwap: boolean;
  canConfirm?: boolean;
  previousRoll?: { player: number; dice: number[] };
  roll?: () => void;
  confirm?: () => void;
  pass?: () => void;
  swap: () => void;
  label: string;
  descriptionId?: string;
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
  partOfPair,
  children,
}: {
  index: number;
  width: number;
  height: number;
  interaction?: DiceInteraction;
  partOfPair: boolean;
  children: ReactNode;
}) {
  const maskId = useId();
  if (!interaction) return <>{children}</>;
  const used = Number(interaction.used[index] ?? 0);
  const halfUsed = used > 0 && used < 1;
  const presentation = {
    className: `play-die ${interaction.rolling ? 'play-die-rolling' : ''}`,
    'data-rolling': Boolean(interaction.rolling),
    'data-play-die-index': index,
    'data-used': used >= 1,
    'data-used-fraction': used,
  };
  const artwork = (
    <>
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
    </>
  );
  if (partOfPair)
    return (
      <g {...presentation} aria-hidden="true">
        {artwork}
      </g>
    );

  return (
    <Tooltip
      as="g"
      {...presentation}
      text={interaction.label}
      role="button"
      tabIndex={-1}
      aria-label={interaction.label}
      aria-disabled="true"
      data-dice-action="none"
    >
      <rect
        className="play-die-hit"
        x="0"
        y="0"
        width={width}
        height={height + 8}
      />
      {artwork}
    </Tooltip>
  );
}
