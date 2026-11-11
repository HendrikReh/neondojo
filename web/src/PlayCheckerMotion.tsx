import type { CSSProperties } from 'react';
import { checkerCounts } from './boardMath';
import type { CheckerFrame } from './play-animation';
import { pointColumns, stackGeometry, type BgBlitzTheme } from './themes';

export function checkerCenter(
  board: number[],
  point: number,
  player: number,
  theme?: BgBlitzTheme,
) {
  const off = point === -1 || point === 26;
  const count = off
    ? 15 - checkerCounts(board)[player]
    : Math.abs(board[point]);
  const bottom = point === 25 || point === -1 || (point >= 1 && point <= 12);
  const column = off
    ? 14
    : point === 0 || point === 25
      ? 7
      : pointColumns.find((p) => p.point === point)!.column;
  if (theme) {
    const image = (
      player === 0 ? theme.checkers.white : theme.checkers.black
    )[0];
    const geometry = stackGeometry(
      theme,
      image,
      column,
      Math.max(1, count),
      bottom,
    );
    return {
      x: geometry.x + image.width / 2,
      y:
        geometry.y +
        (bottom ? -1 : 1) * (geometry.visible - 1) * geometry.step +
        image.height / 2,
    };
  }
  const x = off
    ? 706
    : column === 7
      ? 340
      : 40 + (column - 1 - (column > 7 ? 1 : 0)) * 51 + (column > 7 ? 28 : 0);
  const first = point === 0 ? 64 : point === 25 ? 406 : bottom ? 409 : 61;
  return {
    x,
    y: first + (bottom ? -1 : 1) * (Math.min(5, Math.max(1, count)) - 1) * 29,
  };
}
export default function PlayCheckerMotion({
  frame,
  theme,
}: {
  frame: CheckerFrame;
  theme?: BgBlitzTheme;
}) {
  return (
    <g className="play-checker-motion" aria-hidden="true">
      {frame.motions.map((motion, index) => {
        const start = checkerCenter(
          frame.before,
          motion.from,
          motion.player,
          theme,
        );
        const end = checkerCenter(frame.after, motion.to, motion.player, theme);
        const image = theme
          ? (motion.player === 0
              ? theme.checkers.white
              : theme.checkers.black)[0]
          : undefined;
        return (
          <g
            key={index}
            data-animated-checker={motion.player}
            data-from={motion.from}
            data-to={motion.to}
            className="play-moving-checker"
            style={
              {
                '--move-from-x': `${start.x}px`,
                '--move-from-y': `${start.y}px`,
                '--move-to-x': `${end.x}px`,
                '--move-to-y': `${end.y}px`,
              } as CSSProperties
            }
          >
            {image ? (
              <image
                href={image.src}
                x={-image.width / 2}
                y={-image.height / 2}
                width={image.width}
                height={image.height}
              />
            ) : (
              <>
                <circle
                  r="18.7"
                  className={
                    motion.player === 0 ? 'white-checker' : 'black-checker'
                  }
                />
                <circle r="13.5" className="checker-ring" />
              </>
            )}
          </g>
        );
      })}
    </g>
  );
}
