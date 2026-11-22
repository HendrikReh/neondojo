import type { Position } from '../http/api';
import { checkerCounts, pipCounts, matchContext } from './boardMath';

import { useEffect, useState } from 'react';
import { useI18n } from '../i18n/i18n';
import { useBoardReviewText } from './board-review-messages';
import BgBlitzBoard from './BgBlitzBoard';
import PlayBoardDice from './PlayBoardDice';
import PlayCube from './PlayCube';
import PlayCheckerMotion from './PlayCheckerMotion';
import { underlayBoard, type PlaybackFrame } from './play-animation';
import BoardPlayTargets, { type BoardInteraction } from './BoardPlayTargets';
import { archiveThemes, type Theme } from './themes';
import MoveArrows from './MoveArrows';
import type { BoardView, MovePreview } from './move-preview';
import { displayedBoardPosition } from './displayed-board';
import './board-preview.css';
export { themes, type Theme } from './themes';

function Checkers({
  count,
  x,
  y,
  bottom = false,
  point,
}: {
  count: number;
  x: number;
  y: number;
  bottom?: boolean;
  point?: number;
}) {
  const n = Math.abs(count);
  return (
    <g data-point={point} data-count={count}>
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
  preview,
  previewMode,
  previewLabel,
  interaction,
  animation,
  showPipCounts = true,
  cubeOfferPending = false,
}: {
  position: Position;
  white: string;
  black: string;
  theme: Theme;
  preview?: MovePreview;
  previewMode?: BoardView;
  previewLabel?: string;
  interaction?: BoardInteraction;
  animation?: PlaybackFrame | null;
  showPipCounts?: boolean;
  /** A response exercise may carry an older Match ID without the offer bit. */
  cubeOfferPending?: boolean;
}) {
  const { t, formatNumber, formatPoints } = useI18n();
  const review = useBoardReviewText();
  const [failedTheme, setFailedTheme] = useState('');
  useEffect(() => setFailedTheme(''), [theme]);
  const archive = archiveThemes.find((entry) => entry.id === theme);
  const custom = failedTheme === theme ? undefined : archive;
  const effectiveMode = preview?.ok ? (previewMode ?? 'before') : 'before';
  const displayedPosition = displayedBoardPosition(
    position,
    preview,
    effectiveMode,
  );
  const moving = animation?.kind === 'checker' ? animation : undefined;
  const board = moving ? underlayBoard(moving) : displayedPosition.board;
  const pips = pipCounts(displayedPosition.board);
  const counts = checkerCounts(displayedPosition.board);
  const context = matchContext(position.gnubg_match_id);
  const cubePending = context.doubled || cubeOfferPending;
  const displayedCube = context.cube * (cubePending ? 2 : 1);
  const cubeOwner = cubePending ? null : context.owner;
  const active = position.player === 0 ? white : black;
  const x = (index: number) => 40 + index * 51 + (index >= 6 ? 28 : 0);
  const cubeLabel = t('board.cube', {
    value: displayedCube,
    owner:
      cubeOwner === null
        ? t('board.centered')
        : cubeOwner === 0
          ? white
          : black,
  });
  const hasDice =
    displayedPosition.dice.length === 2 &&
    displayedPosition.dice.every((die) => die >= 1 && die <= 6);
  const openingDice = interaction?.dice?.opening;
  const stateLabel = !showPipCounts
    ? `${t('app.toMove', { player: active })}. ${hasDice ? displayedPosition.dice.join(' · ') : ''}. ${cubeLabel}`
    : openingDice
      ? openingDice.values
          .map(
            (value, index) =>
              `${openingDice.labels[index]} · ${value ?? openingDice.unrolled}`,
          )
          .join(' · ')
      : hasDice
        ? t('board.label', {
            player: active,
            first: displayedPosition.dice[0],
            second: displayedPosition.dice[1],
            white: pips[0],
            black: pips[1],
          })
        : effectiveMode === 'after'
          ? `${review('pipsLabel', { white, black, whitePips: formatNumber(pips[0]), blackPips: formatNumber(pips[1]) })}. ${cubeLabel}`
          : `${t('app.toMove', { player: active })}. ${cubeLabel}`;
  const label = previewLabel ? `${previewLabel}. ${stateLabel}` : stateLabel;
  return (
    <div
      className={`board-shell theme-${custom ? 'bgblitz' : archive ? 'forest' : theme}`}
      data-preview-mode={effectiveMode}
    >
      <div className="player-bar">
        <span>
          {custom ? (
            <img
              className="theme-player-checker"
              src={custom.checkers.black[0].src}
              alt=""
              onError={() => setFailedTheme(theme)}
            />
          ) : (
            <i className="player-dot black" />
          )}
          {black} <small>· {formatPoints(context.scores[1])}</small>
        </span>
        {showPipCounts && (
          <span>
            {t('board.pipsOff', {
              pips: pips[1],
              off: formatNumber(15 - counts[1]),
            })}
          </span>
        )}
      </div>
      {failedTheme === theme && (
        <p className="theme-load-error" role="alert">
          {t('board.assetError')}
        </p>
      )}
      {custom ? (
        <BgBlitzBoard
          position={displayedPosition}
          theme={custom}
          label={label}
          cubeLabel={cubeLabel}
          cube={{
            value: displayedCube,
            owner: cubeOwner,
            pending: cubePending,
          }}
          onError={() => setFailedTheme(theme)}
          interaction={interaction}
          animation={animation}
          previewSteps={
            effectiveMode === 'arrows' && preview?.ok
              ? preview.steps
              : undefined
          }
        />
      ) : (
        <svg
          className="board"
          viewBox={interaction ? '0 0 730 470' : '0 0 680 470'}
          role={interaction ? 'group' : 'img'}
          aria-label={label}
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
                <Checkers count={board[top]} x={x(i)} y={61} point={top} />
                <path
                  d={`M${x(i) - 24} 432 L${x(i) + 24} 432 L${x(i)} 256 Z`}
                  className={i % 2 ? 'point-dark' : 'point-light'}
                />
                <Checkers
                  count={board[bottom]}
                  x={x(i)}
                  y={409}
                  bottom
                  point={bottom}
                />
                <text x={x(i)} y="455" className="point-number">
                  {position.player === 0 ? bottom : 25 - bottom}
                </text>
              </g>
            );
          })}
          <Checkers count={board[0]} x={340} y={64} point={0} />
          <Checkers count={board[25]} x={340} y={406} bottom point={25} />
          {interaction && <BoardPlayTargets interaction={interaction} />}
          <PlayCube
            transform={`translate(328,${cubeOwner === null ? 223 : cubeOwner === 0 ? 365 : 81})`}
            bounds={{ x: 0, y: 0, width: 24, height: 24 }}
            label={cubeLabel}
            interaction={interaction?.cube}
            value={displayedCube}
            owner={cubeOwner}
            pending={cubePending}
          >
            <rect
              width="24"
              height="24"
              rx="3"
              fill="#fff8e8"
              stroke="#a2ac92"
            />
            <text
              x="12"
              y="17"
              textAnchor="middle"
              fontSize="13"
              fill="#3b5944"
              fontWeight="600"
            >
              {displayedCube === 1 ? '64' : displayedCube}
            </text>
          </PlayCube>
          <PlayBoardDice
            dice={displayedPosition.dice}
            player={position.player}
            interaction={interaction?.dice}
          />
          {moving && <PlayCheckerMotion key={animation?.id} frame={moving} />}
          {effectiveMode === 'arrows' && preview?.ok && (
            <MoveArrows
              board={position.board}
              player={position.player}
              steps={preview.steps}
            />
          )}
        </svg>
      )}
      <div className="player-bar">
        <span>
          {custom ? (
            <img
              className="theme-player-checker"
              src={custom.checkers.white[0].src}
              alt=""
              onError={() => setFailedTheme(theme)}
            />
          ) : (
            <i className="player-dot white" />
          )}
          {white}{' '}
          <small>
            · {formatPoints(context.scores[0])}
            {context.crawford ? ' · Crawford' : ''}
          </small>
        </span>
        {showPipCounts && (
          <span>
            {t('board.pipsOff', {
              pips: pips[0],
              off: formatNumber(15 - counts[0]),
            })}
          </span>
        )}
      </div>
    </div>
  );
}
