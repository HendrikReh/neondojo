import { useEffect, useId, useRef, useState, type RefObject } from 'react';
import { useLatestCallback } from '../../shared/hooks/useLatest';
import { useI18n } from '../../shared/i18n/i18n';
import { actionFrames } from '../../shared/board/play-animation';
import { playSounds, unlockPlayAudio } from '../../shared/board/play-audio';
import { isNewCubeOffer } from '../../shared/board/play-sound-events';
import { matchAssessmentReportId } from '../study/reports/match-assessment-api';
import {
  actInPlaySession,
  changePlayMode,
  reviewPlaySession,
  updatePlayFeedback,
  type PlayAction,
  type PlaySession,
} from './play-api';
import {
  isMoneyGameFinished,
  playFeedback,
  playInterruption,
} from './play-interruption';
import { usePlayText } from './play-messages';
import { usePlayModeText, type PlayMode } from './play-mode-messages';
import { usePlayResignText } from './play-resign-messages';
import {
  computePlayTurnStatus,
  doubleUnavailableReasonKey,
} from './play-turn-status';
import type { ResignLevel } from './PlayResignDialog';
import type { PlayBoardCallbacks, PlayBoardTurnState } from './PlayBoardStage';
import type {
  PlayTurnToolbarCallbacks,
  PlayTurnToolbarLabels,
  PlayTurnToolbarState,
} from './PlayTurnToolbar';
import type { usePlayAnimation } from './usePlayAnimation';
import type { PlayDialogState } from './usePlayDialogs';
import type { usePlayReview } from './usePlayReview';
import type { usePlaySession } from './usePlaySession';
import { usePlayTurn } from './usePlayTurn';

export { computePlayTurnStatus } from './play-turn-status';

export interface UsePlayTurnActionsOptions {
  visible: boolean;
  playSession: ReturnType<typeof usePlaySession>;
  playback: ReturnType<typeof usePlayAnimation>;
  review: ReturnType<typeof usePlayReview>;
  dialogs: PlayDialogState;
  soundsVisible: RefObject<boolean>;
  onPaused: () => void;
  onModeChanged?: (session: PlaySession) => void;
  onOpenSetup: () => void;
  name: (player: number) => string;
}

export type PlayTurnActions = ReturnType<typeof usePlayTurnActions>;

/** Turn, feedback, pause, forfeit, and phase-progression actions plus turn status. */
export function usePlayTurnActions({
  visible,
  playSession,
  playback,
  review,
  dialogs,
  soundsVisible,
  onPaused,
  onModeChanged,
  onOpenSetup,
  name,
}: UsePlayTurnActionsOptions) {
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
    isLocked,
  } = playSession;
  const t = usePlayText();
  const { t: appT } = useI18n();
  const modeT = usePlayModeText();
  const resignT = usePlayResignText();
  const doubleReasonId = useId();
  const turnNotationId = useId();
  const autoMoneyReviews = useRef(new Set<string>());
  const [acknowledgedMatch, setAcknowledgedMatch] = useState<string | null>(
    null,
  );

  const paused = Boolean(session?.paused);
  const moneyFinished = isMoneyGameFinished(session);
  const isBlocked = () => isLocked() || playback.isPlaying();
  const {
    pending,
    mode: feedbackMode,
    competition,
    routineRoll,
    awaiting: awaitingFeedback,
  } = playFeedback(session);
  const uiBusy = busy || playback.animating;

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
    void run(() => actInPlaySession(session, 'pause'), onPaused);
  }

  function resumeMatch() {
    if (!session) return;
    if (session.paused) void run(() => actInPlaySession(session, 'resume'));
    else setSetup(false);
  }

  function action(
    kind: PlayAction,
    move?: number,
    decisionId?: string,
    resignLevel?: ResignLevel,
  ) {
    if (!session || paused || isBlocked() || review.isActive()) return;
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
  }, [visible, setup, busy, dialogs.forfeitOpen, error, review.active, session]);

  function feedback(operation: 'hint' | 'reveal' | 'retry') {
    if (!session || paused || isBlocked() || review.isActive()) return;
    if (operation === 'hint') dialogs.openHint();
    void run(
      () => updatePlayFeedback(session, operation),
      operation === 'hint' ? () => dialogs.setHintReady(true) : undefined,
      operation === 'hint',
    );
  }

  const closeDialog = (close: () => void) => {
    if (isLocked()) return;
    close();
    setError(undefined);
  };
  const openResign = () => {
    if (isBlocked() || review.isActive()) return;
    setError(undefined);
    dialogs.openResign();
  };
  const closeResign = () => closeDialog(dialogs.closeResign);
  const openForfeit = () => {
    setError(undefined);
    dialogs.setForfeitOpen(true);
  };
  const closeForfeit = () => closeDialog(() => dialogs.setForfeitOpen(false));
  const closePauseWarning = () =>
    closeDialog(() => dialogs.setPauseWarning(false));
  const switchMode = (value: PlayMode) => {
    if (!session || isBlocked()) return;
    void run(
      () => changePlayMode(session, value),
      (updated) => onModeChanged?.(updated),
      true,
    );
  };

  const interactive =
    !paused &&
    !dialogs.hintOpen &&
    !dialogs.resignOpen &&
    !awaitingFeedback &&
    !review.active;
  const canRoll = Boolean(visible && !setup && interactive && session?.phase === 'roll');
  const canMove = Boolean(
    session &&
      !busy &&
      interactive &&
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
    (session?.phase === 'opening_roll' || session?.phase === 'opening_result') &&
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

  const continueDecision = () => {
    if (pending?.action)
      action(
        pending.action as PlayAction,
        pending.move_index ?? undefined,
        pending.id,
      );
  };
  const confirmTurn = () => {
    if (canContinueCoachTurn && pendingCoachTurn?.action === 'move')
      continueDecision();
    else if (canConfirm && turn.completed)
      action('move', turn.completed.move_index);
  };

  // The move history renders on its own changes, not on every animation frame.
  const selectHistory = useLatestCallback((index: number) => {
    if (isBlocked()) return;
    turn.hover(null);
    review.select(index);
  });

  const doubleUnavailableReason = t(doubleUnavailableReasonKey(session));
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
  const boardRollReady =
    canRoll && !uiBusy && !interruption && !dialogs.pauseWarning;
  const rollTurn = () => {
    if (boardRollReady) action('roll', undefined, routineRoll?.id);
  };
  const doubleCube = () => {
    if (boardRollReady && session?.can_double) action('double');
  };

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
  const canForfeit = Boolean(
    session && session.match_length > 0 && !paused && session.phase !== 'match_over',
  );
  const hasPauseDraft = turn.steps.length > 0 && pending?.action !== 'move';

  const requestPause = () => {
    if (hasPauseDraft) {
      setError(undefined);
      dialogs.setPauseWarning(true);
    } else pauseMatch();
  };
  const ensureReview = (onReady: (reviewed: PlaySession) => void) => {
    if (!session) return;
    if (session.review) onReady(session);
    else
      void run(
        () => reviewPlaySession(session),
        (report) => {
          if (soundsVisible.current) onReady(report);
        },
      );
  };
  const openAssessment = (onOpenAssessment: (reportId: string) => void) =>
    ensureReview((report) =>
      onOpenAssessment(matchAssessmentReportId(report)),
    );
  const openLearningReview = () =>
    ensureReview(() => dialogs.setLearningReviewOpen(true));

  const turnStatus = computePlayTurnStatus(
    {
      session,
      reviewActive: review.active,
      paused,
      scene: playback.scene,
      openingScene,
      openingPhase,
      pendingRoll,
      busy,
      hintOpen: dialogs.hintOpen,
      awaitingFeedback,
      forcedPass: turn.forcedPass,
      name,
    },
    { t, appT, modeT, resignT },
  );

  const onPass = () => action('pass');
  const boardTurn: PlayBoardTurnState = {
    draft: turn,
    selectedSteps,
    canMove,
    canConfirm,
    canPass,
    canContinuePass: canContinueCoachTurn && pendingCoachTurn?.action === 'pass',
    rollReady: boardRollReady,
    showActions: showBoardActions,
    doubleReasonId,
    doubleUnavailableReason,
    turnNotationId,
  };
  const boardCallbacks: PlayBoardCallbacks = {
    onRoll: rollTurn,
    onDouble: doubleCube,
    onConfirm: confirmTurn,
    onPass,
    onContinue: continueDecision,
  };
  const toolbarState: PlayTurnToolbarState = {
    heading: beforeRoll ? 'yourTurn' : turn.forcedPass ? 'pass' : 'move',
    beforeRoll,
    forcedPass: turn.forcedPass,
    rollReady: boardRollReady,
    canPass,
    canConfirm,
    canMove,
    pendingRoutineRoll: Boolean(routineRoll),
    competition,
    busy: uiBusy,
    steps: turn.steps,
    complete: Boolean(turn.completed),
    missingCheckerPaths: turn.missingCheckerPaths,
  };
  const toolbarLabels: PlayTurnToolbarLabels = {
    doubleReasonId,
    doubleUnavailableReason,
    notationId: turnNotationId,
  };
  const toolbarCallbacks: PlayTurnToolbarCallbacks = {
    ...boardCallbacks,
    onUndo: turn.undo,
    onRetry: () => {
      if (session) load(session.id);
    },
  };

  return {
    paused,
    moneyFinished,
    pending,
    feedbackMode,
    competition,
    awaitingFeedback,
    uiBusy,
    turn,
    openingPhase,
    coachDecision,
    interruption,
    moneyResultDialog,
    showBoardActions,
    canForfeit,
    hasPauseDraft,
    turnStatus,
    boardTurn,
    boardCallbacks,
    toolbarState,
    toolbarLabels,
    toolbarCallbacks,
    isBlocked,
    action,
    feedback,
    continueDecision,
    pauseMatch,
    requestPause,
    resumeMatch,
    openResign,
    closeResign,
    openForfeit,
    closeForfeit,
    closePauseWarning,
    switchMode,
    selectHistory,
    acknowledgeMatch: () => setAcknowledgedMatch(matchResultKey),
    acknowledgeAndNewMatch: () => {
      setAcknowledgedMatch(matchResultKey);
      onOpenSetup();
    },
    openAssessment,
    openLearningReview,
  };
}
