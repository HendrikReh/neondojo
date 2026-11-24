import { useDecimalInput } from '../../shared/format/decimal-input';
import {
  playStrengthLabel,
  useSettingsText,
} from '../../settings/settings-messages';
import { loadEngines, type EngineInfo } from '../../settings/settings-api';
import Tooltip from '../../shared/ui/Tooltip';
import ActionButton from '../../shared/ui/ActionButton';
import {
  useEffect,
  useId,
  useRef,
  useMemo,
  useState,
  type CSSProperties,
  type FormEvent,
} from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import Board from '../../shared/board/Board';
import PlayHistory from './PlayHistory';
import PlayFeedback from './PlayFeedback';
import PlayCoachFeedback from './PlayCoachFeedback';
import PlayHintView from './PlayHintView';
import { hasPlayMovePreview } from './PlayMovePreview';
import LearningReviewDialog from '../study/reports/LearningReviewDialog';
import {
  matchAssessmentReportId,
  studyAssessment,
} from '../study/reports/match-assessment-api';
import { useMatchAssessmentText } from '../study/reports/match-assessment-messages';
import {
  playModes,
  usePlayModeText,
  type PlayMode,
} from './play-mode-messages';
import type { AppSettings } from '../../settings/settings-api';
import { usePlayReview } from './usePlayReview';
import { projectMove, type MovePreview } from '../../shared/board/move-preview';
import type { Theme } from '../../shared/board/themes';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import { usePlayText } from './play-messages';
import { useLibraryText } from '../library/library-messages';
import {
  nextCheckerSteps,
  applySteps,
  boardFlips,
  matchingCheckerTurns,
  relativePoint,
  type CheckerStepWithDie,
} from '../../shared/board/play-state';
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
import {
  playSounds,
  stopPlaySounds,
  unlockPlayAudio,
} from '../../shared/board/play-audio';
import { isNewCubeOffer } from '../../shared/board/play-sound-events';
import { matchContext } from '../../shared/board/boardMath';
import PlayCubeDecision from './PlayCubeDecision';
import PlayBoardDialog from '../../shared/board/PlayBoardDialog';
import PlayResignDialog, { type ResignLevel } from './PlayResignDialog';
import { usePlayResignText } from './play-resign-messages';
import GameResultSummary from './GameResultSummary';
import {
  actionFrames,
  checkerFrame,
  reverseFrame,
} from '../../shared/board/play-animation';
import { usePlayAnimation, useRollingFaces } from './usePlayAnimation';
import PlaySetupForm from './PlaySetupForm';
import { usePlaySetupOptions } from './usePlaySetupOptions';
import PlayMatchSummary from './PlayMatchSummary';
import PlayHistoryReviewToolbar from './PlayHistoryReviewToolbar';
import PlayTurnToolbar from './PlayTurnToolbar';
import './play.css';
const storageKey = 'neondojo.play-session';
function storedSession() {
  try {
    return localStorage.getItem(storageKey);
  } catch {
    return null;
  }
}
function forgetStoredSession(id: string) {
  try {
    if (localStorage.getItem(storageKey) === id)
      localStorage.removeItem(storageKey);
  } catch {
    /* Nothing is restored automatically without storage. */
  }
}
/** Starting position on the empty table behind the setup dialog. */
const idlePosition: PlaySession['position'] = {
  id: '',
  match_uuid: '',
  ordinal: 0,
  game_no: 1,
  ply: 0,
  player: 0,
  dice: [],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  // Centered cube at 1, no score: the empty table carries no match context.
  gnubg_match_id: 'MAAAAAAAAAAA',
  notation: '',
};
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
  const focusSetup = useRef(false);
  const [pauseWarning, setPauseWarning] = useState(false);
  const autoMoneyReviews = useRef(new Set<string>());
  const [learningReviewOpen, setLearningReviewOpen] = useState(false);
  const menuId = useId();
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const resignTrigger = useRef<HTMLButtonElement>(null);
  const restoreResignFocus = useRef(false);
  const [resignOpen, setResignOpen] = useState(false);
  const forfeitTrigger = useRef<HTMLButtonElement>(null);
  const [forfeitOpen, setForfeitOpen] = useState(false);
  const restoreHintFocus = useRef(false);
  const [acknowledgedMatch, setAcknowledgedMatch] = useState<string | null>(
    null,
  );
  const [hintOpen, setHintOpen] = useState(false);
  const [hintReady, setHintReady] = useState(false);
  const [pendingRoll, setPendingRoll] = useState<PlayAction | null>(null);
  const waitingFaces = useRollingFaces(pendingRoll !== null);
  const soundsVisible = useLatest(visible);
  useEffect(() => {
    if (!visible) {
      stopPlaySounds();
      setHintOpen(false);
      setResignOpen(false);
      setForfeitOpen(false);
      setPauseWarning(false);
      setLearningReviewOpen(false);
    }
    return () => stopPlaySounds();
  }, [visible]);
  const t = usePlayText(),
    { formatError, locale, t: appT } = useI18n();
  const modeT = usePlayModeText();
  const libraryT = useLibraryText();
  const resignT = usePlayResignText();
  const [session, setSession] = useState<PlaySession | null>(null);
  const assessmentReportId =
    visible &&
    session?.phase === 'match_over' &&
    session.match_length > 0 &&
    session.review
      ? matchAssessmentReportId(session)
      : undefined;
  const [savedAssessment, setSavedAssessment] = useState<string>();
  const assessmentKey = assessmentReportId
    ? `${assessmentReportId}:${locale}`
    : undefined;
  useEffect(() => {
    if (!assessmentReportId || !assessmentKey) return;
    const abort = new AbortController();
    void studyAssessment(assessmentReportId, locale, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted)
          setSavedAssessment(
            result.job && result.job.status !== 'failed'
              ? assessmentKey
              : undefined,
          );
      })
      .catch(() => {
        if (!abort.signal.aborted) setSavedAssessment(undefined);
      });
    return () => abort.abort();
  }, [assessmentReportId, assessmentKey, locale]);
  const assessmentAction = assessmentT(
    assessmentKey && savedAssessment === assessmentKey ? 'open' : 'create',
  );
  const paused = Boolean(session?.paused);
  const moneyFinished =
    session?.match_length === 0 &&
    ['game_over', 'match_over'].includes(session.phase);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(),
    // A saved match is restored first; without one, the setup dialog opens.
    [setup, setSetup] = useState(() => storedSession() === null);
  const review = usePlayReview(session, visible && !setup);
  useEffect(() => {
    if (visible && paused && !busy && !setup && focusResume.current)
      resumeTrigger.current?.focus({ preventScroll: true });
    if (!visible || !busy) focusResume.current = false;
  }, [visible, paused, busy, setup]);
  const settingsT = useSettingsText();
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
  const [idleFlipX, idleFlipY] = boardFlips(
    human,
    confirmed?.appearance.bearoff_direction ??
      settings?.bearoff_direction ??
      'right',
  );
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  useEffect(() => {
    if (!setupOpen) return;
    const abort = new AbortController();
    void loadEngines(abort.signal)
      .then((value) => {
        if (!abort.signal.aborted) setEngines(value);
      })
      .catch(() => {
        /* Engine availability is advisory; creating the match still verifies it. */
      });
    return () => abort.abort();
  }, [setupOpen]);
  const [liveThreshold, setLiveThreshold] = useState('0.04');
  const thresholdInput = useDecimalInput({
    value: liveThreshold,
    onChange: (value) => {
      setThresholdStatus('idle');
      setLiveThreshold(value);
    },
    step: 0.01,
    min: 0,
    max: 1,
    onKeyDown: (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        event.currentTarget.blur();
      }
    },
  });
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
  const [steps, setSteps] = useState<CheckerStepWithDie[]>([]);
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
      setForfeitOpen(false);
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
    load(id, requested ? requestedSession.resume : false, !requested);
  }, [visible, requestedSession, busy]);
  /** `restoring` marks the automatic restore of the last stored session. */
  function load(id: string, resume = false, restoring = false) {
    if (locked.current && !initialLoad.current) return;
    setSetup(false);
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
        if (abort.signal.aborted || token !== generation.current) return;
        // A deleted session is not an error: forget it and offer a new match.
        if (
          restoring &&
          e instanceof AppError &&
          String(e.params.status) === '404'
        ) {
          forgetStoredSession(id);
          retrySession.current = null;
          setSetup(true);
        } else setError(e);
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
    setSetupOptions({});
    setPauseWarning(false);
    setForfeitOpen(false);
    setSetup(true);
    setError(undefined);
  }
  function closeSetup() {
    if (locked.current) return;
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
      forfeitOpen ||
      review.active ||
      error !== undefined
    )
      return;
    if (session?.phase === 'opening_roll') action('opening_roll');
    if (session?.phase === 'opening_result') action('start_opening_move');
  }, [visible, setup, busy, forfeitOpen, error, review.active, session]);

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
  function closeForfeit() {
    if (locked.current) return;
    setForfeitOpen(false);
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
  const feedbackMode = session?.play_mode ?? 'competition';
  const competition = feedbackMode === 'competition';
  const pendingRoutineRoll =
    pending?.action === 'roll' &&
    !pending.warning &&
    (feedbackMode === 'training' ||
      (feedbackMode === 'coach' && pending.severity === 'none'))
      ? pending
      : null;
  const awaitingFeedback = Boolean(pending?.action && !pendingRoutineRoll);
  const selectedSteps =
    !paused && pending?.action === 'move' && pending.move_index != null
      ? (session?.legal_moves[pending.move_index]?.steps ?? [])
      : steps;
  const uiBusy = busy || playback.animating;
  const setupDisabled = uiBusy;
  const canRoll = Boolean(
    visible &&
    !setup &&
    !paused &&
    !review.active &&
    !hintOpen &&
    !resignOpen &&
    !awaitingFeedback &&
    session?.phase === 'roll',
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
  const botRoll = useMemo(() => {
    if (session?.phase !== 'roll') return undefined;
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
  }, [session]);
  const savedDice = session?.position.dice;
  const turnDice = useMemo(
    () => (savedDice && swapped ? [...savedDice].reverse() : (savedDice ?? [])),
    [savedDice, swapped],
  );
  const previousRoll =
    !review.active && !playback.frame && !pendingRoll ? botRoll : undefined;
  const dice =
    playback.frame && !openingFrame
      ? playback.frame.dice
      : pendingRoll === 'roll'
        ? waitingFaces
        : openingPhase
          ? []
          : turnDice;
  const moves = session?.legal_moves.length ? (session.legal_turns ?? []) : [];
  const missingCheckerPaths =
    canMove && !moves.length && !!session?.legal_moves.length;
  const continuations = canMove ? matchingCheckerTurns(moves, steps) : [];
  const choices = nextCheckerSteps(
    continuations,
    steps.length,
    turnDice[steps.length],
  );
  const sources = [...choices.keys()];
  const hoverStep = hovered === null ? undefined : choices.get(hovered);
  const hoverDestinations = hoverStep ? [hoverStep.to] : [];
  const completed = continuations.find(
    (move) => move.steps.length === steps.length && steps.length > 0,
  );
  const forcedPass = canMove && session?.legal_moves.length === 0;
  const canPass =
    forcedPass &&
    visible &&
    !setup &&
    !uiBusy &&
    !pendingRoll &&
    !pauseWarning &&
    !forfeitOpen;
  const isDouble = turnDice[0] === turnDice[1];
  const usedDice = isDouble
    ? [Math.min(1, steps.length / 2), Math.max(0, (steps.length - 2) / 2)]
    : turnDice.map((die) => steps.some((step) => step.die === die));
  const canSwap =
    canMove &&
    !forcedPass &&
    !playback.animating &&
    steps.length === 0 &&
    turnDice.length === 2 &&
    !isDouble;
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
    !hintOpen &&
    !resignOpen &&
    !forfeitOpen &&
    !review.active;
  const canConfirm =
    (canMove && !uiBusy && Boolean(completed)) ||
    (canContinueCoachTurn && pendingCoachTurn?.action === 'move');
  function confirmTurn() {
    if (canContinueCoachTurn && pendingCoachTurn?.action === 'move')
      continueDecision();
    else if (canConfirm && completed) action('move', completed.move_index);
  }
  function rollTurn() {
    if (boardRollReady) action('roll', undefined, pendingRoutineRoll?.id);
  }
  function doubleCube() {
    if (boardRollReady && session?.can_double) action('double');
  }
  function swapDice() {
    if (!canSwap || review.isActive()) return;
    setSwapped((value) => !value);
    setHovered(null);
  }
  function point(point: number) {
    const step = choices.get(point);
    if (!session || !step || review.isActive()) return;
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
    visible && !setup && !paused && feedbackMode === 'coach' && awaitingFeedback
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
                : session?.phase === 'game_over'
                  ? 'game_over'
                  : moneyFinished && !session?.money_session_ended
                    ? 'game_over'
                    : session?.phase === 'match_over' &&
                        acknowledgedMatch !== matchResultKey
                      ? 'match_over'
                      : null;
  const returnToTurnControl = restoreResignFocus.current
    ? resignTrigger
    : restoreHintFocus.current
      ? hintTrigger
      : undefined;
  useEffect(() => {
    if (!visible || setup) {
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    } else if (!hintOpen && !resignOpen && !uiBusy) {
      if (!interruption)
        returnToTurnControl?.current?.focus({ preventScroll: true });
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    }
  }, [
    visible,
    setup,
    hintOpen,
    resignOpen,
    uiBusy,
    interruption,
    returnToTurnControl,
  ]);
  const boardRollReady = canRoll && !uiBusy && !interruption && !pauseWarning;
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
  const turnHeading = beforeRoll ? 'yourTurn' : forcedPass ? 'pass' : 'move';
  const canForfeit =
    session &&
    session.match_length > 0 &&
    !paused &&
    session.phase !== 'match_over';
  const forfeitButton = () =>
    canForfeit && (
      <ActionButton
        type="button"
        icon="resign"
        ref={forfeitTrigger}
        label={resignT('forfeitMatch')}
        disabled={uiBusy || review.active}
        aria-haspopup="dialog"
        onClick={() => {
          setError(undefined);
          setForfeitOpen(true);
        }}
      />
    );
  // Resigning sits beside Pause match so it is found without opening the menu.
  const resignButton = () =>
    session &&
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
  const hasPauseDraft = steps.length > 0 && pending?.action !== 'move';
  function closePauseWarning() {
    if (locked.current) return;
    setPauseWarning(false);
    setError(undefined);
  }
  const pauseButton = () =>
    session &&
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
  const hintButton = () => (
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
  function closeMenu() {
    menu.current?.hidePopover();
  }
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
  const decisionError = forfeitOpen ? null : errorNotice;
  const modeControl = session && (
    <div className="play-mode-control">
      <label>
        {modeT('mode')}
        <Tooltip
          as="select"
          text={modeT(`${feedbackMode}Help`)}
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
        </Tooltip>
      </label>
      {feedbackMode === 'tutor' && (
        <div className="play-threshold-control">
          <label>
            {modeT('threshold')}
            <Tooltip
              as="input"
              aria-label={modeT('threshold')}
              text={modeT('liveThresholdHelp')}
              {...thresholdInput.inputProps}
              required
              disabled={
                uiBusy ||
                paused ||
                session.phase === 'match_over' ||
                moneyFinished
              }
              onChange={(event) => thresholdInput.change(event.target.value)}
              onBlur={saveThreshold}
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
    if (openingFrame && !openingFrame.rolling)
      return openingFrame.tied
        ? t('openingTie')
        : appT('app.toMove', { player: name(openingFrame.player) });
    if (pendingRoll || frame?.kind === 'dice')
      return openingPhase
        ? t('openingRolling')
        : `${name(frame?.player ?? session.human_player)} · ${t('rolling')}`;
    if (busy && !hintOpen)
      return openingPhase ? t('openingPreparing') : t('thinking');
    if (awaitingFeedback) return modeT('unconfirmed');
    if (session.money_session_ended) return t('sessionEnded');
    if (session.match_forfeited) return resignT('matchForfeited');
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
        !pauseWarning &&
        !coachDecision &&
        !forfeitOpen &&
        errorNotice}
      {!session && (
        // The empty table stays visible behind the setup dialog.
        <div
          className="play-board play-idle-board"
          inert={setupOpen}
          ref={boardAnchor}
          data-human-player={human}
          style={
            {
              '--play-flip-x': idleFlipX,
              '--play-flip-y': idleFlipY,
            } as CSSProperties
          }
        >
          <Board
            position={idlePosition}
            white={t(human === 0 ? 'you' : 'bot')}
            black={t(human === 1 ? 'you' : 'bot')}
            theme={theme}
          />
        </div>
      )}
      {setupOpen && (
        <PlayBoardDialog
          anchor={boardAnchor}
          heading={t('newMatch')}
          busy={uiBusy}
          className="play-setup-dialog"
          modal={false}
          focusOnOpen={focusSetup.current}
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
            <div className="play-actions play-match-actions">
              {pauseButton() && (
                <span
                  className="play-pause-slot"
                  aria-hidden={Boolean(interruption)}
                  inert={Boolean(interruption)}
                >
                  {resignButton()}
                  {pauseButton()}
                </span>
              )}
              {paused && (
                <ActionButton
                  ref={resumeTrigger}
                  className="primary"
                  onClick={resumeMatch}
                  disabled={uiBusy}
                  icon="resume"
                  label={t('resumeMatch')}
                />
              )}
              <ActionButton
                ref={menuTrigger}
                type="button"
                className="play-menu-button"
                icon="more"
                label={t('moreActions')}
                compact
                popoverTarget={menuId}
                disabled={uiBusy}
              />
              <div
                id={menuId}
                ref={menu}
                popover="auto"
                role="group"
                aria-label={t('moreActions')}
                className="play-menu"
                onKeyDown={(event) => {
                  if (event.key !== 'Escape') return;
                  event.preventDefault();
                  event.stopPropagation();
                  closeMenu();
                  menuTrigger.current?.focus();
                }}
              >
                {canForfeit && !interruption && (
                  <ActionButton
                    type="button"
                    icon="resign"
                    label={resignT('forfeitMatch')}
                    disabled={uiBusy || review.active}
                    aria-haspopup="dialog"
                    onClick={() => {
                      closeMenu();
                      setError(undefined);
                      setForfeitOpen(true);
                    }}
                  />
                )}
                {interruption !== 'match_over' && (
                  <ActionButton
                    type="button"
                    onClick={() => {
                      closeMenu();
                      openMatchSetup();
                    }}
                    disabled={uiBusy}
                    icon="new-game"
                    label={t('newMatch')}
                  />
                )}
                <ActionButton
                  type="button"
                  onClick={() => {
                    closeMenu();
                    onOpenPausedMatches();
                  }}
                  disabled={uiBusy}
                  icon="open"
                  label={libraryT('pausedMatches')}
                />
              </div>
            </div>
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
                  </>
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
                  <PlayHistoryReviewToolbar
                    review={review}
                    index={review.index}
                    view={historyView}
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
                    forcedPass={forcedPass}
                    rollReady={boardRollReady}
                    canPass={canPass}
                    canConfirm={canConfirm}
                    canMove={canMove}
                    pendingRoutineRoll={Boolean(pendingRoutineRoll)}
                    competition={competition}
                    hint={hintButton()}
                    busy={uiBusy}
                    steps={steps}
                    complete={Boolean(completed)}
                    missingCheckerPaths={missingCheckerPaths}
                    doubleReasonId={doubleReasonId}
                    doubleUnavailableReason={doubleUnavailableReason}
                    notationId={turnNotationId}
                    onRoll={rollTurn}
                    onPass={() => action('pass')}
                    onConfirm={confirmTurn}
                    onDouble={doubleCube}
                    onUndo={undo}
                    onRetry={() => load(session.id)}
                  />
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
                    cube: boardRollReady
                      ? {
                          activate:
                            session.can_double && !pendingRoutineRoll
                              ? doubleCube
                              : undefined,
                          label:
                            session.can_double && !pendingRoutineRoll
                              ? t('double')
                              : doubleUnavailableReason,
                          descriptionId: session.can_double
                            ? undefined
                            : doubleReasonId,
                        }
                      : undefined,
                    dice: {
                      previousRoll,
                      roll:
                        boardRollReady && previousRoll ? rollTurn : undefined,
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
                      pass:
                        canContinueCoachTurn &&
                        pendingCoachTurn?.action === 'pass'
                          ? continueDecision
                          : canPass
                            ? () => action('pass')
                            : undefined,
                      descriptionId:
                        canConfirm && steps.length > 0
                          ? turnNotationId
                          : undefined,
                      rolling: Boolean(
                        playback.frame?.rolling ||
                        pendingRoll === 'roll' ||
                        pendingRoll === 'opening_roll',
                      ),
                      swap: swapDice,
                      label:
                        t(
                          previousRoll
                            ? 'roll'
                            : pendingRoll
                              ? 'rolling'
                              : forcedPass ||
                                  pendingCoachTurn?.action === 'pass'
                                ? 'pass'
                                : canConfirm
                                  ? 'confirm'
                                  : 'swapDice',
                        ) +
                        ' · ' +
                        (previousRoll?.dice ?? dice).join(', '),
                      used:
                        review.active || playback.frame || pendingRoll
                          ? [false, false]
                          : previousRoll
                            ? [true, true]
                            : usedDice,
                      active:
                        playback.frame || pendingRoll || !canMove || completed
                          ? null
                          : usedDice.findIndex((used) => Number(used) < 1),
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
                    onForfeit={
                      canForfeit
                        ? () => {
                            closeResign();
                            setForfeitOpen(true);
                          }
                        : undefined
                    }
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
                    {decisionError}
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
                      {pauseButton()}
                      {forfeitButton()}
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
                          : interruption === 'match_over' &&
                              session.money_session_ended
                            ? t('sessionEnded')
                            : t(interruption)
                      }
                    >
                      {decisionError}
                      {feedbackPanel}
                      {(interruption === 'game_over' ||
                        interruption === 'match_over') && (
                        <>
                          {session.match_forfeited ? (
                            <p>{resignT('matchForfeited')}</p>
                          ) : currentResult ? (
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
                                {pauseButton()}
                                {moneyFinished && (
                                  <ActionButton
                                    disabled={uiBusy}
                                    onClick={() => action('end_session')}
                                    icon="confirm"
                                    label={t('endSession')}
                                  >
                                    {t('endSession')}
                                  </ActionButton>
                                )}
                                {forfeitButton()}
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
                            {assessmentButton}
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
                            {pauseButton()}
                            {forfeitButton()}
                          </div>
                        )}
                      {dialogPreferences}
                    </PlayBoardDialog>
                  )}
                {forfeitOpen && canForfeit && (
                  <PlayBoardDialog
                    anchor={boardAnchor}
                    heading={resignT('forfeitMatch')}
                    busy={uiBusy}
                    returnFocus={interruption ? forfeitTrigger : menuTrigger}
                    onClose={closeForfeit}
                  >
                    <p>{resignT('forfeitHelp')}</p>
                    {errorNotice}
                    <div className="play-actions play-cube-choices">
                      <ActionButton
                        type="button"
                        icon="cancel"
                        label={resignT('cancel')}
                        disabled={uiBusy}
                        onClick={closeForfeit}
                      />
                      <ActionButton
                        type="button"
                        icon="resign"
                        label={resignT('forfeitMatch')}
                        className="play-resign-confirm"
                        disabled={uiBusy}
                        onClick={() => action('forfeit_match')}
                      />
                    </div>
                  </PlayBoardDialog>
                )}
              </div>
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
