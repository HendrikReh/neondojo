import { useEffect, useRef } from 'react';
import {
  playStrengthLabel,
  useSettingsText,
} from '../../settings/settings-messages';
import type { AppSettings } from '../../settings/settings-api';
import { stopPlaySounds } from '../../shared/board/play-audio';
import type { Theme } from '../../shared/board/themes';
import { formatPly } from '../../shared/format/ply';
import { useLatest } from '../../shared/hooks/useLatest';
import ActionButton from '../../shared/ui/ActionButton';
import { useLibraryText } from '../library/library-messages';
import { useMatchAssessmentText } from '../study/reports/match-assessment-messages';
import type { PlaySession } from './play-api';
import { usePlayText } from './play-messages';
import { usePlayModeText } from './play-mode-messages';
import { usePlayResignText } from './play-resign-messages';
import PlayBoardStage from './PlayBoardStage';
import PlayDialogStack, {
  PlayBoardBeforeStack,
  PlayLearningReviewModal,
  PlaySetupDialog,
} from './PlayDialogStack';
import PlayErrorNotice from './PlayErrorNotice';
import PlayHistory from './PlayHistory';
import PlayIdleBoard from './PlayIdleBoard';
import PlayMatchActions from './PlayMatchActions';
import PlayMatchSummary, { PlayPostGameStatus } from './PlayMatchSummary';
import PlayModeControl from './PlayModeControl';
import { usePlayAnimation } from './usePlayAnimation';
import { usePlayDialogFocus, usePlayDialogs } from './usePlayDialogs';
import { usePlayReview } from './usePlayReview';
import { usePlaySession, type PlaySessionRequest } from './usePlaySession';
import { usePlayMatchSetup } from './usePlaySetupOptions';
import { usePlayThreshold } from './usePlayThreshold';
import { usePlayTurnActions } from './usePlayTurnActions';
import { useSavedAssessment } from './useSavedAssessment';
import './play.css';

export default function PlayWorkspace({
  visible,
  settings,
  theme,
  requestedSession,
  setupRequest,
  onSaved,
  onModeChanged,
  onOpenPausedMatches,
  onOpenPlaySettings,
  onOpenAssessment,
}: {
  visible: boolean;
  settings?: AppSettings;
  theme: Theme;
  setupRequest?: number;
  requestedSession?: PlaySessionRequest;
  onSaved?: () => void;
  onModeChanged?: (session: PlaySession) => void;
  onOpenPausedMatches: () => void;
  onOpenPlaySettings: () => void;
  onOpenAssessment: (reportId: string) => void;
}) {
  const playback = usePlayAnimation(visible);
  const boardAnchor = useRef<HTMLDivElement>(null);
  const hintTrigger = useRef<HTMLButtonElement>(null);
  const learningReviewTrigger = useRef<HTMLButtonElement>(null);
  const moneyReviewTrigger = useRef<HTMLButtonElement>(null);
  const resumeTrigger = useRef<HTMLButtonElement>(null);
  const pauseTrigger = useRef<HTMLButtonElement>(null);
  const newMatchTrigger = useRef<HTMLButtonElement>(null);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const resignTrigger = useRef<HTMLButtonElement>(null);
  const forfeitTrigger = useRef<HTMLButtonElement>(null);
  const focusResume = useRef(false);
  const soundsVisible = useLatest(visible);
  useEffect(() => {
    if (!visible) stopPlaySounds();
    return () => stopPlaySounds();
  }, [visible]);

  const dialogs = usePlayDialogs(visible);
  const t = usePlayText();
  const modeT = usePlayModeText();
  const libraryT = useLibraryText();
  const resignT = usePlayResignText();
  const settingsT = useSettingsText();
  const assessmentT = useMatchAssessmentText();

  const playSession = usePlaySession({
    visible,
    requestedSession,
    onLoad: playback.cancel,
    onAccept: (accepted) => {
      dialogs.accept(accepted);
      review.reset();
      threshold.show(accepted.value);
      if (!accepted.preserveSteps) actions.turn.reset();
    },
    onSaved,
  });
  const { session, busy, error, setup, pendingRoll, load, run, retry } =
    playSession;

  const assessmentAction = assessmentT(
    useSavedAssessment(session, visible) ? 'open' : 'create',
  );
  const review = usePlayReview(session, visible && !setup);
  const matchSetup = usePlayMatchSetup({
    visible,
    settings,
    setupRequest,
    playSession,
    dialogs,
    animating: playback.animating,
  });
  const name = (player: number) =>
    player === session?.human_player
      ? session.human_name || t('you')
      : t('bot');

  const actions = usePlayTurnActions({
    visible,
    playSession,
    playback,
    review,
    dialogs,
    soundsVisible,
    onPaused: () => {
      focusResume.current = true;
    },
    onModeChanged,
    onOpenSetup: matchSetup.openSetup,
    name,
  });

  const threshold = usePlayThreshold({
    session,
    setup,
    isBlocked: actions.isBlocked,
    run,
  });

  useEffect(() => {
    if (visible && actions.paused && !busy && !setup && focusResume.current)
      resumeTrigger.current?.focus({ preventScroll: true });
    if (!visible || !busy) focusResume.current = false;
  }, [visible, actions.paused, busy, setup]);

  usePlayDialogFocus({
    dialogs,
    resignTrigger,
    hintTrigger,
    hidden: !visible || setup,
    busy: actions.uiBusy,
    interrupted: Boolean(actions.interruption),
  });

  const sessionStrength =
    session &&
    (session.engine.difficulty
      ? t(session.engine.difficulty)
      : session.engine.top_two
        ? playStrengthLabel(
            settingsT,
            session.engine.plies,
            session.engine.id,
            true,
          )
        : formatPly(session.engine.plies));

  const errorNotice =
    error === undefined ? null : (
      <PlayErrorNotice
        error={error}
        hint={dialogs.hintOpen}
        busy={actions.uiBusy}
        onHint={() => actions.feedback('hint')}
        onRetry={retry}
      />
    );
  const decisionError = dialogs.forfeitOpen ? null : errorNotice;
  const assessmentButton =
    session && session.match_length > 0 && session.phase === 'match_over' ? (
      <ActionButton
        icon="analysis"
        label={assessmentAction}
        disabled={actions.uiBusy || (actions.paused && !session.review)}
        onClick={() => actions.openAssessment(onOpenAssessment)}
      >
        {assessmentAction}
      </ActionButton>
    ) : undefined;
  // Resigning sits beside Pause match so it is found without opening the menu.
  const resignButton = session &&
    !actions.paused &&
    session.phase !== 'game_over' &&
    session.phase !== 'match_over' && (
      <ActionButton
        type="button"
        ref={resignTrigger}
        icon="resign"
        label={resignT('confirmGame')}
        disabled={actions.uiBusy || !actions.showBoardActions}
        aria-haspopup="dialog"
        onClick={actions.openResign}
      />
    );
  const pauseButton = session &&
    !actions.paused &&
    !session.money_session_ended &&
    (session.phase !== 'match_over' || actions.moneyFinished) && (
      <ActionButton
        type="button"
        ref={actions.interruption ? undefined : pauseTrigger}
        icon="pause"
        label={t('pauseMatch')}
        disabled={actions.uiBusy}
        aria-haspopup={actions.hasPauseDraft ? 'dialog' : undefined}
        onClick={actions.requestPause}
      />
    );
  const modeControl = session && (
    <PlayModeControl
      session={session}
      mode={actions.feedbackMode}
      threshold={threshold}
      disabled={
        actions.uiBusy ||
        actions.paused ||
        session.phase === 'match_over' ||
        actions.moneyFinished
      }
      onMode={actions.switchMode}
    />
  );

  return (
    <div className="play-workspace" aria-busy={actions.uiBusy}>
      <header className="play-header" inert={matchSetup.open}>
        <h1 className={session ? 'sr-only' : undefined}>{t('title')}</h1>
        {!session && !setup && (
          <div className="play-actions">
            <ActionButton
              ref={newMatchTrigger}
              className="primary"
              onClick={matchSetup.openSetup}
              disabled={actions.uiBusy}
              icon="new-game"
              label={t('newMatch')}
            >
              {t('newMatch')}
            </ActionButton>
            <ActionButton
              onClick={onOpenPausedMatches}
              disabled={actions.uiBusy}
              icon="open"
              label={libraryT('pausedMatches')}
            />
          </div>
        )}
      </header>
      {!matchSetup.open &&
        !actions.interruption &&
        !dialogs.pauseWarning &&
        !actions.coachDecision &&
        !dialogs.forfeitOpen &&
        errorNotice}
      {!session && (
        <PlayIdleBoard
          anchor={boardAnchor}
          human={matchSetup.choices.human}
          direction={
            matchSetup.choices.confirmed?.appearance.bearoff_direction ??
            settings?.bearoff_direction ??
            'right'
          }
          inert={matchSetup.open}
          theme={theme}
        />
      )}
      {matchSetup.open && (
        <PlaySetupDialog
          anchor={boardAnchor}
          busy={busy}
          uiBusy={actions.uiBusy}
          focusOnOpen={matchSetup.focusSetup}
          returnFocus={session ? menuTrigger : newMatchTrigger}
          choices={matchSetup.choices}
          engines={matchSetup.engines}
          errorNotice={errorNotice}
          settings={settings}
          session={session}
          onClose={matchSetup.closeSetup}
          onSubmit={matchSetup.create}
          onResume={actions.resumeMatch}
          onOpenPausedMatches={onOpenPausedMatches}
          onOpenPlaySettings={onOpenPlaySettings}
        />
      )}
      {session && (
        <>
          <div className="play-match-heading" inert={matchSetup.open}>
            <PlayMatchSummary
              session={session}
              name={name}
              strength={sessionStrength}
              status={actions.turnStatus}
            />
            <div
              className="play-mode-slot"
              aria-hidden={Boolean(actions.interruption)}
              inert={Boolean(actions.interruption)}
            >
              {modeControl}
            </div>
            <PlayMatchActions
              pause={pauseButton}
              resign={resignButton}
              interrupted={Boolean(actions.interruption)}
              paused={actions.paused}
              busy={actions.uiBusy}
              reviewing={review.active}
              resumeRef={resumeTrigger}
              menuRef={menuTrigger}
              canForfeit={actions.canForfeit}
              canStartMatch={actions.interruption !== 'match_over'}
              onResume={actions.resumeMatch}
              onForfeit={actions.openForfeit}
              onNewMatch={matchSetup.openSetup}
              onOpenPausedMatches={onOpenPausedMatches}
            />
          </div>
          <div className="play-layout" inert={matchSetup.open}>
            <PlayHistory
              history={session.history}
              gameResults={session.game_results}
              white={name(0)}
              black={name(1)}
              sessionId={session.id}
              analysisEngine={session.engine_snapshots?.analysis.engine_version}
              analysisPlies={session.analysis_plies}
              visible={visible}
              selectedIndex={review.index}
              disabled={actions.uiBusy || actions.awaitingFeedback}
              decisions={
                actions.competition &&
                session.phase !== 'match_over' &&
                !actions.moneyFinished
                  ? undefined
                  : session.decisions
              }
              onSelect={actions.selectHistory}
              reviewAction={
                session.phase === 'match_over' || actions.moneyFinished ? (
                  <>
                    {!actions.interruption && assessmentButton}
                    <ActionButton
                      ref={learningReviewTrigger}
                      tooltip={modeT('reviewTooltip')}
                      compact
                      disabled={
                        actions.uiBusy || (actions.paused && !session.review)
                      }
                      icon="analysis"
                      label={modeT('review')}
                      onClick={actions.openLearningReview}
                    >
                      {modeT('review')}
                    </ActionButton>
                  </>
                ) : undefined
              }
            />
            <section className="play-table">
              {visible &&
                !setup &&
                (!actions.interruption || actions.moneyResultDialog) &&
                dialogs.learningReviewOpen &&
                session.review && (
                  <PlayLearningReviewModal
                    anchor={boardAnchor}
                    returnFocus={
                      actions.moneyResultDialog
                        ? moneyReviewTrigger
                        : learningReviewTrigger
                    }
                    session={session}
                    theme={theme}
                    moneyFinished={actions.moneyFinished}
                    onClose={() => dialogs.setLearningReviewOpen(false)}
                  />
                )}
              <PlayBoardStage
                session={session}
                board={{
                  anchor: boardAnchor,
                  theme,
                  name,
                  paused: actions.paused,
                  frames: playback.frames,
                  pendingRoll,
                  openingPhase: actions.openingPhase,
                }}
                turn={actions.boardTurn}
                review={review}
                callbacks={actions.boardCallbacks}
                before={
                  <PlayBoardBeforeStack
                    session={session}
                    anchor={boardAnchor}
                    theme={theme}
                    name={name}
                    actions={actions}
                    dialogs={dialogs}
                    review={review}
                    pauseTrigger={pauseTrigger}
                    hintTrigger={hintTrigger}
                    errorNotice={errorNotice}
                    decisionError={decisionError}
                    onRetrySession={() => load(session.id)}
                  />
                }
              >
                <PlayDialogStack
                  session={session}
                  anchor={boardAnchor}
                  theme={theme}
                  name={name}
                  actions={actions}
                  dialogs={dialogs}
                  reviewActive={review.active}
                  busy={busy}
                  error={error}
                  errorNotice={errorNotice}
                  decisionError={decisionError}
                  pauseButton={pauseButton}
                  assessmentButton={assessmentButton}
                  modeControl={modeControl}
                  triggers={{
                    forfeit: forfeitTrigger,
                    menu: menuTrigger,
                    moneyReview: moneyReviewTrigger,
                  }}
                />
              </PlayBoardStage>
              {!busy &&
                !actions.paused &&
                !review.active &&
                !actions.interruption &&
                !actions.awaitingFeedback &&
                (session.phase === 'game_over' ||
                  session.phase === 'match_over') && (
                  <PlayPostGameStatus
                    session={session}
                    name={name}
                    busy={actions.uiBusy}
                    onNextGame={() => actions.action('next_game')}
                  />
                )}
            </section>
          </div>
        </>
      )}
    </div>
  );
}
