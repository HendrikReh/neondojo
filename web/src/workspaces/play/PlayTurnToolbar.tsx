import type { ReactNode } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import {
  relativePoint,
  type CheckerStepWithDie,
} from '../../shared/board/play-state';
import type { PlaySession } from './play-api';
import { usePlayText } from './play-messages';

/** The human's turn actions above the board: roll, pass or confirm, double, hint and undo. */
export default function PlayTurnToolbar({
  session,
  heading,
  beforeRoll,
  forcedPass,
  rollReady,
  canPass,
  canConfirm,
  canMove,
  pendingRoutineRoll,
  competition,
  hint,
  busy,
  steps,
  complete,
  missingCheckerPaths,
  doubleReasonId,
  doubleUnavailableReason,
  notationId,
  onRoll,
  onPass,
  onConfirm,
  onDouble,
  onUndo,
  onRetry,
}: {
  session: PlaySession;
  heading: 'yourTurn' | 'pass' | 'move';
  beforeRoll: boolean;
  forcedPass: boolean;
  rollReady: boolean;
  canPass: boolean;
  canConfirm: boolean;
  canMove: boolean;
  pendingRoutineRoll: boolean;
  competition: boolean;
  hint: ReactNode;
  busy: boolean;
  steps: CheckerStepWithDie[];
  complete: boolean;
  missingCheckerPaths: boolean;
  doubleReasonId: string;
  doubleUnavailableReason: ReactNode;
  notationId: string;
  onRoll: () => void;
  onPass: () => void;
  onConfirm: () => void;
  onDouble: () => void;
  onUndo: () => void;
  onRetry: () => void;
}) {
  const t = usePlayText();
  // Tells the player why only some checkers respond when the rules leave one play.
  const onlyMove =
    canMove && !beforeRoll && !forcedPass && session.legal_moves.length === 1
      ? session.legal_moves[0].notation
      : null;
  return (
    <div
      className="play-board-toolbar"
      data-testid="play-board-toolbar"
      role="group"
      aria-label={t(heading)}
    >
      <h3>{t(heading)}</h3>
      {/* The dice pair stays a second way to trigger the same action. */}
      {beforeRoll ? (
        <ActionButton
          className="primary play-main-action"
          data-play-action="roll"
          disabled={!rollReady}
          onClick={onRoll}
          icon="roll"
          label={t('roll')}
          shortcut="Enter"
        />
      ) : forcedPass ? (
        <ActionButton
          className="primary play-main-action"
          data-play-action="pass"
          disabled={!canPass}
          onClick={onPass}
          icon="pass"
          label={t('passTurn')}
          shortcut="Enter"
        />
      ) : (
        <ActionButton
          className="primary play-main-action"
          data-play-action="confirm"
          disabled={!canConfirm}
          onClick={onConfirm}
          icon="confirm"
          label={t('confirm')}
          shortcut="Enter"
        />
      )}
      {beforeRoll && !pendingRoutineRoll && (
        <ActionButton
          className="play-double-button"
          data-play-action="double"
          disabled={!rollReady || !session.can_double}
          aria-describedby={session.can_double ? undefined : doubleReasonId}
          onClick={onDouble}
          icon="double"
          label={t('double')}
          shortcut="D"
        />
      )}
      {beforeRoll && !session.can_double && (
        <p id={doubleReasonId} className="play-double-unavailable">
          {doubleUnavailableReason}
        </p>
      )}
      {!competition &&
        !pendingRoutineRoll &&
        !forcedPass &&
        (!beforeRoll || session.can_double) &&
        hint}
      {!beforeRoll && !forcedPass && (
        <ActionButton
          type="button"
          className="play-undo-button"
          disabled={busy || !canMove || !steps.length}
          onClick={onUndo}
          icon="undo"
          label={t('undo')}
          shortcut="Z"
        >
          <span>{t('undo')}</span>
          {steps.length > 0 && (
            <span className="play-undo-count" aria-hidden="true">
              {steps.length}
            </span>
          )}
        </ActionButton>
      )}
      {missingCheckerPaths && (
        <div role="alert" className="play-error">
          <p>{t('checkerTurnsError')}</p>
          <ActionButton icon="refresh" label={t('retry')} onClick={onRetry}>
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {steps.length > 0 && (
        <p id={notationId} className="play-selected-steps">
          {steps
            .map(
              (step) =>
                `${step.from === 0 || step.from === 25 ? t('bar') : relativePoint(step.from, session.human_player)}/${step.to < 0 || step.to > 25 ? t('off') : relativePoint(step.to, session.human_player)}`,
            )
            .join(' ')}
        </p>
      )}
      {!beforeRoll && (
        <p className="play-turn-help">
          {forcedPass
            ? t('passReady')
            : complete
              ? t('ready')
              : onlyMove
                ? t('onlyMove', { move: onlyMove })
                : t('select')}
        </p>
      )}
    </div>
  );
}
