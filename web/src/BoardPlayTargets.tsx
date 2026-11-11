import type { DiceInteraction } from './PlayDice';
import type { CubeInteraction } from './PlayCube';
import type { BgBlitzTheme } from './themes';
import { pointColumns, stackGeometry } from './themes';
import { relativePoint } from './play-state';
export interface BoardInteraction {
  enabled: number[];
  human: number;
  sources: number[];
  destinations: number[];
  board: number[];
  onHover: (point: number | null) => void;
  dice?: DiceInteraction;
  cube?: CubeInteraction;
  onPoint: (point: number) => void;
  label: (point: number) => string;
}
export default function BoardPlayTargets({
  interaction,
  theme,
}: {
  interaction: BoardInteraction;
  theme?: BgBlitzTheme;
}) {
  const width = theme?.background.width ?? 730,
    height = theme?.background.height ?? 470;
  const inset = theme
    ? stackGeometry(theme, theme.checkers.white[0], 1, 1, false).y
    : height * 0.08;
  const topY = Math.max(0, inset - 4),
    bottomY = height / 2 + 6,
    targetHeight = height / 2 - inset - 6;
  const rectangles = pointColumns.map(({ point, column, bottom }) => ({
    point,
    x: theme
      ? theme.columns[column] + theme.checkerOffsetX - 2
      : 40 +
        (column - 1 - (column > 7 ? 1 : 0)) * 51 +
        (column > 7 ? 28 : 0) -
        24,
    y: bottom ? bottomY : topY,
    width: theme ? theme.checkers.white[0].width + 4 : 48,
    height: targetHeight,
  }));
  const barX = theme ? theme.columns[7] + theme.checkerOffsetX - 2 : 325;
  const barWidth = theme ? theme.checkers.white[0].width + 4 : 30;
  for (const point of [0, 25])
    rectangles.push({
      point,
      x: barX,
      y: point === 0 ? topY : bottomY,
      width: barWidth,
      height: targetHeight,
    });
  const offX = theme ? theme.columns[14] + theme.checkerOffsetX - 2 : 686;
  for (const point of [-1, 26])
    rectangles.push({
      point,
      x: offX,
      y: point === 26 ? topY : bottomY,
      width: theme ? theme.checkers.white[0].width + 4 : 40,
      height: targetHeight,
    });
  return (
    <g className="board-play-targets">
      {rectangles.map(({ point, x, y, width: w, height: h }) => {
        const count = point >= 0 && point <= 25 ? interaction.board[point] : 0;
        const total = Math.min(5, Math.abs(count));
        const bottom =
          point === 25 || point === -1 || (point >= 1 && point <= 12);
        let checkerX = x,
          checkerY = y,
          checkerW = w,
          checkerH = 0;
        if (total && point >= 0 && point <= 25) {
          if (theme) {
            const column =
              point === 0 || point === 25
                ? 7
                : pointColumns.find((p) => p.point === point)!.column;
            const image =
              count > 0 ? theme.checkers.white[0] : theme.checkers.black[0];
            const geo = stackGeometry(theme, image, column, count, bottom);
            checkerX = geo.x;
            checkerY = geo.y - (bottom ? (geo.visible - 1) * geo.step : 0);
            checkerW = image.width;
            checkerH = image.height + (geo.visible - 1) * geo.step;
          } else {
            checkerW = 37.4;
            checkerX = x + (w - checkerW) / 2;
            const first = bottom
              ? point === 25
                ? 406
                : 409
              : point === 0
                ? 64
                : 61;
            checkerY = first - (bottom ? (total - 1) * 29 : 0) - 18.7;
            checkerH = 37.4 + (total - 1) * 29;
          }
        }
        return (
          <g
            key={point}
            role="button"
            aria-label={interaction.label(point)}
            aria-disabled={!interaction.enabled.includes(point)}
            tabIndex={interaction.enabled.includes(point) ? 0 : -1}
            data-play-point={point}
            data-enabled={interaction.enabled.includes(point)}
            onFocus={() =>
              interaction.onHover(
                interaction.sources.includes(point) ? point : null,
              )
            }
            onBlur={() => interaction.onHover(null)}
            onClick={() =>
              interaction.enabled.includes(point) && interaction.onPoint(point)
            }
            onKeyDown={(event) => {
              if (
                interaction.enabled.includes(point) &&
                ['Enter', ' '].includes(event.key)
              ) {
                event.preventDefault();
                interaction.onPoint(point);
              }
            }}
          >
            <rect
              x={x}
              y={y}
              width={w}
              height={h}
              rx="5"
              className="play-point"
            />
            {interaction.destinations.includes(point) && (
              <circle
                className="play-destination-dot"
                data-play-destination={point}
                cx={x + w / 2}
                cy={y + h * (bottom ? 0.45 : 0.55)}
                r={height * 0.009}
              />
            )}
            {interaction.sources.includes(point) && total > 0 && (
              <rect
                className="play-checker-hover"
                data-checker-hover-point={point}
                x={checkerX}
                y={checkerY}
                width={checkerW}
                height={checkerH}
                rx={checkerW / 2}
                aria-hidden="true"
                onPointerEnter={() => interaction.onHover(point)}
                onPointerLeave={() => interaction.onHover(null)}
              />
            )}
            {point >= 1 && point <= 24 && (
              <text
                className="play-point-number"
                x={x + w / 2}
                y={point >= 13 ? height * 0.055 : height * 0.97}
                textAnchor="middle"
                fontSize={height * 0.029}
              >
                {relativePoint(point, interaction.human)}
              </text>
            )}
            {!theme && (point === -1 || point === 26) && (
              <text
                className="play-point-number"
                x={x + w / 2}
                y={y + h / 2}
                textAnchor="middle"
                fontSize="12"
              >
                ↗
              </text>
            )}
          </g>
        );
      })}
      <desc>
        {width} × {height}
      </desc>
    </g>
  );
}
