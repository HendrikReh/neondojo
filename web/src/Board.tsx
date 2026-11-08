import type { Position } from './api';
import { checkerCounts, pipCounts, matchContext } from './boardMath';

export const themes = [
  { id: 'forest', name: 'Forest' },
  { id: 'walnut', name: 'Walnut' },
  { id: 'slate', name: 'Slate' },
] as const;
export type Theme = (typeof themes)[number]['id'];

function Checkers({
  count,
  x,
  y,
  bottom = false,
}: {
  count: number;
  x: number;
  y: number;
  bottom?: boolean;
}) {
  const n = Math.abs(count);
  return (
    <g>
      {Array.from({ length: Math.min(n, 5) }, (_, index) => (
        <g key={index}>
          <circle
            cx={x}
            cy={y + (bottom ? -1 : 1) * index * 29}
            r="18.7"
            className={count > 0 ? 'white-checker' : 'black-checker'}
          />
          <circle
            cx={x}
            cy={y + (bottom ? -1 : 1) * index * 29}
            r="13.5"
            className="checker-ring"
          />
          {index === 4 && n > 5 && (
            <text
              className={`checker-count ${count > 0 ? 'on-white' : 'on-black'}`}
              x={x}
              y={y + (bottom ? -1 : 1) * index * 29 + 5}
            >
              {n}
            </text>
          )}
        </g>
      ))}
    </g>
  );
}

export default function Board({
  position,
  white,
  black,
  theme,
}: {
  position: Position;
  white: string;
  black: string;
  theme: Theme;
}) {
  const board = position.board;
  const pips = pipCounts(board);
  const counts = checkerCounts(board);
  const context = matchContext(position.gnubg_match_id);
  const active = position.player === 0 ? white : black;
  const x = (index: number) => 40 + index * 51 + (index >= 6 ? 28 : 0);
  return (
    <div className={`board-shell theme-${theme}`}>
      <div className="player-bar">
        <span>
          <i className="player-dot black" />
          {black} <small>· {context.scores[1]} Punkte</small>
        </span>
        <span>
          {pips[1]} Pips <b>·</b> {15 - counts[1]} aus
        </span>
      </div>
      <svg
        className="board"
        viewBox="0 0 680 470"
        role="img"
        aria-label={`Backgammon-Position. ${active} am Zug, Würfel ${position.dice.join(' und ')}. Weiß ${pips[0]} Pips, Schwarz ${pips[1]} Pips.`}
      >
        <rect
          x="1"
          y="1"
          width="678"
          height="468"
          rx="14"
          className="board-frame"
        />
        <rect
          x="16"
          y="37"
          width="648"
          height="396"
          rx="3"
          className="board-felt"
        />
        <rect x="325" y="37" width="30" height="396" className="board-bar" />
        {Array.from({ length: 12 }, (_, i) => {
          const top = 13 + i,
            bottom = 12 - i;
          return (
            <g key={i}>
              <text x={x(i)} y="25" className="point-number">
                {position.player === 0 ? top : 25 - top}
              </text>
              <path
                d={`M${x(i) - 24} 38 L${x(i) + 24} 38 L${x(i)} 214 Z`}
                className={i % 2 ? 'point-light' : 'point-dark'}
              />
              <Checkers count={board[top]} x={x(i)} y={61} />
              <path
                d={`M${x(i) - 24} 432 L${x(i) + 24} 432 L${x(i)} 256 Z`}
                className={i % 2 ? 'point-dark' : 'point-light'}
              />
              <Checkers count={board[bottom]} x={x(i)} y={409} bottom />
              <text x={x(i)} y="455" className="point-number">
                {position.player === 0 ? bottom : 25 - bottom}
              </text>
            </g>
          );
        })}
        <Checkers count={board[0]} x={340} y={64} />
        <Checkers count={board[25]} x={340} y={406} bottom />
        <g
          transform={`translate(328,${context.owner === null ? 223 : context.owner === 0 ? 365 : 81})`}
          aria-label={`Doppler ${context.cube}, ${context.owner === null ? 'zentriert' : context.owner === 0 ? white : black}`}
        >
          <rect width="24" height="24" rx="3" fill="#fff8e8" stroke="#a2ac92" />
          <text
            x="12"
            y="17"
            textAnchor="middle"
            fontSize="13"
            fill="#3b5944"
            fontWeight="600"
          >
            {context.cube === 1 ? '64' : context.cube}
          </text>
        </g>
        {position.dice.map((die, i) => (
          <g key={i} transform={`translate(${420 + i * 45},219)`}>
            <rect
              width="33"
              height="33"
              rx="6"
              className={
                position.player === 0 ? 'white-checker' : 'black-checker'
              }
            />
            <text
              x="16.5"
              y="23"
              className={`die-value ${position.player === 0 ? 'on-white' : 'on-black'}`}
            >
              {['', '⚀', '⚁', '⚂', '⚃', '⚄', '⚅'][die]}
            </text>
          </g>
        ))}
      </svg>
      <div className="player-bar">
        <span>
          <i className="player-dot white" />
          {white}{' '}
          <small>
            · {context.scores[0]} Punkte{context.crawford ? ' · Crawford' : ''}
          </small>
        </span>
        <span>
          {pips[0]} Pips <b>·</b> {15 - counts[0]} aus
        </span>
      </div>
    </div>
  );
}
