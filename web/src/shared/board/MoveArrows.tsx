import type { MovePreviewStep } from './move-preview';
import { checkerCenter, type BgBlitzTheme } from './themes';

interface Point {
  x: number;
  y: number;
}
interface ArrowGeometry {
  start: Point;
  end: Point;
}
export function moveArrowGeometry(
  original: number[],
  player: number,
  steps: MovePreviewStep[],
  theme?: BgBlitzTheme,
): ArrowGeometry[] {
  const board = [...original];
  const sign = player === 0 ? 1 : -1;
  const ownBar = player === 0 ? 25 : 0;
  const opponentBar = player === 0 ? 0 : 25;
  return steps.map((step) => {
    const from = step.from === 'bar' ? ownBar : step.from;
    const start = checkerCenter(board, from, player, theme);
    board[from] -= sign;
    if (step.to !== 'off') {
      if (step.hit) {
        board[step.to] = 0;
        board[opponentBar] -= sign;
      }
      board[step.to] += sign;
    }
    const target = step.to === 'off' ? (player === 0 ? -1 : 26) : step.to;
    const end = checkerCenter(board, target, player, theme);
    return { start, end };
  });
}

export default function MoveArrows({
  board,
  player,
  steps,
  theme,
}: {
  board: number[];
  player: number;
  steps: MovePreviewStep[];
  theme?: BgBlitzTheme;
}) {
  const width = theme?.background.width ?? 680;
  const height = theme?.background.height ?? 470;
  const scale = width / 680;
  const geometry = moveArrowGeometry(board, player, steps, theme);
  return (
    <g className="move-arrows" aria-hidden="true">
      {geometry.map(({ start, end }, index) => {
        const sameHalf = start.y < height / 2 === end.y < height / 2;
        const bend = (18 + (index % 3) * 8) * scale;
        const control = {
          x:
            (start.x + end.x) / 2 +
            (sameHalf ? 0 : (index % 2 ? 1 : -1) * bend),
          y:
            (start.y + end.y) / 2 +
            (sameHalf ? (start.y < height / 2 ? 1 : -1) * bend : 0),
        };
        const dx = end.x - control.x;
        const dy = end.y - control.y;
        const length = Math.hypot(dx, dy) || 1;
        const ux = dx / length;
        const uy = dy / length;
        const back = { x: end.x - ux * 15 * scale, y: end.y - uy * 15 * scale };
        const line = `M${start.x},${start.y} Q${control.x},${control.y} ${end.x},${end.y}`;
        const head = `M${back.x - uy * 7 * scale},${back.y + ux * 7 * scale} L${end.x},${end.y} L${back.x + uy * 7 * scale},${back.y - ux * 7 * scale} Z`;
        const step = steps[index];
        return (
          <g
            key={index}
            data-from={step.from}
            data-to={step.to}
            data-die={step.die}
            data-hit={step.hit}
          >
            <path
              d={line}
              className="move-arrow-line move-arrow-dark"
              strokeWidth={10 * scale}
            />
            <path
              d={line}
              className="move-arrow-line move-arrow-light"
              strokeWidth={7 * scale}
            />
            <path
              d={line}
              className="move-arrow-line move-arrow-green"
              strokeWidth={4 * scale}
            />
            <path
              d={head}
              className="move-arrow-head move-arrow-dark"
              strokeWidth={6 * scale}
            />
            <path
              d={head}
              className="move-arrow-head move-arrow-light"
              strokeWidth={3 * scale}
            />
            <path
              d={head}
              className="move-arrow-head move-arrow-green"
              strokeWidth={1 * scale}
            />
          </g>
        );
      })}
    </g>
  );
}
