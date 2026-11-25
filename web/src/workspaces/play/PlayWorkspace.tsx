import {
  playStrengthLabel,
  useSettingsText,
} from '../../settings/settings-messages';
import ActionButton from '../../shared/ui/ActionButton';
import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { useLatest, useLatestCallback } from '../../shared/hooks/useLatest';
import PlayHistory from './PlayHistory';
import PlayFeedback from './PlayFeedback';
import PlayCoachFeedback from './PlayCoachFeedback';
import LearningReviewDialog from '../study/reports/LearningReviewDialog';
import { matchAssessmentReportId } from '../study/reports/match-assessment-api';
import { useMatchAssessmentText } from '../study/reports/match-assessment-messages';
import { usePlayModeText, type PlayMode } from './play-mode-messages';
import type { AppSettings } from '../../settings/settings-api';
import { usePlayReview } from './usePlayReview';
import type { Theme } from '../../shared/board/themes';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import { usePlayText } from './play-messages';
import { useLibraryText } from '../library/library-messages';
import {
  actInPlaySession,
  createPlaySession,
  updatePlayFeedback,
  reviewPlaySession,
  changePlayMode,
  type PlayAction,
  type PlaySession,
} from './play-api';
import {
  playSounds,
  stopPlaySounds,
  unlockPlayAudio,
} from '../../shared/board/play-audio';
import { isNewCubeOffer } from '../../shared/board/play-sound-events';
import { matchContext } from '../../shared/board/boardMath';
import PlayBoardDialog from '../../shared/board/PlayBoardDialog';
import PlayBoardStage, { playHistoryView } from './PlayBoardStage';
import {
  PlayCubeDialog,
  PlayFeedbackDialog,
  PlayForfeitDialog,
  PlayHintDialog,
  PlayPauseDialog,
  PlayResultDialog,
} from './PlayDialogs';
import PlayResignDialog, { type ResignLevel } from './PlayResignDialog';
import { usePlayResignText } from './play-resign-messages';
import { actionFrames } from '../../shared/board/play-animation';
import {
  isMoneyGameFinished,
  playFeedback,
  playInterruption,
} from './play-interruption';
import { usePlayAnimation } from './usePlayAnimation';
import { usePlayDialogFocus, usePlayDialogs } from './usePlayDialogs';
import { usePlaySession } from './usePlaySession';
import { usePlayThreshold } from './usePlayThreshold';
import { usePlayTurn } from './usePlayTurn';
import { useSavedAssessment } from './useSavedAssessment';
import PlayErrorNotice from './PlayErrorNotice';
import PlayMatchActions from './PlayMatchActions';
import PlayModeControl from './PlayModeControl';
import PlaySetupForm from './PlaySetupForm';
import PlayIdleBoard from './PlayIdleBoard';
import { usePlayEngines, usePlaySetupOptions } from './usePlaySetupOptions';
import PlayMatchSummary from './PlayMatchSummary';
import PlayHistoryReviewToolbar from './PlayHistoryReviewToolbar';
import PlayTurnToolbar from './PlayTurnToolbar';
import './play.css';
export default function PlayWorkspace({
  visible,
  settings,
  theme,
  requestedSession,
  onSaved,
  onModeChanged,
  onOpenPausedMatches,
  onOpenPlaySettings,
  onOpenAssessment,
}: {
  visible: boolean;
  settings?: AppSettings;
  theme: Theme;
  requestedSession?: { id: string; token: number; resume?: boolean };
  onSaved?: () => void;
  onModeChanged?: (session: PlaySession) => void;
  onOpenPausedMatches: () => void;
  onOpenPlaySettings: () => void;
  onOpenAssessment: (reportId: string) => void;
}) {
  const assessmentT = useMatchAssessmentText();
  const playback = usePlayAnimation(visible);
  const boardAnchor = useRef<HTMLDivElement>(null);
  const doubleReasonId = useId();
  const turnNotationId = useId();
  const hintTrigger = useRef<HTMLButtonElement>(null);
  const learningReviewTrigger = useRef<HTMLButtonElement>(null);
  const moneyReviewTrigger = useRef<HTMLButtonElement>(null);
  const resumeTrigger = useRef<HTMLButtonElement>(null);
  const pauseTrigger = useRef<HTMLButtonElement>(null);
  const focusResume = useRef(false);
  const newMatchTrigger = useRef<HTMLButtonElement>(null);
  // Only a setup the user opened takes the focus; an automatic one leaves it.
  const [focusSetup, setFocusSetup] = useState(false);
  const autoMoneyReviews = useRef(new Set<string>());
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const resignTrigger = useRef<HTMLButtonElement>(null);
  const forfeitTrigger = useRef<HTMLButtonElement>(null);
  const [acknowledgedMatch, setAcknowledgedMatch] = useState<string | null>(
    null,
  );
  const soundsVisible = useLatest(visible);
  useEffect(() => {
    if (!visible) stopPlaySounds();
    return () => stopPlaySounds();
  }, [visible]);
  const dialogs = usePlayDialogs(visible);
  const t = usePlayText(),
    { t: appT } = useI18n();
  const modeT = usePlayModeText();
  const libraryT = useLibraryText();
  const resignT = usePlayResignText();
  const settingsT = useSettingsText();
  const {
    session,
    busy,
    error,
    setError,
    setup,
    setSetup,
    pendingRoll,
    setPendingRoll,
    load,
    run,
    retry,
    isLocked,
  } = usePlaySession({
    visible,
    requestedSession,
    onLoad: playback.cancel,
    onAccept: (accepted) => {
      dialogs.accept(accepted);
      review.reset();
      threshold.show(accepted.value);
      if (!accepted.preserveSteps) turn.reset();
    },
    onSaved,
  });
  const assessmentAction = assessmentT(
    useSavedAssessment(session, visible) ? 'open' : 'create',
  );
  const paused = Boolean(session?.paused);
  const moneyFinished = isMoneyGameFinished(session);
  const review = usePlayReview(session, visible && !setup);
  useEffect(() => {
    if (visible && paused && !busy && !setup && focusResume.current)
      resumeTrigger.current?.focus({ preventScroll: true });
    if (!visible || !busy) focusResume.current = false;
  }, [visible, paused, busy, setup]);
  const setupChoices = usePlaySetupOptions(settings);
  const {
    options: setupOptions,
    setOptions: setSetupOptions,
    confirmed,
    human,
    length,
    mode,
    automatic,
    botEngine,
    botDifficulty,
    botPlies,
    botTopTwo,
  } = setupChoices;
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
        : `${session.engine.plies}-ply`);
  const setupOpen = visible && setup;
  const engines = usePlayEngines(setupOpen);
  const isBlocked = () => isLocked() || playback.isPlaying();
  const threshold = usePlayThreshold({ session, setup, isBlocked, run });
  useEffect(() => {
    if (
      !visible ||
      setup ||
      !session ||
      !moneyFinished ||
      paused ||
      session.review ||
      busy ||
      playback.animating ||
      isLocked()
    )
      return;
    const key = `${session.id}:${session.game_no}:${session.version}`;
    if (autoMoneyReviews.current.has(key)) return;
    autoMoneyReviews.current.add(key);
    void run(() => reviewPlaySession(session));
  }, [
    visible,
    setup,
    session?.id,
    session?.game_no,
    session?.version,
    moneyFinished,
    paused,
    busy,
    playback.animating,
  ]);
  function pauseMatch() {
    if (!session || playback.isPlaying()) return;
    void run(
      () => actInPlaySession(session, 'pause'),
      () => {
        focusResume.current = true;
      },
    );
  }
  function resumeMatch() {
    if (!session) return;
    if (session.paused) void run(() => actInPlaySession(session, 'resume'));
    else setSetup(false);
  }
  function openMatchSetup() {
    setFocusSetup(true);
    setSetupOptions({});
    dialogs.setPauseWarning(false);
    dialogs.setForfeitOpen(false);
    setSetup(true);
    setError(undefined);
  }
  function closeSetup() {
    if (isLocked()) return;
    setSetup(false);
    setError(undefined);
  }
  function create(event: FormEvent) {
    event.preventDefault();
    unlockPlayAudio();
    void run(() =>
      createPlaySession(human, length, {
        play_mode: mode,
        training_auto_analysis: automatic,
        ...(setupOptions.name === undefined
          ? {}
          : { human_name: setupOptions.name }),
        ...(botDifficulty
          ? { play_engine: 'bgsage', play_difficulty: botDifficulty }
          : setupOptions.engine === undefined &&
              setupOptions.plies === undefined
            ? {}
            : {
                play_engine: botEngine,
                play_plies: botPlies,
                play_top_two: botTopTwo,
              }),
      }),
    );
  }
  function action(
    kind: PlayAction,
    move?: number,
    decisionId?: string,
    resignLevel?: ResignLevel,
  ) {
    if (
      !session ||
      paused ||
      isLocked() ||
      playback.isPlaying() ||
      review.isActive()
    )
      return;
    unlockPlayAudio();
    if (
      kind === 'opening_roll' ||
      (kind === 'roll' &&
        (decisionId ||
          session.play_mode === 'competition' ||
          session.play_mode === 'training' ||
          !session.play_mode ||
          !session.can_double))
    )
      setPendingRoll(kind);
    void run(
      () => actInPlaySession(session, kind, move, decisionId, resignLevel),
      async (value) => {
        setPendingRoll(null);
        if (soundsVisible.current && !value.pending_decision?.action)
          await playback.play(actionFrames(session, value, kind));
        if (soundsVisible.current && isNewCubeOffer(session, value))
          playSounds(['cube']);
      },
    );
  }
  useEffect(() => {
    // Each phase advances from its versioned checkpoint. A failed request
    // waits for an explicit reload, which may reveal that it already committed.
    if (
      !visible ||
      setup ||
      busy ||
      dialogs.forfeitOpen ||
      review.active ||
      error !== undefined
    )
      return;
    if (session?.phase === 'opening_roll') action('opening_roll');
    if (session?.phase === 'opening_result') action('start_opening_move');
  }, [
    visible,
    setup,
    busy,
    dialogs.forfeitOpen,
    error,
    review.active,
    session,
  ]);

  function feedback(operation: 'hint' | 'reveal' | 'retry') {
    if (
      !session ||
      paused ||
      isLocked() ||
      playback.isPlaying() ||
      review.isActive()
    )
      return;
    if (operation === 'hint') {
      dialogs.openHint();
    }
    void run(
      () => updatePlayFeedback(session, operation),
      operation === 'hint' ? () => dialogs.setHintReady(true) : undefined,
      operation === 'hint',
    );
  }
  function openResign() {
    if (isBlocked() || review.isActive()) return;
    setError(undefined);
    dialogs.openResign();
  }
  function closeResign() {
    if (isLocked()) return;
    dialogs.closeResign();
    setError(undefined);
  }
  function openForfeit() {
    setError(undefined);
    dialogs.setForfeitOpen(true);
  }
  function closeForfeit() {
    if (isLocked()) return;
    dialogs.setForfeitOpen(false);
    setError(undefined);
  }
  function closePauseWarning() {
    if (isLocked()) return;
    dialogs.setPauseWarning(false);
    setError(undefined);
  }
  function switchMode(value: PlayMode) {
    if (!session || isBlocked()) return;
    void run(
      () => changePlayMode(session, value),
      (updated) => onModeChanged?.(updated),
      true,
    );
  }
  const {
    pending,
    mode: feedbackMode,
    competition,
    routineRoll,
    awaiting: awaitingFeedback,
  } = playFeedback(session);
  const uiBusy = busy || playback.animating;
  const setupDisabled = uiBusy;
  const canRoll = Boolean(
    visible &&
    !setup &&
    !paused &&
    !review.active &&
    !dialogs.hintOpen &&
    !dialogs.resignOpen &&
    !awaitingFeedback &&
    session?.phase === 'roll',
  );
  const canMove = Boolean(
    session &&
    !paused &&
    !busy &&
    !dialogs.hintOpen &&
    !dialogs.resignOpen &&
    !awaitingFeedback &&
    !review.active &&
    session.phase === 'move' &&
    session.position.player === session.human_player,
  );
  const turn = usePlayTurn({ session, canMove, playback, review, isLocked });
  const selectedSteps =
    !paused && pending?.action === 'move' && pending.move_index != null
      ? (session?.legal_moves[pending.move_index]?.steps ?? [])
      : turn.steps;
  const openingScene =
    playback.scene?.kind === 'dice' && playback.scene.opening
      ? playback.scene
      : null;
  const openingPhase =
    !review.active &&
    (session?.phase === 'opening_roll' ||
      session?.phase === 'opening_result') &&
    (!playback.scene || Boolean(openingScene));
  const canPass =
    turn.forcedPass &&
    visible &&
    !setup &&
    !uiBusy &&
    !pendingRoll &&
    !dialogs.pauseWarning &&
    !dialogs.forfeitOpen;
  const pendingCoachTurn =
    feedbackMode === 'coach' &&
    (pending?.action === 'move' || pending?.action === 'pass')
      ? pending
      : null;
  const canContinueCoachTurn =
    Boolean(pendingCoachTurn) &&
    visible &&
    !setup &&
    !paused &&
    !uiBusy &&
    !dialogs.hintOpen &&
    !dialogs.resignOpen &&
    !dialogs.forfeitOpen &&
    !review.active;
  const canConfirm =
    (canMove && !uiBusy && Boolean(turn.completed)) ||
    (canContinueCoachTurn && pendingCoachTurn?.action === 'move');
  function confirmTurn() {
    if (canContinueCoachTurn && pendingCoachTurn?.action === 'move')
      continueDecision();
    else if (canConfirm && turn.completed)
      action('move', turn.completed.move_index);
  }
  function rollTurn() {
    if (boardRollReady) action('roll', undefined, routineRoll?.id);
  }
  function doubleCube() {
    if (boardRollReady && session?.can_double) action('double');
  }
  const name = (player: number) =>
    player === session?.human_player
      ? session.human_name || t('you')
      : t('bot');
  // The move history renders on its own changes, not on every animation frame.
  const selectHistory = useLatestCallback((index: number) => {
    if (isBlocked()) return;
    turn.hover(null);
    review.select(index);
  });
  const cubeContext = session
    ? matchContext(session.position.gnubg_match_id)
    : null;
  const doubleUnavailableReason = cubeContext?.crawford
    ? t('doubleCrawford')
    : cubeContext?.owner != null && cubeContext.owner !== session?.human_player
      ? t('doubleOpponentCube')
      : session &&
          session.match_length > 0 &&
          cubeContext &&
          cubeContext.cube >=
            session.match_length - session.scores[session.human_player]
        ? t('doubleMatchCovered')
        : t('cubeUnavailable');
  const matchResultKey = session ? `${session.id}:${session.game_no}` : '';
  const coachDecision =
    visible && !setup && !paused && feedbackMode === 'coach' && awaitingFeedback
      ? pending
      : null;
  const interruption = playInterruption({
    session,
    blocked:
      !visible ||
      setup ||
      paused ||
      review.active ||
      playback.animating ||
      pendingRoll !== null,
    resignOpen: dialogs.resignOpen,
    hintOpen: dialogs.hintOpen,
    coaching: Boolean(coachDecision),
    acknowledged: acknowledgedMatch === matchResultKey,
  });
  usePlayDialogFocus({
    dialogs,
    resignTrigger,
    hintTrigger,
    hidden: !visible || setup,
    busy: uiBusy,
    interrupted: Boolean(interruption),
  });
  const boardRollReady =
    canRoll && !uiBusy && !interruption && !dialogs.pauseWarning;
  const assessmentButton =
    session && session.match_length > 0 && session.phase === 'match_over' ? (
      <ActionButton
        icon="analysis"
        label={assessmentAction}
        disabled={uiBusy || (paused && !session.review)}
        onClick={() => {
          if (session.review)
            onOpenAssessment(matchAssessmentReportId(session));
          else
            void run(
              () => reviewPlaySession(session),
              (report) => {
                if (soundsVisible.current)
                  onOpenAssessment(matchAssessmentReportId(report));
              },
            );
        }}
      >
        {assessmentAction}
      </ActionButton>
    ) : undefined;
  const moneyResultDialog =
    moneyFinished &&
    (interruption === 'game_over' || interruption === 'match_over');
  const beforeRoll = session?.phase === 'roll';
  const showBoardActions = Boolean(
    session &&
    !paused &&
    !review.active &&
    !interruption &&
    !awaitingFeedback &&
    (beforeRoll ||
      (session.phase === 'move' &&
        session.position.player === session.human_player)),
  );
  const turnHeading = beforeRoll
    ? 'yourTurn'
    : turn.forcedPass
      ? 'pass'
      : 'move';
  const canForfeit = Boolean(
    session &&
    session.match_length > 0 &&
    !paused &&
    session.phase !== 'match_over',
  );
  const forfeitButton = canForfeit && (
    <ActionButton
      type="button"
      icon="resign"
      ref={forfeitTrigger}
      label={resignT('forfeitMatch')}
      disabled={uiBusy || review.active}
      aria-haspopup="dialog"
      onClick={openForfeit}
    />
  );
  // Resigning sits beside Pause match so it is found without opening the menu.
  const resignButton = session &&
    !paused &&
    session.phase !== 'game_over' &&
    session.phase !== 'match_over' && (
      <ActionButton
        type="button"
        ref={resignTrigger}
        icon="resign"
        label={resignT('confirmGame')}
        disabled={uiBusy || !showBoardActions}
        aria-haspopup="dialog"
        onClick={openResign}
      />
    );
  const hasPauseDraft = turn.steps.length > 0 && pending?.action !== 'move';
  const pauseButton = session &&
    !paused &&
    !session.money_session_ended &&
    (session.phase !== 'match_over' || moneyFinished) && (
      <ActionButton
        type="button"
        ref={interruption ? undefined : pauseTrigger}
        icon="pause"
        label={t('pauseMatch')}
        disabled={uiBusy}
        aria-haspopup={hasPauseDraft ? 'dialog' : undefined}
        onClick={() => {
          if (hasPauseDraft) {
            setError(undefined);
            dialogs.setPauseWarning(true);
          } else pauseMatch();
        }}
      />
    );
  const hintButton = (
    <ActionButton
      ref={hintTrigger}
      className="play-hint-button"
      disabled={uiBusy}
      onClick={() => feedback('hint')}
      icon="hint"
      label={modeT('hint')}
      shortcut="H"
    />
  );
  const resignationDeclined =
    error instanceof AppError &&
    String(error.params.status) === '422' &&
    [
      'GNUbg declined this resignation.',
      'The bot declined this resignation.',
    ].some((prefix) => error.diagnostic.startsWith(prefix));
  const continueDecision = () => {
    if (pending?.action)
      action(
        pending.action as PlayAction,
        pending.move_index ?? undefined,
        pending.id,
      );
  };
  const errorNotice =
    error === undefined ? null : (
      <PlayErrorNotice
        error={error}
        hint={dialogs.hintOpen}
        busy={uiBusy}
        onHint={() => feedback('hint')}
        onRetry={retry}
      />
    );
  const decisionError = dialogs.forfeitOpen ? null : errorNotice;
  const modeControl = session && (
    <PlayModeControl
      session={session}
      mode={feedbackMode}
      threshold={threshold}
      disabled={
        uiBusy || paused || session.phase === 'match_over' || moneyFinished
      }
      onMode={switchMode}
    />
  );
  const dialogPreferences = session && (
    <details className="play-dialog-preferences">
      <summary>
        {modeT('activeMode')}: {modeT(feedbackMode)}
      </summary>
      {modeControl}
    </details>
  );
  const feedbackPanel = session &&
    (pending?.action || pending?.original_action) && (
      <PlayFeedback
        session={session}
        preview={
          interruption === 'feedback'
            ? { theme, white: name(0), black: name(1) }
            : undefined
        }
        busy={uiBusy}
        onReveal={() => feedback('reveal')}
        onRetry={() => feedback('retry')}
        onContinue={continueDecision}
      />
    );
  function turnStatus() {
    if (!session) return '';
    if (review.active) return t('reviewing');
    if (paused) return t('matchPaused');
    const { scene } = playback;
    if (scene?.kind === 'checker')
      return `${name(scene.player)} · ${t('watchMove')}`;
    if (openingScene && !openingScene.rolling)
      return openingScene.tied
        ? t('openingTie')
        : appT('app.toMove', { player: name(openingScene.player) });
    if (pendingRoll || scene?.kind === 'dice')
      return openingPhase
        ? t('openingRolling')
        : `${name(scene?.player ?? session.human_player)} · ${t('rolling')}`;
    if (busy && !dialogs.hintOpen)
      return openingPhase ? t('openingPreparing') : t('thinking');
    if (awaitingFeedback) return modeT('unconfirmed');
    if (session.money_session_ended) return t('sessionEnded');
    if (session.match_forfeited) return resignT('matchForfeited');
    if (turn.forcedPass) return t('pass');
    if (session.phase === 'move' || session.phase === 'roll')
      return session.phase === 'roll' ||
        session.position.player === session.human_player
        ? t('yourTurn')
        : appT('app.toMove', { player: name(session.position.player) });
    return t(session.phase);
  }
  return (
    <div className="play-workspace" aria-busy={uiBusy}>
      <header className="play-header" inert={setupOpen}>
        <h1 className={session ? 'sr-only' : undefined}>{t('title')}</h1>
        {!session && !setup && (
          <div className="play-actions">
            <ActionButton
              ref={newMatchTrigger}
              className="primary"
              onClick={openMatchSetup}
              disabled={uiBusy}
              icon="new-game"
              label={t('newMatch')}
            >
              {t('newMatch')}
            </ActionButton>
            <ActionButton
              onClick={onOpenPausedMatches}
              disabled={uiBusy}
              icon="open"
              label={libraryT('pausedMatches')}
            />
          </div>
        )}
      </header>
      {!setupOpen &&
        !interruption &&
        !dialogs.pauseWarning &&
        !coachDecision &&
        !dialogs.forfeitOpen &&
        errorNotice}
      {!session && (
        <PlayIdleBoard
          anchor={boardAnchor}
          human={human}
          direction={
            confirmed?.appearance.bearoff_direction ??
            settings?.bearoff_direction ??
            'right'
          }
          inert={setupOpen}
          theme={theme}
        />
      )}
      {setupOpen && (
        <PlayBoardDialog
          anchor={boardAnchor}
          heading={t('newMatch')}
          busy={uiBusy}
          className="play-setup-dialog"
          modal={false}
          focusOnOpen={focusSetup}
          returnFocus={session ? menuTrigger : newMatchTrigger}
          onClose={closeSetup}
        >
          <PlaySetupForm
            choices={setupChoices}
            engines={engines}
            errorNotice={errorNotice}
            busy={busy}
            disabled={setupDisabled}
            uiBusy={uiBusy}
            settings={settings}
            session={session}
            onSubmit={create}
            onResume={resumeMatch}
            onOpenPausedMatches={onOpenPausedMatches}
            onOpenPlaySettings={onOpenPlaySettings}
          />
        </PlayBoardDialog>
      )}
      {session && (
        <>
          <div className="play-match-heading" inert={setupOpen}>
            <PlayMatchSummary
              session={session}
              name={name}
              strength={sessionStrength}
              status={turnStatus()}
            />
            <div
              className="play-mode-slot"
              aria-hidden={Boolean(interruption)}
              inert={Boolean(interruption)}
            >
              {modeControl}
            </div>
            <PlayMatchActions
              pause={pauseButton}
              resign={resignButton}
              interrupted={Boolean(interruption)}
              paused={paused}
              busy={uiBusy}
              reviewing={review.active}
              resumeRef={resumeTrigger}
              menuRef={menuTrigger}
              canForfeit={canForfeit}
              canStartMatch={interruption !== 'match_over'}
              onResume={resumeMatch}
              onForfeit={openForfeit}
              onNewMatch={openMatchSetup}
              onOpenPausedMatches={onOpenPausedMatches}
            />
          </div>
          <div className="play-layout" inert={setupOpen}>
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
              disabled={uiBusy || awaitingFeedback}
              decisions={
                competition && session.phase !== 'match_over' && !moneyFinished
                  ? undefined
                  : session.decisions
              }
              onSelect={selectHistory}
              reviewAction={
                session.phase === 'match_over' || moneyFinished ? (
                  <>
                    {!interruption && assessmentButton}
                    <ActionButton
                      ref={learningReviewTrigger}
                      tooltip={modeT('reviewTooltip')}
                      compact
                      disabled={uiBusy || (paused && !session.review)}
                      icon="analysis"
                      label={modeT('review')}
                      onClick={() => {
                        if (session.review) dialogs.setLearningReviewOpen(true);
                        else
                          void run(
                            () => reviewPlaySession(session),
                            () => {
                              if (soundsVisible.current)
                                dialogs.setLearningReviewOpen(true);
                            },
                          );
                      }}
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
                (!interruption || moneyResultDialog) &&
                dialogs.learningReviewOpen &&
                session.review && (
                  <LearningReviewDialog
                    anchor={boardAnchor}
                    returnFocus={
                      moneyResultDialog
                        ? moneyReviewTrigger
                        : learningReviewTrigger
                    }
                    session={session}
                    theme={theme}
                    context={
                      moneyFinished
                        ? `${t('game')} ${session.review_game_no ?? session.game_no}`
                        : undefined
                    }
                    onClose={() => dialogs.setLearningReviewOpen(false)}
                  />
                )}
              <PlayBoardStage
                anchor={boardAnchor}
                session={session}
                theme={theme}
                name={name}
                paused={paused}
                frames={playback.frames}
                pendingRoll={pendingRoll}
                review={review}
                turn={turn}
                selectedSteps={selectedSteps}
                canMove={canMove}
                canConfirm={canConfirm}
                canPass={canPass}
                canContinuePass={
                  canContinueCoachTurn && pendingCoachTurn?.action === 'pass'
                }
                rollReady={boardRollReady}
                openingPhase={openingPhase}
                showActions={showBoardActions}
                doubleReasonId={doubleReasonId}
                doubleUnavailableReason={doubleUnavailableReason}
                turnNotationId={turnNotationId}
                onRoll={rollTurn}
                onDouble={doubleCube}
                onConfirm={confirmTurn}
                onPass={() => action('pass')}
                onContinue={continueDecision}
                before={
                  <>
                    {dialogs.pauseWarning && (
                      <PlayPauseDialog
                        anchor={boardAnchor}
                        busy={uiBusy}
                        returnFocus={pauseTrigger}
                        error={errorNotice}
                        onClose={closePauseWarning}
                        onPause={pauseMatch}
                      />
                    )}
                    {review.index !== null && (
                      <PlayHistoryReviewToolbar
                        review={review}
                        index={review.index}
                        view={playHistoryView(review)}
                        onRetry={() => load(session.id)}
                      />
                    )}
                    {coachDecision && (
                      <PlayCoachFeedback
                        key={`${session.id}:${coachDecision.id}`}
                        anchor={boardAnchor}
                        session={session}
                        decision={coachDecision}
                        theme={theme}
                        white={name(0)}
                        black={name(1)}
                        busy={uiBusy}
                        errorNotice={decisionError}
                        onContinue={continueDecision}
                        onRetry={() => feedback('retry')}
                        onReveal={() => feedback('reveal')}
                      />
                    )}
                    {showBoardActions && (
                      <PlayTurnToolbar
                        session={session}
                        heading={turnHeading}
                        beforeRoll={beforeRoll}
                        forcedPass={turn.forcedPass}
                        rollReady={boardRollReady}
                        canPass={canPass}
                        canConfirm={canConfirm}
                        canMove={canMove}
                        pendingRoutineRoll={Boolean(routineRoll)}
                        competition={competition}
                        hint={hintButton}
                        busy={uiBusy}
                        steps={turn.steps}
                        complete={Boolean(turn.completed)}
                        missingCheckerPaths={turn.missingCheckerPaths}
                        doubleReasonId={doubleReasonId}
                        doubleUnavailableReason={doubleUnavailableReason}
                        notationId={turnNotationId}
                        onRoll={rollTurn}
                        onPass={() => action('pass')}
                        onConfirm={confirmTurn}
                        onDouble={doubleCube}
                        onUndo={turn.undo}
                        onRetry={() => load(session.id)}
                      />
                    )}
                  </>
                }
              >
                {interruption === 'resign' && (
                  <PlayResignDialog
                    anchor={boardAnchor}
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
                    onClose={closeResign}
                    onForfeit={
                      canForfeit
                        ? () => {
                            closeResign();
                            dialogs.setForfeitOpen(true);
                          }
                        : undefined
                    }
                    onConfirm={(level) =>
                      action('resign', undefined, undefined, level)
                    }
                  />
                )}
                {interruption === 'hint' && (
                  <PlayHintDialog
                    anchor={boardAnchor}
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
                    anchor={boardAnchor}
                    session={session}
                    busy={uiBusy}
                    error={decisionError}
                    feedback={feedbackPanel}
                    awaitingFeedback={awaitingFeedback}
                    competition={competition}
                    pause={pauseButton}
                    forfeit={forfeitButton}
                    preferences={dialogPreferences}
                    onTake={() => action('take')}
                    onDrop={() => action('drop')}
                    onHint={() => feedback('hint')}
                  />
                )}
                {interruption === 'feedback' && (
                  <PlayFeedbackDialog
                    anchor={boardAnchor}
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
                {(interruption === 'game_over' ||
                  interruption === 'match_over') && (
                  <PlayResultDialog
                    key={interruption}
                    kind={interruption}
                    anchor={boardAnchor}
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
                    moneyReviewRef={moneyReviewTrigger}
                    onNextGame={() => action('next_game')}
                    onEndSession={() => action('end_session')}
                    onNewMatch={() => {
                      setAcknowledgedMatch(matchResultKey);
                      openMatchSetup();
                    }}
                    onBack={() => setAcknowledgedMatch(matchResultKey)}
                    onReview={() => dialogs.setLearningReviewOpen(true)}
                  />
                )}
                {dialogs.forfeitOpen && canForfeit && (
                  <PlayForfeitDialog
                    anchor={boardAnchor}
                    busy={uiBusy}
                    returnFocus={interruption ? forfeitTrigger : menuTrigger}
                    error={errorNotice}
                    onClose={closeForfeit}
                    onForfeit={() => action('forfeit_match')}
                  />
                )}
              </PlayBoardStage>
              {!busy &&
                !paused &&
                !review.active &&
                !interruption &&
                !awaitingFeedback &&
                (session.phase === 'game_over' ||
                  session.phase === 'match_over') && (
                  <div className="play-turn" aria-live="polite">
                    {session.winner != null && (
                      <p>
                        {t('winner')}: {name(session.winner)}
                      </p>
                    )}
                    {session.phase === 'game_over' && (
                      <div className="play-actions">
                        <ActionButton
                          disabled={uiBusy}
                          onClick={() => action('next_game')}
                          icon="new-game"
                          label={t('next_game')}
                        >
                          {t('next_game')}
                        </ActionButton>
                      </div>
                    )}
                  </div>
                )}
            </section>
          </div>
        </>
      )}
    </div>
  );
}
