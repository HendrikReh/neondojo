import type { PieceSet } from './piece-artwork';
import PlayDice, { type DiceInteraction } from './PlayDice';
import ThemedDie from './ThemedDie';
import Tooltip from '../ui/Tooltip';
import type { BgBlitzTheme } from './themes';

/** Shared dice artwork and animation for ordinary turns and the opening roll. */
export default function PlayBoardDice({
  dice,
  player,
  interaction,
  theme,
  diceSet,
  sideByOwner,
  showBothDiceColors,
  onError,
}: {
  dice: number[];
  player: number;
  interaction?: DiceInteraction;
  theme?: BgBlitzTheme;
  diceSet: PieceSet;
  sideByOwner?: boolean;
  showBothDiceColors?: boolean;
  onError?: () => void;
}) {
  const opening = interaction?.opening;
  const split = opening || showBothDiceColors;
  const action = interaction?.roll
    ? 'roll'
    : interaction?.pass
      ? 'pass'
      : interaction?.canConfirm && interaction.confirm
        ? 'confirm'
        : interaction?.canSwap
          ? 'swap'
          : undefined;
  const pairAction = !split && action && interaction?.[action];
  const visibleDice = interaction?.previousRoll?.dice ?? dice;
  const diceOwner = interaction?.previousRoll?.player ?? player;
  const entries = opening
    ? [0, 1].map((owner) => ({ owner, value: opening.values[owner] ?? null }))
    : visibleDice
        .filter((value) => value >= 1 && value <= 6)
        .map((value, index) => ({
          owner: showBothDiceColors ? index : diceOwner,
          value,
        }));
  const faces = entries.map(({ owner, value }, index) => {
    const face = value ?? 1;
    const side = owner === 0 ? 'white' : 'black';
    const slot = theme?.dice[side][face - 1];
    const image = diceSet.archive?.dice[side][face - 1];
    const width = slot?.width ?? 33;
    const height = slot?.height ?? 33;
    const left = owner === 1 && (theme || sideByOwner || split);
    const startX = left
      ? (theme?.diceLeftX ?? 185)
      : (theme?.diceRightX ?? 420);
    const x = startX + (split ? 0 : index * (theme ? width * 1.25 : 45));
    const y = theme ? (theme.background.height - height) / 2 : 219;
    return { owner, value, image, width, height, x, y };
  });
  const boardDice = {
    'data-dice': visibleDice.join('-'),
    'data-play-dice-container': interaction ? true : undefined,
    'data-play-opening-dice': opening ? true : undefined,
  };
  const artwork = faces.map(
    ({ owner, value, image, width, height, x, y }, index) => {
      const controls =
        opening && interaction
          ? {
              ...interaction,
              active: null,
              used: [false, false],
              label: `${opening.labels[owner]} · ${value ?? opening.unrolled}`,
            }
          : interaction;
      return (
        <g
          key={index}
          transform={`translate(${x},${y})`}
          data-die-owner={owner}
          data-opening-player={opening ? owner : undefined}
        >
          <g
            className={
              opening
                ? `play-opening-die-orientation ${value === null ? 'play-die-pending' : ''}`
                : undefined
            }
            data-opening-die={opening ? (value ?? 'pending') : undefined}
          >
            <PlayDice
              index={index}
              width={width}
              height={height}
              interaction={controls}
              partOfPair={Boolean(pairAction)}
            >
              {image ? (
                <image
                  href={image.src}
                  x="0"
                  y="0"
                  width={width}
                  height={height}
                  onError={onError}
                />
              ) : (
                <ThemedDie
                  theme={diceSet.id}
                  light={owner === 0}
                  value={value}
                  width={width}
                  height={height}
                />
              )}
            </PlayDice>
          </g>
        </g>
      );
    },
  );
  if (interaction && pairAction && faces.length) {
    const left = Math.min(...faces.map(({ x }) => x));
    const top = Math.min(...faces.map(({ y }) => y));
    const right = Math.max(...faces.map(({ x, width }) => x + width));
    const bottom = Math.max(...faces.map(({ y, height }) => y + height));
    const hitHeight = bottom - top + 8;
    // Keep the board flip centered on the hit area while the artwork moves.
    const transformOrigin = `${(left + right) / 2}px ${top + hitHeight / 2}px`;
    return (
      <Tooltip
        as="g"
        {...boardDice}
        text={interaction.label}
        aria-label={interaction.label}
        aria-describedby={interaction.descriptionId}
        role="button"
        tabIndex={0}
        style={{ transformBox: 'view-box', transformOrigin }}
        data-dice-action={action}
        onClick={pairAction}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            pairAction();
          }
        }}
      >
        <rect
          className="play-die-hit"
          x={left}
          y={top}
          width={right - left}
          height={hitHeight}
          aria-hidden="true"
        />
        {artwork}
      </Tooltip>
    );
  }
  return <g {...boardDice}>{artwork}</g>;
}
