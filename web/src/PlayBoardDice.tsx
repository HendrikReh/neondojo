import PlayDice, { type DiceInteraction } from './PlayDice';
import type { BgBlitzTheme } from './themes';

/** Shared dice artwork and animation for ordinary turns and the opening roll. */
export default function PlayBoardDice({
  dice,
  player,
  interaction,
  theme,
  onError,
}: {
  dice: number[];
  player: number;
  interaction?: DiceInteraction;
  theme?: BgBlitzTheme;
  onError?: () => void;
}) {
  const opening = interaction?.opening;
  const entries = opening
    ? [0, 1].map((owner) => ({ owner, value: opening.values[owner] ?? null }))
    : interaction?.preRoll
      ? [0, 1].map(() => ({ owner: player, value: null }))
      : dice
          .filter((value) => value >= 1 && value <= 6)
          .map((value) => ({ owner: player, value }));
  return (
    <g
      data-dice={dice.join('-')}
      data-play-dice-container={interaction ? true : undefined}
      data-play-opening-dice={opening ? true : undefined}
    >
      {entries.map(({ owner, value }, index) => {
        const face = value ?? 1;
        const image = theme
          ? (owner === 0 ? theme.dice.white : theme.dice.black)[face - 1]
          : undefined;
        const width = image?.width ?? 33,
          height = image?.height ?? 33;
        const x = theme
          ? (owner === 0 ? theme.diceRightX : theme.diceLeftX) +
            (opening ? 0 : index * width * 1.25)
          : opening
            ? owner === 0
              ? 420
              : 185
            : 420 + index * 45;
        const y = theme ? (theme.background.height - height) / 2 : 219;
        const controls =
          opening && interaction
            ? {
                ...interaction,
                canSwap: false,
                canConfirm: false,
                canRoll: false,
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
                  : interaction?.preRoll
                    ? 'play-preroll-die'
                    : undefined
              }
              data-pre-roll-die={interaction?.preRoll ? true : undefined}
              data-opening-die={opening ? (value ?? 'pending') : undefined}
            >
              <PlayDice
                index={index}
                width={width}
                height={height}
                interaction={controls}
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
                  <>
                    <rect
                      width="33"
                      height="33"
                      rx="6"
                      className={
                        owner === 0 ? 'white-checker' : 'black-checker'
                      }
                    />
                    <text
                      x="16.5"
                      y="23"
                      className={`die-value ${owner === 0 ? 'on-white' : 'on-black'}`}
                    >
                      {value === null
                        ? '–'
                        : ['', '⚀', '⚁', '⚂', '⚃', '⚄', '⚅'][value]}
                    </text>
                  </>
                )}
              </PlayDice>
            </g>
          </g>
        );
      })}
    </g>
  );
}
