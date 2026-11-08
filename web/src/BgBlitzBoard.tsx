import type { Position } from './api';
import { checkerCounts, matchContext } from './boardMath';
import {
  pointColumns,
  stackGeometry,
  type BgBlitzTheme,
  type ThemeImage,
} from './themes';
import { useI18n } from './i18n';

export default function BgBlitzBoard({
  position,
  theme,
  label,
  cubeLabel,
  onError,
}: {
  position: Position;
  theme: BgBlitzTheme;
  label: string;
  cubeLabel: string;
  onError: () => void;
}) {
  const { formatNumber } = useI18n();
  const { width, height } = theme.background;
  const context = matchContext(position.gnubg_match_id);
  const counts = checkerCounts(position.board);
  function artwork(
    image: ThemeImage,
    x: number,
    y: number,
    key?: string | number,
  ) {
    return (
      <image
        key={key}
        href={image.src}
        x={x}
        y={y}
        width={image.width}
        height={image.height}
        onError={onError}
      />
    );
  }
  function stack(
    count: number,
    column: number,
    bottom: boolean,
    point: number | string,
  ) {
    const choices = count > 0 ? theme.checkers.white : theme.checkers.black;
    const image = choices[0];
    const geometry = stackGeometry(theme, image, column, count, bottom);
    if (!geometry.total) return null;
    return (
      <g
        key={point}
        data-point={point}
        data-count={count}
        data-player={count > 0 ? 'white' : 'black'}
      >
        {Array.from({ length: geometry.visible }, (_, i) =>
          artwork(
            choices[(i + column) % choices.length],
            geometry.x,
            geometry.y + (bottom ? -1 : 1) * i * geometry.step,
            i,
          ),
        )}
        {geometry.total > 5 && (
          <g
            transform={`translate(${geometry.x + image.width / 2},${geometry.y + (bottom ? -1 : 1) * 4 * geometry.step + image.height / 2})`}
          >
            <circle
              r={image.width * 0.2}
              fill="#fffbed"
              stroke="#172b22"
              strokeWidth="2"
            />
            <text
              y={image.width * 0.075}
              textAnchor="middle"
              fontSize={image.width * 0.23}
              fontWeight="700"
              fill="#172b22"
            >
              {formatNumber(geometry.total)}
            </text>
          </g>
        )}
      </g>
    );
  }
  const numbers = theme.numbers[position.player === 0 ? 'RLD' : 'RLU'];
  const cubeValue = context.cube === 1 ? 64 : context.cube;
  const cubeImage =
    theme.cubes[context.owner === null ? '64_r' : String(cubeValue)] ||
    theme.cubes[String(cubeValue)];
  const barX = theme.columns[7] + theme.checkers.white[0].width / 2;
  const configuredCubeX =
    context.owner === 0 ? theme.cubeRightX : theme.cubeLeftX;
  // XML positions are image origins; only the centered/default cube is centered on the bar.
  const cubeX =
    context.owner === null || configuredCubeX === 0
      ? barX - (cubeImage?.width ?? 64) / 2
      : configuredCubeX;
  const cubeY =
    height / 2 +
    (context.owner === null
      ? theme.cubeCenteredY
      : (context.owner === 0 ? -1 : 1) * theme.cubeOffsetY);
  const dice = position.player === 0 ? theme.dice.white : theme.dice.black;
  const diceX = position.player === 0 ? theme.diceRightX : theme.diceLeftX;
  return (
    <svg
      className="board bgblitz-board"
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={label}
      data-theme={theme.id}
    >
      {artwork(theme.background, 0, 0)}
      <g data-number-orientation={position.player === 0 ? 'RLD' : 'RLU'}>
        {numbers.map((image, i) =>
          artwork(image, theme.numbersX, theme.numbersY[i], i),
        )}
      </g>
      {pointColumns.map(({ point, column, bottom }) =>
        stack(position.board[point], column, bottom, point),
      )}
      {stack(position.board[0], 7, false, 'bar-black')}
      {stack(position.board[25], 7, true, 'bar-white')}
      {stack(-(15 - counts[1]), 14, false, 'off-black')}
      {stack(15 - counts[0], 14, true, 'off-white')}
      <g
        aria-label={cubeLabel}
        data-cube={context.cube}
        data-cube-owner={context.owner ?? 'centered'}
      >
        {cubeImage ? (
          artwork(cubeImage, cubeX, cubeY - cubeImage.height / 2)
        ) : (
          <g transform={`translate(${cubeX},${cubeY - 32})`}>
            <rect
              width="64"
              height="64"
              rx="8"
              fill="#fffbed"
              stroke="#172b22"
              strokeWidth="2"
            />
            <text
              x="32"
              y="42"
              textAnchor="middle"
              fontSize="23"
              fill="#172b22"
            >
              {cubeValue}
            </text>
          </g>
        )}
      </g>
      <g data-dice={position.dice.join('-')}>
        {position.dice
          .filter((value) => value >= 1 && value <= 6)
          .map((value, i) =>
            artwork(
              dice[value - 1],
              diceX + i * dice[value - 1].width * 1.25,
              (height - dice[value - 1].height) / 2,
              i,
            ),
          )}
      </g>
    </svg>
  );
}
