import type { ReactNode, RefObject } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import type { Theme } from '../../shared/board/themes';
import PlayBoardDialog from '../../shared/board/PlayBoardDialog';
import { matchContext } from '../../shared/board/boardMath';
import { useLibraryText } from '../library/library-messages';
import type { PlaySession } from './play-api';
import GameResultSummary from './GameResultSummary';
import PlayCubeDecision from './PlayCubeDecision';
import PlayHintView from './PlayHintView';
import { hasPlayMovePreview } from './PlayMovePreview';
import { isMoneyGameFinished } from './play-interruption';
import { usePlayText } from './play-messages';
import { usePlayModeText, type PlayMode } from './play-mode-messages';
import { usePlayResignText } from './play-resign-messages';

type Anchor = RefObject<HTMLDivElement | null>;
type Trigger = RefObject<HTMLButtonElement | null>;

/** Asks before pausing discards an unconfirmed turn. */
export function PlayPauseDialog({
  anchor,
  busy,
  returnFocus,
  error,
  onClose,
  onPause,
}: {
  anchor: Anchor;
  busy: boolean;
  returnFocus: Trigger;
  error: ReactNode;
  onClose: () => void;
  onPause: () => void;
}) {
  const t = usePlayText();
  const libraryT = useLibraryText();
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={t('pauseMatch')}
      busy={busy}
      returnFocus={returnFocus}
      onClose={onClose}
    >
      <p>{t('pauseDraftHelp')}</p>
      {error}
      <div className="play-actions">
        <ActionButton
          type="button"
          icon="close"
          label={libraryT('cancel')}
          disabled={busy}
          onClick={onClose}
        />
        <ActionButton
          type="button"
          className="primary"
          icon="pause"
          label={t('pauseMatch')}
          disabled={busy}
          onClick={onPause}
        />
      </div>
    </PlayBoardDialog>
  );
}

export function PlayForfeitDialog({
  anchor,
  busy,
  returnFocus,
  error,
  onClose,
  onForfeit,
}: {
  anchor: Anchor;
  busy: boolean;
  returnFocus: Trigger;
  error: ReactNode;
  onClose: () => void;
  onForfeit: () => void;
}) {
  const resignT = usePlayResignText();
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={resignT('forfeitMatch')}
      busy={busy}
      returnFocus={returnFocus}
      onClose={onClose}
    >
      <p>{resignT('forfeitHelp')}</p>
      {error}
      <div className="play-actions play-cube-choices">
        <ActionButton
          type="button"
          icon="cancel"
          label={resignT('cancel')}
          disabled={busy}
          onClick={onClose}
        />
        <ActionButton
          type="button"
          icon="resign"
          label={resignT('forfeitMatch')}
          className="play-resign-confirm"
          disabled={busy}
          onClick={onForfeit}
        />
      </div>
    </PlayBoardDialog>
  );
}

export function PlayHintDialog({
  anchor,
  session,
  theme,
  name,
  busy,
  loading,
  ready,
  error,
  onClose,
}: {
  anchor: Anchor;
  session: PlaySession;
  theme: Theme;
  name: (player: number) => string;
  busy: boolean;
  loading: boolean;
  ready: boolean;
  error: ReactNode;
  onClose: () => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={modeT('hint')}
      busy={busy}
      className={`play-hint-dialog play-analysis-dialog${ready && hasPlayMovePreview(session) ? ' play-analysis-dialog--checker' : ''}`}
      onClose={onClose}
    >
      {error}
      {loading && <p role="status">{t('hintLoading')}</p>}
      {!loading && ready && (
        <PlayHintView
          key={`${session.id}:${session.pending_decision?.id}`}
          session={session}
          theme={theme}
          white={name(0)}
          black={name(1)}
        />
      )}
    </PlayBoardDialog>
  );
}

/** The bot doubled: take or drop, or the feedback on that choice. */
export function PlayCubeDialog({
  anchor,
  session,
  busy,
  error,
  feedback,
  awaitingFeedback,
  competition,
  pause,
  forfeit,
  preferences,
  onTake,
  onDrop,
  onHint,
}: {
  anchor: Anchor;
  session: PlaySession;
  busy: boolean;
  error: ReactNode;
  feedback: ReactNode;
  awaitingFeedback: boolean;
  competition: boolean;
  pause: ReactNode;
  forfeit: ReactNode;
  preferences: ReactNode;
  onTake: () => void;
  onDrop: () => void;
  onHint: () => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  return (
    <PlayCubeDecision
      anchor={anchor}
      cube={matchContext(session.position.gnubg_match_id).cube}
      busy={busy}
    >
      {error}
      {feedback}
      {!awaitingFeedback && (
        <div className="play-actions play-cube-choices">
          <ActionButton
            className="primary"
            disabled={busy}
            onClick={onTake}
            icon="take"
            label={t('take')}
          >
            {t('take')}
          </ActionButton>
          <ActionButton
            disabled={busy}
            onClick={onDrop}
            icon="drop"
            label={t('drop')}
          >
            {t('drop')}
          </ActionButton>
          {!competition && (
            <ActionButton
              disabled={busy}
              onClick={onHint}
              icon="hint"
              label={modeT('hint')}
            >
              {modeT('hint')}
            </ActionButton>
          )}
        </div>
      )}
      <div className="play-actions play-cube-choices">
        {pause}
        {forfeit}
      </div>
      {preferences}
    </PlayCubeDecision>
  );
}

/** Feedback on a decision that waits for an answer. */
export function PlayFeedbackDialog({
  anchor,
  session,
  mode,
  busy,
  error,
  feedback,
  pause,
  forfeit,
  preferences,
}: {
  anchor: Anchor;
  session: PlaySession;
  mode: PlayMode;
  busy: boolean;
  error: ReactNode;
  feedback: ReactNode;
  pause: ReactNode;
  forfeit: ReactNode;
  preferences: ReactNode;
}) {
  const modeT = usePlayModeText();
  return (
    <PlayBoardDialog
      className={
        hasPlayMovePreview(session)
          ? 'play-analysis-dialog play-analysis-dialog--checker'
          : undefined
      }
      anchor={anchor}
      busy={busy}
      heading={modeT(mode)}
    >
      {error}
      {feedback}
      <div className="play-actions play-cube-choices">
        {pause}
        {forfeit}
      </div>
      {preferences}
    </PlayBoardDialog>
  );
}

/** The result of a game or a match and how to go on. */
export function PlayResultDialog({
  kind,
  anchor,
  session,
  name,
  busy,
  error,
  failed,
  feedback,
  pause,
  forfeit,
  assessment,
  preferences,
  moneyReviewRef,
  onNextGame,
  onEndSession,
  onNewMatch,
  onBack,
  onReview,
}: {
  kind: 'game_over' | 'match_over';
  anchor: Anchor;
  session: PlaySession;
  name: (player: number) => string;
  busy: boolean;
  error: ReactNode;
  /** A failed request; the next game may start without its money review. */
  failed: boolean;
  feedback: ReactNode;
  pause: ReactNode;
  forfeit: ReactNode;
  assessment: ReactNode;
  preferences: ReactNode;
  moneyReviewRef: Trigger;
  onNextGame: () => void;
  onEndSession: () => void;
  onNewMatch: () => void;
  onBack: () => void;
  onReview: () => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  const resignT = usePlayResignText();
  const moneyFinished = isMoneyGameFinished(session);
  const result = session.game_results?.find(
    (entry) => entry.game_no === session.game_no,
  )?.result;
  return (
    <PlayBoardDialog
      className={
        moneyFinished && kind === 'game_over'
          ? 'play-money-result-dialog'
          : undefined
      }
      anchor={anchor}
      busy={busy}
      heading={
        kind === 'match_over' && session.money_session_ended
          ? t('sessionEnded')
          : t(kind)
      }
    >
      {error}
      {feedback}
      {session.match_forfeited ? (
        <p>{resignT('matchForfeited')}</p>
      ) : result ? (
        <GameResultSummary result={result} white={name(0)} black={name(1)} />
      ) : (
        session.winner != null && (
          <p>
            {t('winner')}: {name(session.winner)}
          </p>
        )
      )}
      <div className="play-actions play-cube-choices">
        {kind === 'game_over' ? (
          <>
            <ActionButton
              className="primary"
              disabled={busy || (moneyFinished && !session.review && !failed)}
              onClick={onNextGame}
              icon="new-game"
              label={t('next_game')}
            >
              {t('next_game')}
            </ActionButton>
            {pause}
            {moneyFinished && (
              <ActionButton
                disabled={busy}
                onClick={onEndSession}
                icon="confirm"
                label={t('endSession')}
              >
                {t('endSession')}
              </ActionButton>
            )}
            {forfeit}
          </>
        ) : (
          <>
            <ActionButton
              className="primary"
              disabled={busy}
              onClick={onNewMatch}
              icon="new-game"
              label={t('newMatch')}
            >
              {t('newMatch')}
            </ActionButton>
            <ActionButton
              disabled={busy}
              onClick={onBack}
              icon="back"
              label={t('backToBoard')}
            >
              {t('backToBoard')}
            </ActionButton>
          </>
        )}
        {assessment}
        {moneyFinished && (
          <ActionButton
            ref={moneyReviewRef}
            icon="analysis"
            label={modeT('reviewTooltip')}
            disabled={busy || !session.review}
            onClick={onReview}
          >
            {modeT('reviewTooltip')}
          </ActionButton>
        )}
      </div>
      {preferences}
    </PlayBoardDialog>
  );
}
