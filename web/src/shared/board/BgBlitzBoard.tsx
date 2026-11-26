import CheckerPiece from './CheckerPiece';
import { cubeImage, type PieceSet } from './piece-artwork';
import type { Position } from '../http/api';
import { checkerCounts } from './boardMath';
import {
  pointColumns,
  stackGeometry,
  type BgBlitzTheme,
  type ThemeImage,
} from './themes';
import { useI18n } from '../i18n/i18n';
import MoveArrows from './MoveArrows';
import PlayBoardDice from './PlayBoardDice';
import PlayCube from './PlayCube';
import PlayCheckerMotion from './PlayCheckerMotion';
import { underlayBoard, type PlaybackFrame } from './play-animation';
import BoardPlayTargets, { type BoardInteraction } from './BoardPlayTargets';
import type { MovePreviewStep } from './move-preview';

export default function BgBlitzBoard({
  position,
  theme,
  numberOrientation,
  checkerSet,
  diceSet,
  cubeSet,
  label,
  cubeLabel,
  cube,
  onError,
  previewSteps,
  interaction,
  animation,
}: {
  position: Position;
  theme: BgBlitzTheme;
  numberOrientation: 'RLD' | 'RLU' | 'LRD' | 'LRU';
  checkerSet: PieceSet;
  diceSet: PieceSet;
  cubeSet: PieceSet;
  label: string;
  cubeLabel: string;
  cube: { value: number; owner: number | null; pending: boolean };
  onError: () => void;
  previewSteps?: MovePreviewStep[];
  interaction?: BoardInteraction;
  animation?: PlaybackFrame | null;
}) {
  const { formatNumber } = useI18n();
  const { width, height } = theme.background;
  const counts = checkerCounts(position.board);
  const moving = animation?.kind === 'checker' ? animation : undefined;
  const board = moving ? underlayBoard(moving) : position.board;
  const offWhite =
    15 - counts[0] - (moving?.motions.filter((m) => m.from === -1).length ?? 0);
  const offBlack =
    15 - counts[1] - (moving?.motions.filter((m) => m.from === 26).length ?? 0);
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
    const image = theme.checkers[count > 0 ? 'white' : 'black'][0];
    const geometry = stackGeometry(theme, image, column, count, bottom);
    if (!geometry.total) return null;
    return (
      <g
        key={point}
        data-point={point}
        data-count={count}
        data-player={count > 0 ? 'white' : 'black'}
      >
        {Array.from({ length: geometry.visible }, (_, i) => (
          <CheckerPiece
            key={i}
            x={geometry.x}
            y={geometry.y + (bottom ? -1 : 1) * i * geometry.step}
            width={geometry.width}
            height={geometry.height}
            set={checkerSet}
            white={count > 0}
            variant={column + i}
            onError={onError}
          />
        ))}
        {geometry.total > 5 && (
          <g
            transform={`translate(${geometry.x + geometry.width / 2},${geometry.y + (bottom ? -1 : 1) * 4 * geometry.step + geometry.height / 2})`}
          >
            <circle
              r={geometry.width * 0.2}
              fill="#fffbed"
              stroke="#172b22"
              strokeWidth="2"
            />
            <text
              y={geometry.width * 0.075}
              textAnchor="middle"
              fontSize={geometry.width * 0.23}
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
  const numbers = theme.numbers[numberOrientation];
  // Number strips have uneven transparent margins. Mirror their slot around
  // the outer point centers so the upright artwork stays aligned to the points.
  const outerPointCenterSum =
    theme.columns[1] +
    theme.columns[13] +
    2 * theme.checkerOffsetX +
    theme.checkers.white[0].width;
  const { value: displayedCube, owner: cubeOwner, pending: cubePending } = cube;
  const cubeSlot = cubeImage(theme, displayedCube, cubeOwner, cubePending);
  const cubeWidth = cubeSlot?.width ?? 64;
  const cubeHeight = cubeSlot?.height ?? 64;
  const barX = theme.columns[7] + theme.checkers.white[0].width / 2;
  const configuredCubeX = cubeOwner === 0 ? theme.cubeRightX : theme.cubeLeftX;
  // XML positions are image origins; only the centered/default cube is centered on the bar.
  const cubeX =
    cubeOwner === null || configuredCubeX === 0
      ? barX - cubeWidth / 2
      : configuredCubeX;
  const cubeY =
    height / 2 +
    (cubeOwner === null
      ? theme.cubeCenteredY
      : (cubeOwner === 0 ? -1 : 1) * theme.cubeOffsetY);
  const cubeTop = cubeY - cubeHeight / 2;
  return (
    <svg
      className="board bgblitz-board"
      viewBox={`0 0 ${width} ${height}`}
      role={interaction ? 'group' : 'img'}
      aria-label={label}
      data-theme={theme.id}
    >
      {artwork(theme.background, 0, 0)}
      <g data-number-orientation={numberOrientation}>
        {numbers.map((image, i) =>
          artwork(
            image,
            numberOrientation.startsWith('LR')
              ? outerPointCenterSum - theme.numbersX - image.width
              : theme.numbersX,
            theme.numbersY[i],
            i,
          ),
        )}
      </g>
      {pointColumns.map(({ point, column, bottom }) =>
        stack(board[point], column, bottom, point),
      )}
      {stack(board[0], 7, false, 'bar-black')}
      {stack(board[25], 7, true, 'bar-white')}
      {stack(-offBlack, 14, false, 'off-black')}
      {stack(offWhite, 14, true, 'off-white')}
      {interaction && (
        <BoardPlayTargets interaction={interaction} theme={theme} />
      )}
      <PlayCube
        label={cubeLabel}
        bounds={{
          x: cubeX,
          y: cubeTop,
          width: cubeWidth,
          height: cubeHeight,
        }}
        interaction={interaction?.cube}
        value={displayedCube}
        pending={cubePending}
        owner={cubeOwner}
        artwork={cubeSet}
        onError={onError}
      />
      <PlayBoardDice
        diceSet={diceSet}
        dice={position.dice}
        player={position.player}
        interaction={interaction?.dice}
        theme={theme}
        onError={onError}
      />
      {moving && (
        <PlayCheckerMotion
          key={animation?.id}
          frame={moving}
          theme={theme}
          checkerSet={checkerSet}
          onError={onError}
        />
      )}
      {previewSteps && (
        <MoveArrows
          board={position.board}
          player={position.player}
          steps={previewSteps}
          theme={theme}
        />
      )}
    </svg>
  );
}
