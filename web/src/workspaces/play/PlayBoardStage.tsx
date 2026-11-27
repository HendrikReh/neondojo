import { useMemo, type ReactNode, type RefObject } from 'react';
import Board from '../../shared/board/Board';
import { projectMove, type MovePreview } from '../../shared/board/move-preview';
import {
  applySteps,
  relativePoint,
  type CheckerStep,
} from '../../shared/board/play-state';
import type { Theme } from '../../shared/board/themes';
import type { PlayAction, PlaySession } from './play-api';
import { isMoneyGameFinished, playFeedback } from './play-interruption';
import { usePlayText } from './play-messages';
import { usePlayFrame, useRollingFaces } from './usePlayAnimation';
import type { PlayFrames } from './usePlayAnimation';
import type { usePlayReview } from './usePlayReview';
import type { PlayTurn } from './usePlayTurn';

type Review = ReturnType<typeof usePlayReview>;

export interface PlayBoardContext {
  anchor: RefObject<HTMLDivElement | null>;
  theme: Theme;
  name: (player: number) => string;
  paused: boolean;
  frames: PlayFrames;
  pendingRoll: PlayAction | null;
  openingPhase: boolean;
}

export interface PlayBoardTurnState {
  draft: PlayTurn;
  /** The unconfirmed steps or those of the decision that waits for feedback. */
  selectedSteps: CheckerStep[];
  canMove: boolean;
  canConfirm: boolean;
  canPass: boolean;
  /** Coach feedback on a pass waits to be continued. */
  canContinuePass: boolean;
  rollReady: boolean;
  showActions: boolean;
  doubleReasonId: string;
  doubleUnavailableReason: string;
  turnNotationId: string;
}

export interface PlayBoardCallbacks {
  onRoll: () => void;
  onDouble: () => void;
  onConfirm: () => void;
  onPass: () => void;
  onContinue: () => void;
}

export interface PlayBoardStageProps {
  session: PlaySession;
  board: PlayBoardContext;
  turn: PlayBoardTurnState;
  review: Review;
  callbacks: PlayBoardCallbacks;
  /** Toolbars above the board. */
  before: ReactNode;
  /** Dialogs placed over the board. */
  children: ReactNode;
}

/** Only checker moves have arrows; other turns show their result. */
export const playHistoryView = ({ entry, view }: Review) =>
  entry?.kind !== 'checker' && view === 'arrows' ? 'after' : view;

/** The bot's dice, which stay on the board until the player rolls. */
function lastBotRoll(session: PlaySession) {
  if (session.phase !== 'roll') return undefined;
  const lastResult = session.game_results?.find(
    (game) => game.game_no === session.game_no - 1,
  );
  if (session.game_no > 1 && !lastResult) return undefined;
  const first = lastResult ? lastResult.history_index + 1 : 0;
  for (let index = session.history.length - 1; index >= first; --index) {
    const turn = session.history[index];
    if (turn.player !== session.human_player && turn.dice.length === 2)
      return turn;
  }
}

/**
 * The live board with its animation. It renders on every frame of the rolling
 * dice and the moving checkers, so the workspace around it does not.
 */
export default function PlayBoardStage({
  session,
  board,
  turn,
  review,
  callbacks,
  before,
  children,
}: PlayBoardStageProps) {
  const { anchor, theme, name, paused, frames, pendingRoll, openingPhase } =
    board;
  const {
    draft,
    selectedSteps,
    canMove,
    canConfirm,
    canPass,
    canContinuePass,
    rollReady,
    showActions,
    doubleReasonId,
    doubleUnavailableReason,
    turnNotationId,
  } = turn;
  const { onRoll, onDouble, onConfirm, onPass, onContinue } = callbacks;
  const t = usePlayText();
  const frame = usePlayFrame(frames);
  const waitingFaces = useRollingFaces(pendingRoll !== null);
  const { competition, mode, pending, routineRoll } = playFeedback(session);
  const openingFrame = frame?.kind === 'dice' && frame.opening ? frame : null;
  const botRoll = useMemo(() => lastBotRoll(session), [session]);
  const previousRoll =
    !review.active && !frame && !pendingRoll ? botRoll : undefined;
  const dice =
    frame && !openingFrame
      ? frame.dice
      : pendingRoll === 'roll'
        ? waitingFaces
        : openingPhase
          ? []
          : draft.dice;
  const historical = review.entry;
  const historyView = playHistoryView(review);
  const historicalPosition = historical
    ? historical.kind === 'checker' || historyView === 'before'
      ? historical.before
      : historical.after
    : null;
  const historicalProjection = useMemo<MovePreview | undefined>(() => {
    if (!historical || historical.kind !== 'checker') return undefined;
    const projected = projectMove(historical.before);
    return {
      ok: true,
      board: historical.after.board,
      steps: projected.ok ? projected.steps : [],
      forced: projected.ok && projected.forced,
    };
  }, [historical]);
  const liveBoard = applySteps(
    session.position.board,
    selectedSteps,
    session.human_player,
  );
  const displayedBoard = historical
    ? historyView === 'after'
      ? historical.after.board
      : historical.before.board
    : liveBoard;
  const mayDouble = session.can_double && !routineRoll;
  return (
    <div
      className="play-board play-live-board"
      data-board-actions={showActions}
      data-play-phase={paused ? undefined : session.phase}
      ref={anchor}
      data-play-review={review.index ?? undefined}
      data-play-animation={frame?.kind ?? (pendingRoll ? 'dice' : 'idle')}
      data-animation-player={frame?.player}
      data-animation-dice={frame?.dice.join('-')}
      data-direction={session.bearoff_direction}
      data-human-player={session.human_player}
    >
      {before}
      <Board
        playOrientation={{
          human: session.human_player,
          direction: session.bearoff_direction ?? 'right',
        }}
        position={
          historicalPosition ?? {
            ...session.position,
            dice,
            player: frame?.player ?? session.position.player,
            board: frame?.before ?? liveBoard,
          }
        }
        white={name(0)}
        black={name(1)}
        showPipCounts={
          !competition ||
          session.phase === 'match_over' ||
          isMoneyGameFinished(session)
        }
        theme={theme}
        animation={review.active ? null : frame}
        preview={historicalProjection}
        previewMode={historical ? historyView : undefined}
        interaction={{
          enabled: draft.sources,
          sources: draft.sources,
          destinations: draft.destinations,
          board: displayedBoard,
          onHover: draft.hover,
          cube: rollReady
            ? {
                activate: mayDouble ? onDouble : undefined,
                label: mayDouble ? t('double') : doubleUnavailableReason,
                descriptionId: session.can_double ? undefined : doubleReasonId,
              }
            : undefined,
          dice: {
            previousRoll,
            roll: rollReady && previousRoll ? onRoll : undefined,
            opening: openingPhase
              ? {
                  values:
                    openingFrame?.dice ??
                    (pendingRoll === 'opening_roll'
                      ? waitingFaces
                      : session.phase === 'opening_result' && session.opening
                        ? [session.opening.white_die, session.opening.black_die]
                        : [null, null]),
                  labels: [
                    name(0) + ' · ' + t('white'),
                    name(1) + ' · ' + t('black'),
                  ],
                  unrolled: t('openingNotRolled'),
                }
              : undefined,
            canSwap: draft.canSwap,
            canConfirm,
            confirm: onConfirm,
            pass: canContinuePass ? onContinue : canPass ? onPass : undefined,
            descriptionId:
              canConfirm && draft.steps.length > 0 ? turnNotationId : undefined,
            rolling: Boolean(
              frame?.rolling ||
              pendingRoll === 'roll' ||
              pendingRoll === 'opening_roll',
            ),
            swap: draft.swap,
            label:
              t(
                previousRoll
                  ? 'roll'
                  : pendingRoll
                    ? 'rolling'
                    : draft.forcedPass ||
                        (mode === 'coach' && pending?.action === 'pass')
                      ? 'pass'
                      : canConfirm
                        ? 'confirm'
                        : 'swapDice',
              ) +
              ' · ' +
              (previousRoll?.dice ?? dice).join(', '),
            used:
              review.active || frame || pendingRoll
                ? [false, false]
                : previousRoll
                  ? [true, true]
                  : draft.usedDice,
            active:
              frame || pendingRoll || !canMove || draft.completed
                ? null
                : draft.usedDice.findIndex((used) => Number(used) < 1),
          },
          onPoint: draft.point,
          label: (p) =>
            p === -1 || p === 26
              ? t('off') + ' · ' + name(p === -1 ? 0 : 1)
              : p === 0 || p === 25
                ? t('bar') + ' · ' + name(p === 25 ? 0 : 1)
                : t('point') + ' ' + relativePoint(p, session.human_player),
        }}
      />
      {children}
    </div>
  );
}
