import { useSettingsText } from './settings-messages';
import EngineLabel from './EngineLabel';
import Tooltip from './Tooltip';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import {
  useEffect,
  useId,
  useRef,
  useMemo,
  useState,
  type CSSProperties,
  type FormEvent,
} from 'react';
import Board from './Board';
import PlayOpeningRoll from './PlayOpeningRoll';
import PlayHistory from './PlayHistory';
import PlayFeedback from './PlayFeedback';
import PlayCoachFeedback from './PlayCoachFeedback';
import PlayHintView from './PlayHintView';
import { hasPlayMovePreview } from './PlayMovePreview';
import LearningReviewDialog from './LearningReviewDialog';
import {
  playModes,
  usePlayModeText,
  type PlayMode,
} from './play-mode-messages';
import { loadSettings } from './settings-api';
import { usePlayReview } from './usePlayReview';
import { projectMove, type MovePreview, type BoardView } from './move-preview';
import { useBoardReviewText } from './board-review-messages';
import type { Theme } from './themes';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import { usePlayText } from './play-messages';
import { useLibraryText } from './library-messages';
import {
  orderedLegalTurns,
  applySteps,
  boardFlips,
  matchingMoves,
  nextPoints,
  relativePoint,
  type CheckerStep,
} from './play-state';
import {
  actInPlaySession,
  createPlaySession,
  loadPlaySession,
  updatePlayFeedback,
  reviewPlaySession,
  changePlayMode,
  type PlayAction,
  type PlaySession,
} from './play-api';
import { playSounds, stopPlaySounds, unlockPlayAudio } from './play-audio';
import { isNewCubeOffer } from './play-sound-events';
import { matchContext } from './boardMath';
import PlayCubeDecision from './PlayCubeDecision';
import PlayBoardDialog from './PlayBoardDialog';
import PlayResignDialog, { type ResignLevel } from './PlayResignDialog';
import { usePlayResignText } from './play-resign-messages';
import GameResultSummary from './GameResultSummary';
import { actionFrames, checkerFrame, reverseFrame } from './play-animation';
import { usePlayAnimation, useRollingFaces } from './usePlayAnimation';
import './play.css';
const storageKey = 'bgtutor.play-session';
function storedSession() {
  try {
    return localStorage.getItem(storageKey);
  } catch {
    return null;
  }
}
export default function PlayWorkspace({
  visible,
  theme,
  requestedSession,
  onSaved,
  onModeChanged,
  onOpenPausedMatches,
}: {
  visible: boolean;
  theme: Theme;
  requestedSession?: { id: string; token: number; resume?: boolean };
  onSaved?: () => void;
  onModeChanged?: (session: PlaySession) => void;
  onOpenPausedMatches: () => void;
}) {
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
  const setupHeading = useRef<HTMLHeadingElement>(null);
  const focusSetup = useRef(false);
  const [pauseWarning, setPauseWarning] = useState(false);
  const autoMoneyReviews = useRef(new Set<string>());
  const [learningReviewOpen, setLearningReviewOpen] = useState(false);
  const resignTrigger = useRef<HTMLButtonElement>(null);
  const restoreResignFocus = useRef(false);
  const [resignOpen, setResignOpen] = useState(false);
  const restoreHintFocus = useRef(false);
  const [acknowledgedMatch, setAcknowledgedMatch] = useState<string | null>(
    null,
  );
  const [hintOpen, setHintOpen] = useState(false);
  const [hintReady, setHintReady] = useState(false);
  const [pendingRoll, setPendingRoll] = useState<PlayAction | null>(null);
  const waitingFaces = useRollingFaces(pendingRoll !== null);
  const soundsVisible = useRef(visible);
  soundsVisible.current = visible;
  useEffect(() => {
    if (!visible) {
      stopPlaySounds();
      setHintOpen(false);
      setResignOpen(false);
      setPauseWarning(false);
      setLearningReviewOpen(false);
    }
    return () => stopPlaySounds();
  }, [visible]);
  const t = usePlayText(),
    { formatError, formatNumber, t: appT } = useI18n();
  const modeT = usePlayModeText();
  const libraryT = useLibraryText();
  const resignT = usePlayResignText();
  const [session, setSession] = useState<PlaySession | null>(null);
  const paused = Boolean(session?.paused);
  const moneyFinished =
    session?.match_length === 0 &&
    ['game_over', 'match_over'].includes(session.phase);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(),
    [setup, setSetup] = useState(true);
  const review = usePlayReview(session, visible && !setup);
  useEffect(() => {
    if (setup && focusSetup.current) {
      setupHeading.current?.focus({ preventScroll: true });
      focusSetup.current = false;
    }
  }, [setup]);
  useEffect(() => {
    if (visible && paused && !busy && !setup && focusResume.current)
      resumeTrigger.current?.focus({ preventScroll: true });
    if (!visible || !busy) focusResume.current = false;
  }, [visible, paused, busy, setup]);
  const reviewText = useBoardReviewText();
  const configuration = useOptionalConfiguration();
  const settingsT = useSettingsText();
  const [engineDefaults, setEngineDefaults] = useState<{
    play_engine: string;
    analysis_engine: string;
    play_plies: number;
    analysis_plies: number;
  }>();
  const [localHuman, setLocalHuman] = useState(0),
    [localLength, setLocalLength] = useState(7);
  const [localMode, setLocalMode] = useState<PlayMode>('coach');
  const [localThreshold, setThreshold] = useState(0.04);
  const human = configuration?.current?.play.human_player ?? localHuman;
  const length = configuration?.current?.play.match_length ?? localLength;
  const mode = configuration?.current?.play.default_mode ?? localMode;
  const threshold =
    configuration?.current?.play.tutor_threshold ?? localThreshold;
  const setHuman = (value: number) => {
    setLocalHuman(value);
    configuration?.updateCurrent((c) => ({
      ...c,
      play: { ...c.play, human_player: value as 0 | 1 },
    }));
  };
  const setLength = (value: number) => {
    setLocalLength(value);
    configuration?.updateCurrent((c) => ({
      ...c,
      play: { ...c.play, match_length: value },
    }));
  };
  const setMode = (value: PlayMode) => {
    setLocalMode(value);
    configuration?.updateCurrent((c) => ({
      ...c,
      play: { ...c.play, default_mode: value },
    }));
  };
  const [liveThreshold, setLiveThreshold] = useState('0.04');
  const [thresholdStatus, setThresholdStatus] = useState<
    'idle' | 'saving' | 'saved'
  >('idle');
  useEffect(() => {
    setThresholdStatus('idle');
  }, [session?.id, setup]);
  useEffect(() => {
    if (thresholdStatus !== 'saved') return;
    const timer = window.setTimeout(() => setThresholdStatus('idle'), 3000);
    return () => window.clearTimeout(timer);
  }, [thresholdStatus]);
  const [localAutomatic, setLocalAutomatic] = useState(true);
  const automatic =
    configuration?.current?.play.training_auto_analysis ?? localAutomatic;
  const setAutomatic = (value: boolean) => {
    setLocalAutomatic(value);
    configuration?.updateCurrent((c) => ({
      ...c,
      play: { ...c.play, training_auto_analysis: value },
    }));
  };
  const [defaultsLoading, setDefaultsLoading] = useState(false);
  const [defaultsError, setDefaultsError] = useState(false);
  useEffect(() => {
    if (configuration || !visible || !setup) return;
    const abort = new AbortController();
    setDefaultsLoading(true);
    setDefaultsError(false);
    void loadSettings(abort.signal)
      .then((settings) => {
        if (abort.signal.aborted) return;
        setEngineDefaults(settings);
        setMode(settings.default_play_mode ?? 'coach');
        setThreshold(settings.tutor_threshold ?? 0.04);
        setAutomatic(settings.training_auto_analysis ?? true);
      })
      .catch(() => {
        if (!abort.signal.aborted) setDefaultsError(true);
      })
      .finally(() => {
        if (!abort.signal.aborted) setDefaultsLoading(false);
      });
    return () => abort.abort();
  }, [visible, setup, !!configuration]);
  const [steps, setSteps] = useState<CheckerStep[]>([]);
  const [hovered, setHovered] = useState<number | null>(null);
  const [swapped, setSwapped] = useState(false);
  const generation = useRef(0),
    locked = useRef(false),
    initialized = useRef(false);
  const consumedRequest = useRef<number | undefined>(undefined);
  const retrySession = useRef<string | null>(null);
  const initialLoad = useRef<AbortController | null>(null);
  useEffect(
    () => () => {
      initialLoad.current?.abort();
      initialLoad.current = null;
      ++generation.current;
      locked.current = false;
      initialized.current = false;
      consumedRequest.current = undefined;
    },
    [],
  );
  function accept(value: PlaySession, preserveSteps = false) {
    if (value.id !== session?.id || value.game_no !== session?.game_no)
      setLearningReviewOpen(false);
    if (!preserveSteps || value.id !== session?.id) {
      setHintOpen(false);
      setResignOpen(false);
      setPauseWarning(false);
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    }
    review.reset();
    setSession(value);
    setLiveThreshold(String(value.tutor_threshold ?? 0.04));
    setSetup(false);
    if (!preserveSteps) {
      setSteps([]);
      setHovered(null);
      setSwapped(false);
    }
    setError(undefined);
    retrySession.current = value.id;
    onSaved?.();
    try {
      localStorage.setItem(storageKey, value.id);
    } catch {
      /* Resume through Match Database remains available. */
    }
  }
  useEffect(() => {
    if (!visible) return;
    const requested =
      requestedSession && consumedRequest.current !== requestedSession.token;
    // Explicit selection may replace a read; an in-flight native action must finish.
    if (locked.current && !(requested && initialLoad.current)) return;
    if (!requested && initialized.current) return;
    initialized.current = true;
    if (requested) consumedRequest.current = requestedSession.token;
    const id = requested ? requestedSession.id : storedSession();
    if (!id) return;
    load(id, requested ? requestedSession.resume : false);
  }, [visible, requestedSession, busy]);
  function load(id: string, resume = false) {
    if (locked.current && !initialLoad.current) return;
    setError(undefined);
    retrySession.current = id;
    initialLoad.current?.abort();
    const abort = new AbortController(),
      token = ++generation.current;
    initialLoad.current = abort;
    playback.cancel();
    setBusy(true);
    locked.current = true;
    void loadPlaySession(id, abort.signal)
      .then(async (value) => {
        if (token !== generation.current) return;
        if (resume && value.paused) {
          // Once the mutation starts, a later selection must wait for it.
          initialLoad.current = null;
          try {
            value = await actInPlaySession(value, 'resume');
          } catch (e) {
            if (token === generation.current) accept(value);
            throw e;
          }
        }
        if (token === generation.current) accept(value);
      })
      .catch((e) => {
        if (!abort.signal.aborted && token === generation.current) setError(e);
      })
      .finally(() => {
        if (token === generation.current) {
          initialLoad.current = null;
          setBusy(false);
          locked.current = false;
        }
      });
  }
  async function run(
    work: () => Promise<PlaySession>,
    onSuccess?: (value: PlaySession) => void | Promise<void>,
    preserveSteps = false,
  ) {
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    setError(undefined);
    const token = ++generation.current;
    try {
      const value = await work();
      if (token === generation.current) {
        await onSuccess?.(value);
        if (token === generation.current) accept(value, preserveSteps);
      }
    } catch (e) {
      if (token === generation.current) setError(e);
    } finally {
      if (token === generation.current) {
        locked.current = false;
        setPendingRoll(null);
        setBusy(false);
      }
    }
  }
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
      locked.current
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
    focusSetup.current = true;
    setSetup(true);
    setError(undefined);
  }
  function create(event: FormEvent) {
    event.preventDefault();
    void run(() =>
      configuration
        ? configuration.runWithConfiguration((c) =>
            createPlaySession(c.play.human_player, c.play.match_length, {
              play_mode: c.play.default_mode,
              tutor_threshold: c.play.tutor_threshold,
              training_auto_analysis: c.play.training_auto_analysis,
            }),
          )
        : createPlaySession(human, length, {
            play_mode: mode,
            tutor_threshold: threshold,
            training_auto_analysis: automatic,
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
      locked.current ||
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
  function feedback(operation: 'hint' | 'reveal' | 'retry') {
    if (
      !session ||
      paused ||
      locked.current ||
      playback.isPlaying() ||
      review.isActive()
    )
      return;
    if (operation === 'hint') {
      setHintReady(false);
      setHintOpen(true);
    }
    void run(
      () => updatePlayFeedback(session, operation),
      operation === 'hint' ? () => setHintReady(true) : undefined,
      operation === 'hint',
    );
  }
  function closeHint() {
    restoreHintFocus.current = true;
    setHintOpen(false);
  }
  function openResign() {
    if (locked.current || playback.isPlaying() || review.isActive()) return;
    setError(undefined);
    setResignOpen(true);
  }
  function closeResign() {
    if (locked.current) return;
    restoreResignFocus.current = true;
    setResignOpen(false);
    setError(undefined);
  }
  function switchMode(value: PlayMode) {
    if (!session || locked.current || playback.isPlaying()) return;
    void run(
      () => changePlayMode(session, value),
      (updated) => onModeChanged?.(updated),
      true,
    );
  }
  function saveThreshold() {
    if (
      !session ||
      locked.current ||
      playback.isPlaying() ||
      session.phase === 'match_over'
    )
      return;
    if (!validLiveThreshold) {
      setLiveThreshold(String(session.tutor_threshold ?? 0.04));
      return;
    }
    if (Number(liveThreshold) === (session.tutor_threshold ?? 0.04)) return;
    setThresholdStatus('saving');
    void run(
      () =>
        changePlayMode(
          session,
          session.play_mode ?? 'competition',
          Number(liveThreshold),
        ),
      () => setThresholdStatus('saved'),
      true,
    ).then(() =>
      setThresholdStatus((status) => (status === 'saving' ? 'idle' : status)),
    );
  }
  const validLiveThreshold =
    liveThreshold.trim() !== '' &&
    Number.isFinite(Number(liveThreshold)) &&
    Number(liveThreshold) > 0 &&
    Number(liveThreshold) <= 1;
  const pending = session?.pending_decision;
  const awaitingFeedback = Boolean(pending?.action);
  const feedbackMode = session?.play_mode ?? 'competition';
  const competition = feedbackMode === 'competition';
  const selectedSteps =
    !paused && pending?.action === 'move' && pending.move_index != null
      ? (session?.legal_moves[pending.move_index]?.steps ?? [])
      : steps;
  const uiBusy = busy || playback.animating;
  const setupDisabled =
    uiBusy || defaultsLoading || (!!configuration && !configuration.ready);
  useEffect(() => {
    if (!visible || setup) {
      restoreResignFocus.current = false;
      return;
    }
    if (!resignOpen && !uiBusy && restoreResignFocus.current) {
      resignTrigger.current?.focus({ preventScroll: true });
      restoreResignFocus.current = false;
    }
  }, [resignOpen, uiBusy, visible, setup]);
  useEffect(() => {
    if (!visible || setup) {
      restoreHintFocus.current = false;
      return;
    }
    if (!hintOpen && !uiBusy && restoreHintFocus.current) {
      hintTrigger.current?.focus({ preventScroll: true });
      restoreHintFocus.current = false;
    }
  }, [hintOpen, uiBusy, visible, setup]);
  const canRoll = Boolean(
    visible &&
    !paused &&
    !review.active &&
    !hintOpen &&
    !resignOpen &&
    !awaitingFeedback &&
    session?.phase === 'roll' &&
    !uiBusy,
  );
  const canMove = Boolean(
    session &&
    !paused &&
    !busy &&
    !hintOpen &&
    !resignOpen &&
    !awaitingFeedback &&
    !review.active &&
    session.phase === 'move' &&
    session.position.player === session.human_player,
  );
  const openingFrame =
    playback.frame?.kind === 'dice' && playback.frame.opening
      ? playback.frame
      : null;
  const openingPhase =
    !review.active &&
    (session?.phase === 'opening_roll' ||
      session?.phase === 'opening_result') &&
    (!playback.frame || Boolean(openingFrame));
  const nativeMoves = session?.legal_moves ?? [];
  const dice = useMemo(
    () =>
      playback.frame && !openingFrame
        ? playback.frame.dice
        : pendingRoll === 'roll'
          ? waitingFaces
          : session && !openingPhase
            ? swapped
              ? [...session.position.dice].reverse()
              : session.position.dice
            : [],
    [
      session,
      swapped,
      openingPhase,
      playback.frame,
      openingFrame,
      pendingRoll,
      waitingFaces,
    ],
  );
  const moves = useMemo(
    () =>
      session
        ? orderedLegalTurns(
            session.position.board,
            session.human_player,
            dice,
            nativeMoves,
          )
        : [],
    [session, dice, nativeMoves],
  );
  const sources = canMove ? nextPoints(moves, steps, null) : [];
  const hoverSource =
    hovered !== null && sources.includes(hovered) ? hovered : null;
  const hoverDestinations =
    canMove && hoverSource !== null
      ? nextPoints(moves, steps, hoverSource)
      : [];
  const completed = matchingMoves(moves, steps).find(
    (move) => move.steps.length === steps.length && steps.length > 0,
  );
  const forcedPass = canMove && nativeMoves.length === 0;
  const canSwap =
    canMove &&
    !playback.animating &&
    steps.length === 0 &&
    dice.length === 2 &&
    dice[0] !== dice[1];
  const canConfirm = canMove && !uiBusy && Boolean(completed);
  function confirmTurn() {
    if (canConfirm && completed) action('move', completed.moveIndex);
  }
  function rollTurn() {
    if (canRoll) action('roll');
  }
  function doubleCube() {
    if (canRoll && session?.can_double) action('double');
  }
  function swapDice() {
    if (!canSwap || review.isActive()) return;
    setSwapped((value) => !value);
    setHovered(null);
  }
  function point(point: number) {
    if (!canMove || !session || !sources.includes(point) || review.isActive())
      return;
    const targets = nextPoints(moves, steps, point);
    if (targets.length !== 1) return;
    const step = { from: point, to: targets[0] };
    unlockPlayAudio();
    const board = applySteps(
      session.position.board,
      steps,
      session.human_player,
    );
    setSteps((old) => [...old, step]);
    setHovered(point);
    void playback.play([checkerFrame(board, step, session.human_player, dice)]);
  }
  function undo() {
    if (
      !session ||
      !steps.length ||
      locked.current ||
      playback.isPlaying() ||
      review.isActive()
    )
      return;
    const index = steps.length - 1;
    const board = applySteps(
      session.position.board,
      steps.slice(0, index),
      session.human_player,
    );
    const frame = reverseFrame(
      checkerFrame(board, steps[index], session.human_player, dice),
    );
    setSteps(steps.slice(0, index));
    setHovered(null);
    void playback.play([frame]);
  }
  const name = (player: number) =>
    player === session?.human_player
      ? session.human_name || t('you')
      : t('bot');
  const [flipX, flipY] = boardFlips(
    session?.human_player ?? 0,
    session?.bearoff_direction ?? 'right',
  );
  const historical = review.entry;
  const historyView =
    historical?.kind !== 'checker' && review.view === 'arrows'
      ? 'after'
      : review.view;
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
  const liveBoard = session
    ? applySteps(session.position.board, selectedSteps, session.human_player)
    : [];
  const displayedBoard = historical
    ? historyView === 'after'
      ? historical.after.board
      : historical.before.board
    : liveBoard;
  function selectHistory(index: number) {
    if (locked.current || playback.isPlaying()) return;
    setHovered(null);
    review.select(index);
  }
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
    visible && !setup && !paused && feedbackMode === 'coach' && pending?.action
      ? pending
      : null;
  const interruption =
    !visible ||
    setup ||
    paused ||
    review.active ||
    playback.animating ||
    pendingRoll
      ? null
      : resignOpen
        ? 'resign'
        : hintOpen && !competition
          ? 'hint'
          : coachDecision
            ? null
            : session?.phase === 'double_response'
              ? 'cube'
              : awaitingFeedback
                ? 'feedback'
                : session?.phase === 'opening_roll' ||
                    session?.phase === 'opening_result'
                  ? 'opening'
                  : session?.phase === 'move' &&
                      session.position.player === session.human_player &&
                      !nativeMoves.length
                    ? 'pass'
                    : session?.phase === 'game_over'
                      ? 'game_over'
                      : moneyFinished && !session?.money_session_ended
                        ? 'game_over'
                        : session?.phase === 'match_over' &&
                            acknowledgedMatch !== matchResultKey
                          ? 'match_over'
                          : null;
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
        session.position.player === session.human_player &&
        nativeMoves.length > 0)),
  );
  const showBoardError = showBoardActions && beforeRoll;
  const hasPauseDraft = steps.length > 0 && pending?.action !== 'move';
  function closePauseWarning() {
    if (locked.current) return;
    setPauseWarning(false);
    setError(undefined);
  }
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
            setPauseWarning(true);
          } else pauseMatch();
        }}
      />
    );
  const cubeDecisionOpen = interruption === 'cube';
  const resignationDeclined =
    error instanceof AppError &&
    String(error.params.status) === '422' &&
    [
      'GNUbg declined this resignation.',
      'The bot declined this resignation.',
    ].some((prefix) => error.diagnostic.startsWith(prefix));
  const currentResult = session?.game_results?.find(
    (result) => result.game_no === session.game_no,
  )?.result;
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
      <div role="alert" className="play-error">
        <p>{t(hintOpen ? 'hintError' : 'unknownError')}</p>
        <p>{formatError(error)}</p>
        {hintOpen && (
          <ActionButton
            disabled={uiBusy}
            onClick={() => feedback('hint')}
            icon="reset"
            label={modeT('retry')}
          >
            {modeT('retry')}
          </ActionButton>
        )}
        <ActionButton
          disabled={uiBusy}
          onClick={() => {
            const id = retrySession.current ?? storedSession();
            if (id) load(id);
          }}
          icon="refresh"
          label={t('retry')}
        >
          {t('retry')}
        </ActionButton>
      </div>
    );
  const modeControl = session && (
    <div className="play-mode-control">
      <label>
        {modeT('mode')}
        <select
          aria-label={modeT('changeMode')}
          value={feedbackMode}
          disabled={
            uiBusy || paused || session.phase === 'match_over' || moneyFinished
          }
          onChange={(event) => switchMode(event.target.value as PlayMode)}
        >
          {playModes.map((value) => (
            <option key={value} value={value}>
              {modeT(value)}
            </option>
          ))}
        </select>
      </label>
      {feedbackMode === 'tutor' && (
        <div className="play-threshold-control">
          <label>
            {modeT('threshold')}
            <Tooltip
              as="input"
              aria-label={modeT('threshold')}
              text={modeT('liveThresholdHelp')}
              type="number"
              min="0"
              max="1"
              step="any"
              required
              value={liveThreshold}
              disabled={
                uiBusy ||
                paused ||
                session.phase === 'match_over' ||
                moneyFinished
              }
              onChange={(event) => {
                setThresholdStatus('idle');
                setLiveThreshold(event.target.value);
              }}
              onBlur={saveThreshold}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  event.currentTarget.blur();
                }
              }}
            />
          </label>
          <span
            className="play-threshold-status"
            role="status"
            aria-atomic="true"
            data-state={thresholdStatus}
          >
            {thresholdStatus === 'saving'
              ? modeT('thresholdSaving')
              : thresholdStatus === 'saved'
                ? modeT('thresholdSaved')
                : ''}
          </span>
        </div>
      )}
      {session.learning_used && feedbackMode === 'competition' && (
        <small>{modeT('learningUsed')}</small>
      )}
    </div>
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
    const frame = playback.frame;
    if (frame?.kind === 'checker')
      return `${name(frame.player)} · ${t('watchMove')}`;
    if (pendingRoll || frame?.kind === 'dice')
      return openingPhase
        ? t('openingRolling')
        : `${name(frame?.player ?? session.human_player)} · ${t('rolling')}`;
    if (busy && !hintOpen) {
      if (openingPhase) return t('openingPreparing');
      return t('thinking');
    }
    if (awaitingFeedback) return modeT('unconfirmed');
    if (session.money_session_ended) return t('sessionEnded');
    if (forcedPass) return t('pass');
    if (session.phase === 'move' || session.phase === 'roll')
      return session.phase === 'roll' ||
        session.position.player === session.human_player
        ? t('yourTurn')
        : appT('app.toMove', { player: name(session.position.player) });
    return t(session.phase);
  }
  return (
    <div className="play-workspace" aria-busy={uiBusy}>
      <header className="play-header">
        <div>
          <h1>{t('title')}</h1>
          <p>{t('opening')}</p>
        </div>
        <div className="play-actions">
          <ActionButton
            onClick={onOpenPausedMatches}
            disabled={uiBusy}
            icon="open"
            label={libraryT('pausedMatches')}
          />
          {!setup && pauseButton && (
            <span
              className="play-pause-slot"
              aria-hidden={Boolean(interruption)}
              inert={Boolean(interruption)}
            >
              {pauseButton}
            </span>
          )}
          {!setup && paused && (
            <ActionButton
              ref={resumeTrigger}
              className="primary"
              onClick={resumeMatch}
              disabled={uiBusy}
              icon="resume"
              label={t('resumeMatch')}
            />
          )}
          {!setup && interruption !== 'match_over' && (
            <ActionButton
              onClick={openMatchSetup}
              disabled={uiBusy}
              icon="new-game"
              label={t('newMatch')}
            />
          )}
        </div>
      </header>
      {!interruption &&
        !pauseWarning &&
        !coachDecision &&
        (!showBoardError || setup) &&
        errorNotice}
      {setup && busy && <p role="status">{t('thinking')}</p>}
      {setup && (
        <section className="play-setup">
          <form onSubmit={create}>
            <h2 ref={setupHeading} tabIndex={-1}>
              {t('newMatch')}
            </h2>
            <label>
              {t('humanColor')}
              <select
                value={human}
                onChange={(event) => setHuman(Number(event.target.value))}
                disabled={setupDisabled}
              >
                <option value="0">{t('white')}</option>
                <option value="1">{t('black')}</option>
              </select>
            </label>
            <label>
              {t('length')}
              <select
                value={length}
                onChange={(event) => setLength(Number(event.target.value))}
                disabled={setupDisabled}
              >
                <option value="0">{appT('common.moneyGame')}</option>
                {[1, 3, 5, 7, 9, 11, 15, 21, 25].map((n) => (
                  <option key={n} value={n}>
                    {n} {t('points')}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {modeT('mode')}
              <select
                aria-label={modeT('mode')}
                value={mode}
                disabled={setupDisabled}
                onChange={(event) => setMode(event.target.value as PlayMode)}
              >
                {playModes.map((value) => (
                  <option key={value} value={value}>
                    {modeT(value)}
                  </option>
                ))}
              </select>
            </label>
            <p className="play-mode-help">{modeT(`${mode}Help`)}</p>
            {mode === 'tutor' && (
              <p>
                {modeT('threshold')}: {threshold.toFixed(3)}
              </p>
            )}
            {mode === 'training' && (
              <label className="play-mode-checkbox">
                <input
                  type="checkbox"
                  checked={automatic}
                  onChange={(event) => setAutomatic(event.target.checked)}
                  disabled={setupDisabled}
                />
                {modeT('automatic')}
              </label>
            )}
            <p className="play-engine">
              {settingsT('playEngine')}:{' '}
              {(configuration?.current?.play.engine ??
                engineDefaults?.play_engine) === 'bgsage'
                ? 'Open Sage'
                : 'GNUbg'}{' '}
              ·{' '}
              {configuration?.current?.play.plies ??
                engineDefaults?.play_plies ??
                0}
              -ply
              {' · '}
              {settingsT('analysisEngine')}:{' '}
              {(configuration?.current?.analysis.engine ??
                engineDefaults?.analysis_engine) === 'bgsage'
                ? 'Open Sage'
                : 'GNUbg'}{' '}
              ·{' '}
              {configuration?.current?.analysis.plies ??
                engineDefaults?.analysis_plies ??
                2}
              -ply
            </p>
            {defaultsError && <p role="status">{modeT('defaultsError')}</p>}
            <ActionButton
              type="submit"
              disabled={setupDisabled}
              icon="play"
              label={t('start')}
            >
              {t('start')}
            </ActionButton>
            {session && (
              <ActionButton
                type="button"
                onClick={resumeMatch}
                disabled={uiBusy}
                icon="resume"
                label={t('resumeMatch')}
              >
                {t('resumeMatch')}
              </ActionButton>
            )}
          </form>
        </section>
      )}
      {session && !setup && (
        <>
          <div className="play-match-heading">
            <div className="play-match-summary">
              <h2>
                {name(0)} {formatNumber(session.scores[0])} :{' '}
                {formatNumber(session.scores[1])} {name(1)}
              </h2>
              <p className="play-match-format">
                {session.match_length
                  ? appT('board.points', { count: session.match_length })
                  : appT('common.moneyGame')}{' '}
                · {t('game')} {formatNumber(session.game_no)}
              </p>
              <p className="play-match-status" role="status">
                {turnStatus()}
              </p>
            </div>
            <div
              className="play-mode-slot"
              aria-hidden={Boolean(interruption)}
              inert={Boolean(interruption)}
            >
              {modeControl}
            </div>
            <p className="play-engine play-match-engine">
              <EngineLabel
                details={session.engine.version}
                tooltip={[
                  session.engine.version,
                  `${session.engine.plies}-ply`,
                  feedbackMode !== 'competition' &&
                    `${modeT('analysisDepth')}: ${session.analysis_plies ?? 2}-ply`,
                ].filter(Boolean).join(' · ')}
              />
              {' · '}
              {session.bearoff_direction === 'left' ? t('left') : t('right')}
            </p>
          </div>
          <div className="play-layout">
            <PlayHistory
              history={session.history}
              gameResults={session.game_results}
              white={name(0)}
              black={name(1)}
              sessionId={session.id}
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
                  <ActionButton
                    ref={learningReviewTrigger}
                    tooltip={modeT('reviewTooltip')}
                    compact
                    disabled={uiBusy || (paused && !session.review)}
                    icon="analysis"
                    label={modeT('review')}
                    onClick={() => {
                      if (session.review) setLearningReviewOpen(true);
                      else
                        void run(
                          () => reviewPlaySession(session),
                          () => {
                            if (soundsVisible.current)
                              setLearningReviewOpen(true);
                          },
                        );
                    }}
                  >
                    {modeT('review')}
                  </ActionButton>
                ) : undefined
              }
            />
            <section className="play-table">
              {visible &&
                !setup &&
                (!interruption || moneyResultDialog) &&
                learningReviewOpen &&
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
                    onClose={() => setLearningReviewOpen(false)}
                  />
                )}
              <div
                className="play-board play-live-board"
                data-board-actions={showBoardActions}
                data-play-phase={paused ? undefined : session.phase}
                ref={boardAnchor}
                style={
                  {
                    '--play-flip-x': flipX,
                    '--play-flip-y': flipY,
                  } as CSSProperties
                }
                data-play-review={review.index ?? undefined}
                data-play-animation={
                  playback.frame?.kind ?? (pendingRoll ? 'dice' : 'idle')
                }
                data-animation-player={playback.frame?.player}
                data-animation-dice={playback.frame?.dice.join('-')}
                data-direction={session.bearoff_direction}
                data-human-player={session.human_player}
              >
                {pauseWarning && (
                  <PlayBoardDialog
                    anchor={boardAnchor}
                    heading={t('pauseMatch')}
                    busy={uiBusy}
                    returnFocus={pauseTrigger}
                    onClose={closePauseWarning}
                  >
                    <p>{t('pauseDraftHelp')}</p>
                    {errorNotice}
                    <div className="play-actions">
                      <ActionButton
                        type="button"
                        icon="close"
                        label={libraryT('cancel')}
                        disabled={uiBusy}
                        onClick={closePauseWarning}
                      />
                      <ActionButton
                        type="button"
                        className="primary"
                        icon="pause"
                        label={t('pauseMatch')}
                        disabled={uiBusy}
                        onClick={pauseMatch}
                      />
                    </div>
                  </PlayBoardDialog>
                )}
                {review.index !== null && (
                  <section className="play-history-review play-board-toolbar">
                    <div className="play-history-review-heading">
                      <strong>
                        {t('reviewing')} {formatNumber(review.index + 1)}
                        {historical
                          ? ` · ${t('game')} ${formatNumber(historical.game_no)}`
                          : ''}
                      </strong>
                      <ActionButton
                        type="button"
                        onClick={review.reset}
                        icon="resume"
                        label={t('returnToGame')}
                      >
                        {t('returnToGame')}
                      </ActionButton>
                    </div>
                    {review.loading && (
                      <p role="status">{t('loadingHistory')}</p>
                    )}
                    {review.error !== undefined && (
                      <div role="alert" className="play-error">
                        <p>{t('historyError')}</p>
                        <p>{formatError(review.error)}</p>
                        <ActionButton
                          onClick={() => load(session.id)}
                          icon="refresh"
                          label={t('retry')}
                        >
                          {t('retry')}
                        </ActionButton>
                      </div>
                    )}
                    {historical && (
                      <div
                        className="board-view-buttons text-navigation text-navigation--compact"
                        role="group"
                        aria-label={reviewText('views')}
                      >
                        {(['before', 'arrows', 'after'] as BoardView[])
                          .filter(
                            (mode) =>
                              historical.kind === 'checker' ||
                              mode !== 'arrows',
                          )
                          .map((mode) => (
                            <button
                              type="button"
                              key={mode}
                              aria-pressed={historyView === mode}
                              aria-keyshortcuts={
                                mode === 'before'
                                  ? 'ArrowLeft'
                                  : mode === 'after'
                                    ? 'ArrowRight'
                                    : undefined
                              }
                              onClick={() => review.setView(mode)}
                            >
                              {reviewText(mode)}
                            </button>
                          ))}
                      </div>
                    )}
                  </section>
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
                    errorNotice={errorNotice}
                    onContinue={continueDecision}
                    onRetry={() => feedback('retry')}
                    onReveal={() => feedback('reveal')}
                  />
                )}
                {showBoardActions && (
                  <div
                    className="play-board-toolbar"
                    data-testid="play-board-toolbar"
                    role="group"
                    aria-label={t(beforeRoll ? 'nextRoll' : 'move')}
                  >
                    {beforeRoll ? (
                      <>
                        <ActionButton
                          className="primary play-main-action"
                          data-play-action="roll"
                          disabled={!canRoll}
                          onClick={rollTurn}
                          icon="roll"
                          label={t('roll')}
                        />
                        <ActionButton
                          data-play-action="double"
                          disabled={!canRoll || !session.can_double}
                          aria-describedby={
                            !session.can_double ? doubleReasonId : undefined
                          }
                          onClick={doubleCube}
                          icon="double"
                          label={t('double')}
                        />
                        {!session.can_double && (
                          <p
                            id={doubleReasonId}
                            className="play-double-unavailable"
                          >
                            {doubleUnavailableReason}
                          </p>
                        )}
                      </>
                    ) : (
                      <div className="play-confirm-slot">
                        {completed ? (
                          <ActionButton
                            className="primary play-main-action"
                            disabled={!canConfirm}
                            aria-describedby={turnNotationId}
                            onClick={confirmTurn}
                            icon="confirm"
                            label={t('confirm')}
                          />
                        ) : (
                          <h3>{t('move')}</h3>
                        )}
                      </div>
                    )}
                    {!competition && (!beforeRoll || session.can_double) && (
                      <ActionButton
                        ref={hintTrigger}
                        className="play-hint-button"
                        title={modeT('hint')}
                        disabled={uiBusy}
                        onClick={() => feedback('hint')}
                        icon="hint"
                        label={modeT('hint')}
                      >
                        {modeT('hint')}
                      </ActionButton>
                    )}
                    {!beforeRoll && (
                      <ActionButton
                        type="button"
                        className="play-undo-button"
                        disabled={uiBusy || !canMove || !steps.length}
                        onClick={undo}
                        icon="undo"
                        label={t('undo')}
                      >
                        <span>{t('undo')}</span>
                        {steps.length > 0 && (
                          <span className="play-undo-count" aria-hidden="true">
                            {steps.length}
                          </span>
                        )}
                      </ActionButton>
                    )}
                    <ActionButton
                      ref={resignTrigger}
                      type="button"
                      className="play-resign-button"
                      disabled={uiBusy}
                      onClick={openResign}
                      icon="resign"
                      label={resignT('resign')}
                    />
                    {steps.length > 0 && (
                      <p id={turnNotationId} className="play-selected-steps">
                        {steps
                          .map(
                            (step) =>
                              `${step.from === 0 || step.from === 25 ? t('bar') : relativePoint(step.from, session.human_player)}/${step.to < 0 || step.to > 25 ? t('off') : relativePoint(step.to, session.human_player)}`,
                          )
                          .join(' ')}
                      </p>
                    )}
                    {showBoardError && errorNotice}
                  </div>
                )}
                <Board
                  position={
                    historicalPosition ?? {
                      ...session.position,
                      dice,
                      player: playback.frame?.player ?? session.position.player,
                      board:
                        playback.frame?.before ??
                        applySteps(
                          session.position.board,
                          selectedSteps,
                          session.human_player,
                        ),
                    }
                  }
                  white={name(0)}
                  black={name(1)}
                  showPipCounts={
                    !competition ||
                    session.phase === 'match_over' ||
                    moneyFinished
                  }
                  theme={theme}
                  animation={review.active ? null : playback.frame}
                  preview={historicalProjection}
                  previewMode={historical ? historyView : undefined}
                  interaction={{
                    enabled: sources,
                    sources,
                    destinations: hoverDestinations,
                    board: displayedBoard,
                    onHover: setHovered,
                    cube: {
                      enabled: canRoll && session.can_double,
                      label: session.can_double
                        ? t('cube')
                        : t('cubeUnavailable'),
                      activate: doubleCube,
                    },
                    dice: {
                      canRoll,
                      preRoll:
                        !review.active &&
                        session.phase === 'roll' &&
                        !playback.frame &&
                        pendingRoll !== 'roll',
                      roll: rollTurn,
                      opening: openingPhase
                        ? {
                            values:
                              openingFrame?.dice ??
                              (pendingRoll === 'opening_roll'
                                ? waitingFaces
                                : session.phase === 'opening_result' &&
                                    session.opening
                                  ? [
                                      session.opening.white_die,
                                      session.opening.black_die,
                                    ]
                                  : [null, null]),
                            labels: [
                              name(0) + ' · ' + t('white'),
                              name(1) + ' · ' + t('black'),
                            ],
                            unrolled: t('openingNotRolled'),
                          }
                        : undefined,
                      canSwap,
                      canConfirm,
                      confirm: confirmTurn,
                      rolling: Boolean(
                        playback.frame?.rolling ||
                        pendingRoll === 'roll' ||
                        pendingRoll === 'opening_roll',
                      ),
                      swap: swapDice,
                      label:
                        t(
                          canRoll
                            ? 'roll'
                            : pendingRoll
                              ? 'rolling'
                              : canConfirm
                                ? 'confirm'
                                : 'swapDice',
                        ) +
                        ' · ' +
                        dice.join(', '),
                      used:
                        review.active || playback.frame || pendingRoll
                          ? [false, false]
                          : dice[0] === dice[1]
                            ? [
                                Math.min(1, steps.length / 2),
                                Math.max(0, (steps.length - 2) / 2),
                              ]
                            : [steps.length >= 1, steps.length >= 2],
                      active:
                        playback.frame || pendingRoll || !canMove || completed
                          ? null
                          : dice[0] === dice[1]
                            ? steps.length < 2
                              ? 0
                              : 1
                            : steps.length,
                    },
                    human: session.human_player,
                    onPoint: point,
                    label: (p) =>
                      p === -1 || p === 26
                        ? t('off') + ' · ' + name(p === -1 ? 0 : 1)
                        : p === 0 || p === 25
                          ? t('bar') + ' · ' + name(p === 25 ? 0 : 1)
                          : t('point') +
                            ' ' +
                            relativePoint(p, session.human_player),
                  }}
                />
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
                    onConfirm={(level) =>
                      action('resign', undefined, undefined, level)
                    }
                  />
                )}
                {interruption === 'hint' && (
                  <PlayBoardDialog
                    anchor={boardAnchor}
                    heading={modeT('hint')}
                    busy={uiBusy}
                    className={`play-hint-dialog play-analysis-dialog${hintReady && hasPlayMovePreview(session) ? ' play-analysis-dialog--checker' : ''}`}
                    onClose={closeHint}
                  >
                    {errorNotice}
                    {busy && <p role="status">{t('hintLoading')}</p>}
                    {!busy && hintReady && (
                      <PlayHintView
                        key={`${session.id}:${pending?.id}`}
                        session={session}
                        theme={theme}
                        white={name(0)}
                        black={name(1)}
                      />
                    )}
                  </PlayBoardDialog>
                )}
                {cubeDecisionOpen && (
                  <PlayCubeDecision
                    anchor={boardAnchor}
                    cube={matchContext(session.position.gnubg_match_id).cube}
                    busy={uiBusy}
                  >
                    {errorNotice}
                    {feedbackPanel}
                    {!awaitingFeedback && (
                      <div className="play-actions play-cube-choices">
                        <ActionButton
                          className="primary"
                          disabled={uiBusy}
                          onClick={() => action('take')}
                          icon="take"
                          label={t('take')}
                        >
                          {t('take')}
                        </ActionButton>
                        <ActionButton
                          disabled={uiBusy}
                          onClick={() => action('drop')}
                          icon="drop"
                          label={t('drop')}
                        >
                          {t('drop')}
                        </ActionButton>
                        {!competition && (
                          <ActionButton
                            disabled={uiBusy}
                            onClick={() => feedback('hint')}
                            icon="hint"
                            label={modeT('hint')}
                          >
                            {modeT('hint')}
                          </ActionButton>
                        )}
                      </div>
                    )}
                    <div className="play-actions play-cube-choices">
                      {pauseButton}
                    </div>
                    {dialogPreferences}
                  </PlayCubeDecision>
                )}
                {interruption &&
                  !cubeDecisionOpen &&
                  interruption !== 'resign' &&
                  interruption !== 'hint' && (
                    <PlayBoardDialog
                      key={interruption}
                      className={
                        interruption === 'feedback' &&
                        hasPlayMovePreview(session)
                          ? 'play-analysis-dialog play-analysis-dialog--checker'
                          : moneyFinished && interruption === 'game_over'
                            ? 'play-money-result-dialog'
                            : undefined
                      }
                      anchor={boardAnchor}
                      busy={uiBusy}
                      heading={
                        interruption === 'feedback'
                          ? modeT(feedbackMode)
                          : interruption === 'opening'
                            ? t(
                                session.phase as
                                  'opening_roll' | 'opening_result',
                              )
                            : interruption === 'match_over' &&
                                session.money_session_ended
                              ? t('sessionEnded')
                              : t(interruption)
                      }
                    >
                      {errorNotice}
                      {feedbackPanel}
                      {interruption === 'opening' && (
                        <PlayOpeningRoll
                          stage={
                            session.phase as 'opening_roll' | 'opening_result'
                          }
                          opening={session.opening}
                          white={name(0)}
                          black={name(1)}
                          busy={uiBusy}
                          tied={openingFrame?.tied && !openingFrame?.rolling}
                          onRoll={() => action('opening_roll')}
                          onContinue={() => action('start_opening_move')}
                        />
                      )}

                      {interruption === 'pass' && (
                        <ActionButton
                          className="primary"
                          disabled={uiBusy}
                          onClick={() => action('pass')}
                          icon="pass"
                          label={t('pass')}
                        >
                          {t('pass')}
                        </ActionButton>
                      )}
                      {(interruption === 'game_over' ||
                        interruption === 'match_over') && (
                        <>
                          {currentResult ? (
                            <GameResultSummary
                              result={currentResult}
                              white={name(0)}
                              black={name(1)}
                            />
                          ) : (
                            session.winner != null && (
                              <p>
                                {t('winner')}: {name(session.winner)}
                              </p>
                            )
                          )}
                          <div className="play-actions play-cube-choices">
                            {interruption === 'game_over' ? (
                              <>
                                <ActionButton
                                  className="primary"
                                  disabled={
                                    uiBusy ||
                                    (moneyFinished && !session.review && !error)
                                  }
                                  onClick={() => action('next_game')}
                                  icon="new-game"
                                  label={t('next_game')}
                                >
                                  {t('next_game')}
                                </ActionButton>
                                {pauseButton}
                              </>
                            ) : (
                              <>
                                <ActionButton
                                  className="primary"
                                  disabled={uiBusy}
                                  onClick={() => {
                                    setAcknowledgedMatch(matchResultKey);
                                    openMatchSetup();
                                  }}
                                  icon="new-game"
                                  label={t('newMatch')}
                                >
                                  {t('newMatch')}
                                </ActionButton>
                                <ActionButton
                                  disabled={uiBusy}
                                  onClick={() =>
                                    setAcknowledgedMatch(matchResultKey)
                                  }
                                  icon="back"
                                  label={t('backToBoard')}
                                >
                                  {t('backToBoard')}
                                </ActionButton>
                              </>
                            )}
                            {moneyFinished && (
                              <ActionButton
                                ref={moneyReviewTrigger}
                                icon="analysis"
                                label={modeT('reviewTooltip')}
                                disabled={uiBusy || !session.review}
                                onClick={() => setLearningReviewOpen(true)}
                              >
                                {modeT('reviewTooltip')}
                              </ActionButton>
                            )}
                          </div>
                        </>
                      )}
                      {interruption !== 'game_over' &&
                        interruption !== 'match_over' && (
                          <div className="play-actions play-cube-choices">
                            {pauseButton}
                          </div>
                        )}
                      {dialogPreferences}
                    </PlayBoardDialog>
                  )}
              </div>
              <div className="play-turn-status">
                <div className="play-turn-reserved" aria-hidden="true">
                  <h3>{t('move')}</h3>
                  <p>{t('select')}</p>
                  <p>&nbsp;</p>
                </div>
                {!busy &&
                  !paused &&
                  !review.active &&
                  !interruption &&
                  !openingPhase &&
                  !awaitingFeedback &&
                  session.phase !== 'double_response' &&
                  session.phase !== 'roll' && (
                    <div className="play-turn" aria-live="polite">
                      {session.winner != null &&
                        ['game_over', 'match_over'].includes(session.phase) && (
                          <p>
                            {t('winner')}: {name(session.winner)}
                          </p>
                        )}
                      {canMove && !awaitingFeedback && !forcedPass && (
                        <p>
                          {completed
                            ? t('ready')
                            : !moves.length
                              ? t('swapFirstDie')
                              : t('select')}
                        </p>
                      )}
                      <div className="play-actions">
                        {canMove && forcedPass && (
                          <ActionButton
                            disabled={uiBusy}
                            onClick={() => action('pass')}
                            icon="pass"
                            label={t('pass')}
                          >
                            {t('pass')}
                          </ActionButton>
                        )}
                        {session.phase === 'game_over' && (
                          <ActionButton
                            disabled={uiBusy}
                            onClick={() => action('next_game')}
                            icon="new-game"
                            label={t('next_game')}
                          >
                            {t('next_game')}
                          </ActionButton>
                        )}
                      </div>
                    </div>
                  )}
              </div>
            </section>
          </div>
        </>
      )}
    </div>
  );
}
