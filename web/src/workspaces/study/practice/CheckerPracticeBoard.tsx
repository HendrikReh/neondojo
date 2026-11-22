import { useMemo, useState, type ReactNode } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import Board from '../../../shared/board/Board';
import type { Position } from '../../../shared/api/api';
import { useI18n } from '../../../shared/i18n/i18n';
import { usePlayText } from '../../play/play-messages';
import { usePlayModeText } from '../../play/play-mode-messages';
import {
  applySteps,
  matchingMoves,
  nextPoints,
  relativePoint,
  type CheckerStep,
} from '../../../shared/board/play-state';
import type { Theme } from '../../../shared/board/themes';
import type { useCheckerTurns } from './useCheckerTurns';

export interface CheckerDraft {
  steps: CheckerStep[];
  firstDie?: number;
}

export default function CheckerPracticeBoard({
  position,
  checkerTurns,
  players,
  theme,
  boardClassName,
  draft,
  onChange,
  busy,
  checkLabel,
  onCheck,
  children,
}: {
  position: Position;
  checkerTurns: ReturnType<typeof useCheckerTurns>;
  players: readonly [string, string];
  theme: Theme;
  boardClassName?: string;
  draft: CheckerDraft;
  onChange: (draft: CheckerDraft) => void;
  busy: boolean;
  checkLabel: string;
  onCheck: (steps: CheckerStep[]) => void;
  children?: ReactNode;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  const { formatError } = useI18n();
  const [hovered, setHovered] = useState<number | null>(null);
  const { player } = position;
  const { steps } = draft;
  const { turns, error, loading, retry } = checkerTurns;
  const firstDie = draft.firstDie ?? turns?.[0]?.dice[0];
  const dice =
    firstDie === position.dice[1]
      ? [...position.dice].reverse()
      : position.dice;
  const activeDie = dice[0];
  const moves = useMemo(
    () => turns?.filter((turn) => turn.dice[0] === activeDie) ?? [],
    [turns, activeDie],
  );
  const sources = busy ? [] : nextPoints(moves, steps, null);
  const canSwap =
    !busy &&
    !steps.length &&
    dice[0] !== dice[1] &&
    turns?.some((turn) => turn.dice[0] === dice[1]) === true;
  const forcedPass = turns?.length === 0;
  const submitLabel = forcedPass ? t('pass') : checkLabel;
  const canCheck =
    forcedPass ||
    matchingMoves(moves, steps).some(
      (turn) => turn.steps.length === steps.length,
    );
  const board = applySteps(position.board, steps, player);

  function moveChecker(from: number) {
    if (!sources.includes(from)) return;
    const [to] = nextPoints(moves, steps, from);
    if (to !== undefined)
      onChange({ ...draft, steps: [...steps, { from, to }] });
  }

  function swapDice() {
    if (!canSwap) return;
    setHovered(null);
    onChange({ ...draft, firstDie: dice[1] });
  }

  return (
    <>
      <div className={['play-board', boardClassName].filter(Boolean).join(' ')}>
        <Board
          position={{ ...position, board, dice, notation: '' }}
          white={players[0]}
          black={players[1]}
          theme={theme}
          interaction={{
            enabled: sources,
            sources,
            destinations:
              hovered !== null && sources.includes(hovered)
                ? nextPoints(moves, steps, hovered)
                : [],
            board,
            human: player,
            onHover: setHovered,
            onPoint: moveChecker,
            label: (point) =>
              point === -1 || point === 26
                ? t('off')
                : point === 0 || point === 25
                  ? t('bar')
                  : `${t('point')} ${relativePoint(point, player)}`,
            dice: {
              canSwap,
              swap: swapDice,
              label: `${t('swapDice')} · ${dice.join(', ')}`,
              used:
                dice[0] === dice[1]
                  ? [
                      Math.min(1, steps.length / 2),
                      Math.max(0, (steps.length - 2) / 2),
                    ]
                  : [steps.length >= 1, steps.length >= 2],
              active:
                busy || !turns || canCheck
                  ? null
                  : dice[0] === dice[1]
                    ? steps.length < 2
                      ? 0
                      : 1
                    : steps.length,
            },
          }}
        />
      </div>
      {loading && <p role="status">{t('checkerTurnsLoading')}</p>}
      {error !== undefined && (
        <div role="alert">
          <p>
            {t('checkerTurnsError')} {formatError(error)}
          </p>
          <ActionButton
            type="button"
            icon="refresh"
            label={modeT('retry')}
            onClick={retry}
          >
            {modeT('retry')}
          </ActionButton>
        </div>
      )}
      <div className="play-actions">
        <ActionButton
          type="button"
          icon="undo"
          label={t('undo')}
          disabled={busy || !steps.length}
          onClick={() => {
            setHovered(null);
            onChange({ ...draft, steps: steps.slice(0, -1) });
          }}
        >
          {t('undo')}
        </ActionButton>
        <ActionButton
          type="button"
          icon="confirm"
          label={submitLabel}
          shortcut="Enter"
          disabled={busy || !canCheck}
          onClick={() => onCheck(steps)}
        >
          {submitLabel}
        </ActionButton>
        {children}
      </div>
    </>
  );
}
