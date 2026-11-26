import type { CSSProperties } from 'react';
import BuiltInChecker from './BuiltInChecker';
import type { CheckerFrame } from './play-animation';
import { checkerCenter, pointStackGeometry, type BgBlitzTheme } from './themes';

export default function PlayCheckerMotion({
  frame,
  theme,
  builtInTheme,
}: {
  frame: CheckerFrame;
  builtInTheme?: string;
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
        const size = pointStackGeometry(
          motion.from,
          motion.player === 0 ? 1 : -1,
          theme,
        );
        const sourceCount = Math.abs(frame.before[motion.from] ?? 0);
        const topIndex = Math.max(0, Math.min(5, sourceCount) - 1);
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
                x={-size.width / 2}
                y={-size.height / 2}
                width={size.width}
                height={size.height}
              />
            ) : (
              <BuiltInChecker
                white={motion.player === 0}
                theme={builtInTheme}
                variant={Math.abs(motion.from) + topIndex}
              />
            )}
          </g>
        );
      })}
    </g>
  );
}
