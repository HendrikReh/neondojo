import type { FormEvent, ReactNode, RefObject } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { matchContext } from '../../shared/board/boardMath';
import PlayBoardDialog from '../../shared/board/PlayBoardDialog';
import type { Theme } from '../../shared/board/themes';
import { AppError } from '../../shared/i18n/i18n-core';
import type { AppSettings, EngineInfo } from '../../settings/settings-api';
import LearningReviewDialog from '../study/reports/LearningReviewDialog';
import type { PlaySession } from './play-api';
import { playHistoryView } from './PlayBoardStage';
import PlayCoachFeedback from './PlayCoachFeedback';
import {
  PlayCubeDialog,
  PlayFeedbackDialog,
  PlayForfeitDialog,
  PlayHintDialog,
  PlayPauseDialog,
  PlayResultDialog,
} from './PlayDialogs';
import PlayFeedback from './PlayFeedback';
import PlayHistoryReviewToolbar from './PlayHistoryReviewToolbar';
import PlayResignDialog from './PlayResignDialog';
import PlaySetupForm from './PlaySetupForm';
import PlayTurnToolbar from './PlayTurnToolbar';
import { usePlayText } from './play-messages';
import { usePlayModeText } from './play-mode-messages';
import { usePlayResignText } from './play-resign-messages';
import type { PlayDialogState } from './usePlayDialogs';
import type { usePlayReview } from './usePlayReview';
import type { PlaySetupChoices } from './usePlaySetupOptions';
import type { PlayTurnActions } from './usePlayTurnActions';

type Anchor = RefObject<HTMLDivElement | null>;
type Trigger = RefObject<HTMLButtonElement | null>;

/** Setup modal for starting a new match, kept outside the inert match layout. */
export function PlaySetupDialog({
  anchor,
  busy,
  uiBusy,
  focusOnOpen,
  returnFocus,
  choices,
  engines,
  errorNotice,
  settings,
  session,
  onClose,
  onSubmit,
  onResume,
  onOpenPausedMatches,
  onOpenPlaySettings,
}: {
  anchor: Anchor;
  busy: boolean;
  uiBusy: boolean;
  focusOnOpen: boolean;
  returnFocus: Trigger;
  choices: PlaySetupChoices;
  engines: EngineInfo[];
  errorNotice: ReactNode;
  settings?: AppSettings;
  session: PlaySession | null;
  onClose: () => void;
  onSubmit: (event: FormEvent) => void;
  onResume: () => void;
  onOpenPausedMatches: () => void;
  onOpenPlaySettings: () => void;
}) {
  const t = usePlayText();
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={t('newMatch')}
      busy={uiBusy}
      className="play-setup-dialog"
      modal={false}
      focusOnOpen={focusOnOpen}
      returnFocus={returnFocus}
      onClose={onClose}
    >
      <PlaySetupForm
        choices={choices}
        engines={engines}
        errorNotice={errorNotice}
        busy={busy}
        disabled={uiBusy}
        uiBusy={uiBusy}
        settings={settings}
        session={session}
        onSubmit={onSubmit}
        onResume={onResume}
        onOpenPausedMatches={onOpenPausedMatches}
        onOpenPlaySettings={onOpenPlaySettings}
      />
    </PlayBoardDialog>
  );
}

/** Toolbars and board-top dialogs rendered in PlayBoardStage's `before` slot. */
export function PlayBoardBeforeStack({
  session,
  anchor,
  theme,
  name,
  actions,
  dialogs,
  review,
  pauseTrigger,
  hintTrigger,
  errorNotice,
  decisionError,
  onRetrySession,
}: {
  session: PlaySession;
  anchor: Anchor;
  theme: Theme;
  name: (player: number) => string;
  actions: PlayTurnActions;
  dialogs: PlayDialogState;
  review: ReturnType<typeof usePlayReview>;
  pauseTrigger: Trigger;
  hintTrigger: Trigger;
  errorNotice: ReactNode;
  decisionError: ReactNode;
  onRetrySession: () => void;
}) {
  const modeT = usePlayModeText();
  const hintButton = (
    <ActionButton
      ref={hintTrigger}
      className="play-hint-button"
      disabled={actions.uiBusy}
      onClick={() => actions.feedback('hint')}
      icon="hint"
      label={modeT('hint')}
      shortcut="H"
    />
  );
  return (
    <>
      {dialogs.pauseWarning && (
        <PlayPauseDialog
          anchor={anchor}
          busy={actions.uiBusy}
          returnFocus={pauseTrigger}
          error={errorNotice}
          onClose={actions.closePauseWarning}
          onPause={actions.pauseMatch}
        />
      )}
      {review.index !== null && (
        <PlayHistoryReviewToolbar
          review={review}
          index={review.index}
          view={playHistoryView(review)}
          onRetry={onRetrySession}
        />
      )}
      {actions.coachDecision && (
        <PlayCoachFeedback
          key={`${session.id}:${actions.coachDecision.id}`}
          anchor={anchor}
          session={session}
          decision={actions.coachDecision}
          theme={theme}
          white={name(0)}
          black={name(1)}
          busy={actions.uiBusy}
          errorNotice={decisionError}
          onContinue={actions.continueDecision}
          onRetry={() => actions.feedback('retry')}
          onReveal={() => actions.feedback('reveal')}
        />
      )}
      {actions.showBoardActions && (
        <PlayTurnToolbar
          session={session}
          state={actions.toolbarState}
          labels={actions.toolbarLabels}
          hint={hintButton}
          callbacks={actions.toolbarCallbacks}
        />
      )}
    </>
  );
}

/** Post-game / post-match learning review dialog anchored over the board. */
export function PlayLearningReviewModal({
  anchor,
  returnFocus,
  session,
  theme,
  moneyFinished,
  onClose,
}: {
  anchor: Anchor;
  returnFocus: Trigger;
  session: PlaySession;
  theme: Theme;
  moneyFinished: boolean;
  onClose: () => void;
}) {
  const t = usePlayText();
  return (
    <LearningReviewDialog
      anchor={anchor}
      returnFocus={returnFocus}
      session={session}
      theme={theme}
      context={
        moneyFinished
          ? `${t('game')} ${session.review_game_no ?? session.game_no}`
          : undefined
      }
      onClose={onClose}
    />
  );
}

/** All in-match interruption and confirmation dialogs rendered over the live board. */
export default function PlayDialogStack({
  session,
  anchor,
  theme,
  name,
  actions,
  dialogs,
  reviewActive,
  busy,
  error,
  errorNotice,
  decisionError,
  pauseButton,
  assessmentButton,
  modeControl,
  triggers,
}: {
  session: PlaySession;
  anchor: Anchor;
  theme: Theme;
  name: (player: number) => string;
  actions: PlayTurnActions;
  dialogs: PlayDialogState;
  reviewActive: boolean;
  busy: boolean;
  error: unknown;
  errorNotice: ReactNode;
  decisionError: ReactNode;
  pauseButton: ReactNode;
  assessmentButton: ReactNode;
  modeControl: ReactNode;
  triggers: {
    forfeit: Trigger;
    menu: Trigger;
    moneyReview: Trigger;
  };
}) {
  const modeT = usePlayModeText();
  const resignT = usePlayResignText();
  const {
    interruption,
    uiBusy,
    pending,
    feedbackMode,
    awaitingFeedback,
    competition,
    canForfeit,
  } = actions;

  const resignationDeclined =
    error instanceof AppError &&
    String(error.params.status) === '422' &&
    [
      'GNUbg declined this resignation.',
      'The bot declined this resignation.',
    ].some((prefix) => error.diagnostic.startsWith(prefix));

  const forfeitButton = canForfeit && (
    <ActionButton
      type="button"
      icon="resign"
      ref={triggers.forfeit}
      label={resignT('forfeitMatch')}
      disabled={uiBusy || reviewActive}
      aria-haspopup="dialog"
      onClick={actions.openForfeit}
    />
  );

  const dialogPreferences = (
    <details className="play-dialog-preferences">
      <summary>
        {modeT('activeMode')}: {modeT(feedbackMode)}
      </summary>
      {modeControl}
    </details>
  );

  const feedbackPanel = (pending?.action || pending?.original_action) && (
    <PlayFeedback
      session={session}
      preview={
        interruption === 'feedback'
          ? { theme, white: name(0), black: name(1) }
          : undefined
      }
      busy={uiBusy}
      onReveal={() => actions.feedback('reveal')}
      onRetry={() => actions.feedback('retry')}
      onContinue={actions.continueDecision}
    />
  );

  return (
    <>
      {interruption === 'resign' && (
        <PlayResignDialog
          anchor={anchor}
          cube={matchContext(session.position.gnubg_match_id).cube}
          white={name(0)}
          black={name(1)}
          humanPlayer={session.human_player}
          scores={[session.scores[0], session.scores[1]]}
          matchLength={session.match_length}
          busy={uiBusy}
          error={
            resignationDeclined ? (
              <p role="alert" className="play-error">
                {resignT('declined')}
              </p>
            ) : (
              errorNotice
            )
          }
          onClose={actions.closeResign}
          onForfeit={
            canForfeit
              ? () => {
                  actions.closeResign();
                  dialogs.setForfeitOpen(true);
                }
              : undefined
          }
          onConfirm={(level) =>
            actions.action('resign', undefined, undefined, level)
          }
        />
      )}
      {interruption === 'hint' && (
        <PlayHintDialog
          anchor={anchor}
          session={session}
          theme={theme}
          name={name}
          busy={uiBusy}
          loading={busy}
          ready={dialogs.hintReady}
          error={errorNotice}
          onClose={dialogs.closeHint}
        />
      )}
      {interruption === 'cube' && (
        <PlayCubeDialog
          anchor={anchor}
          session={session}
          busy={uiBusy}
          error={decisionError}
          feedback={feedbackPanel}
          awaitingFeedback={awaitingFeedback}
          competition={competition}
          pause={pauseButton}
          forfeit={forfeitButton}
          preferences={dialogPreferences}
          onTake={() => actions.action('take')}
          onDrop={() => actions.action('drop')}
          onHint={() => actions.feedback('hint')}
        />
      )}
      {interruption === 'feedback' && (
        <PlayFeedbackDialog
          anchor={anchor}
          session={session}
          mode={feedbackMode}
          busy={uiBusy}
          error={decisionError}
          feedback={feedbackPanel}
          pause={pauseButton}
          forfeit={forfeitButton}
          preferences={dialogPreferences}
        />
      )}
      {(interruption === 'game_over' || interruption === 'match_over') && (
        <PlayResultDialog
          key={interruption}
          kind={interruption}
          anchor={anchor}
          session={session}
          name={name}
          busy={uiBusy}
          error={decisionError}
          failed={Boolean(error)}
          feedback={feedbackPanel}
          pause={pauseButton}
          forfeit={forfeitButton}
          assessment={assessmentButton}
          preferences={dialogPreferences}
          moneyReviewRef={triggers.moneyReview}
          onNextGame={() => actions.action('next_game')}
          onEndSession={() => actions.action('end_session')}
          onNewMatch={actions.acknowledgeAndNewMatch}
          onBack={actions.acknowledgeMatch}
          onReview={() => dialogs.setLearningReviewOpen(true)}
        />
      )}
      {dialogs.forfeitOpen && canForfeit && (
        <PlayForfeitDialog
          anchor={anchor}
          busy={uiBusy}
          returnFocus={interruption ? triggers.forfeit : triggers.menu}
          error={errorNotice}
          onClose={actions.closeForfeit}
          onForfeit={() => actions.action('forfeit_match')}
        />
      )}
    </>
  );
}
