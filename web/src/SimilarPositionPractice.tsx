import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import ActionButton from './ActionButton';
import Board from './Board';
import PlayBoardDialog from './PlayBoardDialog';
import { request } from './api';
import { useBoardReviewText } from './board-review-messages';
import { matchContext } from './boardMath';
import type { components } from './generated-api';
import { useI18n } from './i18n';
import {
  projectMove,
  type MovePreview,
  type MovePreviewStep,
} from './move-preview';
import { usePlayText } from './play-messages';
import {
  applySteps,
  legalCheckerTurns,
  matchingMoves,
  nextPoints,
  relativePoint,
  type CheckerStep,
} from './play-state';
import { usePositionText } from './position-browser-messages';
import {
  savePracticeSummary,
  type PracticeSummaryInput,
} from './similar-practice-history-api';
import type { Theme } from './themes';

type Hit = components['schemas']['SimilarHit'];
type Answer = components['schemas']['CheckerPracticeAnswer'];
type Score = components['schemas']['CheckerPracticeScore'];
type ScoredMove = { item: Hit; loss: number };
type PendingSave = {
  summary: PracticeSummaryInput;
  failure?: { error: unknown };
};

function startRound(items: Hit[], isRetry = false) {
  return {
    id: crypto.randomUUID(),
    items,
    index: 0,
    scored: [] as ScoredMove[],
    isRetry,
  };
}

export default function SimilarPositionPractice({
  items,
  theme,
  returnFocus,
  onClose,
}: {
  items: Hit[];
  theme: Theme;
  returnFocus: RefObject<HTMLButtonElement | null>;
  onClose: () => void;
}) {
  const p = usePositionText();
  const { formatNumber, formatError } = useI18n();
  const [round, setRound] = useState(() => startRound(items));
  const [pendingSaves, setPendingSaves] = useState<PendingSave[]>([]);
  const saving = useRef(new Set<string>());
  const { items: roundItems, index, scored } = round;
  const item = roundItems[index];
  const progress = useRef<HTMLParagraphElement>(null);
  const completion = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (!item) completion.current?.focus();
    else if (index > 0 || roundItems !== items) progress.current?.focus();
  }, [item, index, roundItems, items]);

  const missed = scored.filter(({ loss }) => loss > 0).map(({ item }) => item);
  const currentSave = pendingSaves.find(
    ({ summary }) => summary.id === round.id,
  );
  const closeButton = (
    <ActionButton
      type="button"
      icon="close"
      label={p('practiceClose')}
      onClick={onClose}
    />
  );

  function advance(loss?: number) {
    if (!item) return;
    const nextScores =
      loss === undefined ? scored : [...scored, { item, loss }];
    setRound((current) =>
      current === round
        ? {
            ...round,
            index: index + 1,
            scored: nextScores,
          }
        : current,
    );
    if (index + 1 === roundItems.length && nextScores.length) {
      void persist({
        id: round.id,
        completed_at: new Date().toISOString(),
        total_positions: roundItems.length,
        losses: nextScores.map(({ loss }) => loss),
        is_retry: round.isRetry,
      });
    }
  }

  async function persist(summary: PracticeSummaryInput) {
    if (saving.current.has(summary.id)) return;
    saving.current.add(summary.id);
    setPendingSaves((pending) => [
      ...pending.filter((save) => save.summary.id !== summary.id),
      { summary },
    ]);
    try {
      await savePracticeSummary(summary);
      setPendingSaves((pending) =>
        pending.filter((save) => save.summary.id !== summary.id),
      );
    } catch (error) {
      setPendingSaves((pending) =>
        pending.map((save) =>
          save.summary.id === summary.id
            ? { summary, failure: { error } }
            : save,
        ),
      );
    } finally {
      saving.current.delete(summary.id);
    }
  }

  return (
    <PlayBoardDialog
      heading={p('practiceSimilar')}
      className="similar-practice-dialog"
      busy={false}
      onClose={onClose}
      returnFocus={returnFocus}
    >
      {pendingSaves.map(
        ({ summary, failure }) =>
          failure && (
            <div key={summary.id} role="alert">
              <p>
                {p('practiceSaveFailed')} {formatError(failure.error)}
              </p>
              <ActionButton
                type="button"
                icon="refresh"
                label={p('practiceRetrySave')}
                onClick={() => void persist(summary)}
              />
            </div>
          ),
      )}
      {item ? (
        <>
          <p ref={progress} tabIndex={-1} role="status">
            {index + 1} / {roundItems.length}
          </p>
          <PracticeQuestion
            key={index}
            item={item}
            theme={theme}
            last={index + 1 === roundItems.length}
            onAdvance={advance}
            closeButton={closeButton}
          />
        </>
      ) : (
        <>
          <h3 ref={completion} tabIndex={-1}>
            {p('practiceComplete')}
          </h3>
          {scored.length ? (
            <>
              <p>
                <span>{p('practiceAverageLoss')}</span> ·{' '}
                <strong>
                  {formatNumber(
                    scored.reduce((total, move) => total + move.loss, 0) /
                      scored.length,
                    {
                      minimumFractionDigits: 3,
                      maximumFractionDigits: 6,
                    },
                  )}
                </strong>
              </p>
              <p>
                {p('practiceScoredMoves')} ·{' '}
                <strong>
                  {formatNumber(scored.length)} /{' '}
                  {formatNumber(roundItems.length)}
                </strong>
              </p>
              {!currentSave?.failure && (
                <p role="status">
                  {p(
                    currentSave
                      ? 'practiceSavingSummary'
                      : 'practiceSavedSummary',
                  )}
                </p>
              )}
            </>
          ) : (
            <p>{p('practiceNoScores')}</p>
          )}
          <div className="play-actions">
            {missed.length > 0 && (
              <ActionButton
                type="button"
                icon="reset"
                label={p('practiceRetryMissed')}
                onClick={() => setRound(startRound(missed, true))}
              />
            )}
            {closeButton}
          </div>
        </>
      )}
    </PlayBoardDialog>
  );
}

function PracticeQuestion({
  item,
  theme,
  last,
  onAdvance,
  closeButton,
}: {
  item: Hit;
  theme: Theme;
  last: boolean;
  onAdvance: (loss?: number) => void;
  closeButton: ReactNode;
}) {
  const p = usePositionText();
  const play = usePlayText();
  const review = useBoardReviewText();
  const {
    t: uiText,
    formatError,
    formatDecision,
    formatEquity,
    formatNumber,
  } = useI18n();
  const [answer, setAnswer] = useState<Answer | Score>();
  const [previewChoice, setPreviewChoice] = useState<'played' | 'best'>('best');
  const [pendingAction, setPendingAction] = useState<'score' | 'reveal'>();
  const loading = pendingAction !== undefined;
  const [error, setError] = useState<unknown>();
  const [steps, setSteps] = useState<CheckerStep[]>([]);
  const [chosenDie, setChosenDie] = useState<number>();
  const [hovered, setHovered] = useState<number | null>(null);
  const revealed = useRef<HTMLDivElement>(null);
  const pending = useRef<AbortController | null>(null);
  useEffect(() => () => pending.current?.abort(), []);
  useEffect(() => {
    if (answer) revealed.current?.focus();
  }, [answer]);
  const { position } = item;
  const context = matchContext(position.gnubg_match_id);
  const { player } = position;
  const turns = useMemo(
    () => legalCheckerTurns(position.board, player, position.dice),
    [position, player],
  );
  const firstDie = chosenDie ?? turns[0]?.dice[0];
  const dice =
    firstDie === position.dice[1]
      ? [...position.dice].reverse()
      : position.dice;
  const activeDie = dice[0];
  const moves = useMemo(
    () => turns.filter((turn) => turn.dice[0] === activeDie),
    [turns, activeDie],
  );
  const sources = loading ? [] : nextPoints(moves, steps, null);
  const canSwap =
    !loading &&
    !steps.length &&
    dice[0] !== dice[1] &&
    turns.some((turn) => turn.dice[0] === dice[1]);
  const forcedPass = !turns.length;
  const completedTurn = matchingMoves(moves, steps).find(
    (turn) => turn.steps.length === steps.length,
  );
  const canCheck = forcedPass || !!completedTurn;
  const board = answer
    ? position.board
    : applySteps(position.board, steps, player);
  const score = answer && 'equity_loss' in answer ? answer : undefined;
  const previewLabels = {
    played: p('practiceYourMove'),
    best: p('practiceBestPlay'),
  };
  const preview =
    score && previewChoice === 'played'
      ? projectPracticeSteps(position, steps, completedTurn?.dice ?? [])
      : answer && projectMove(position, answer.best);

  function moveChecker(from: number) {
    if (!sources.includes(from)) return;
    const [to] = nextPoints(moves, steps, from);
    if (to === undefined) return;
    setSteps((current) => [...current, { from, to }]);
    setError(undefined);
  }

  function swapDice() {
    if (!canSwap) return;
    setChosenDie(dice[1]);
    setHovered(null);
  }

  async function reveal(checkMove: boolean) {
    if (pending.current) return;
    const abort = new AbortController();
    pending.current = abort;
    setPendingAction(checkMove ? 'score' : 'reveal');
    setError(undefined);
    try {
      const result = await request<Answer | Score>(
        `/positions/${position.id}/${checkMove ? 'practice-score' : 'practice-answer'}`,
        {
          method: 'POST',
          signal: abort.signal,
          ...(checkMove && {
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ steps }),
          }),
        },
      );
      if (!abort.signal.aborted) setAnswer(result);
    } catch (failure) {
      if (!abort.signal.aborted) setError(failure);
    } finally {
      pending.current = null;
      if (!abort.signal.aborted) setPendingAction(undefined);
    }
  }

  return (
    <>
      <p>
        {item.match_name} · {p('gameNumber')} {position.game_no} ·{' '}
        {p('moveNumber')} {position.ply} ·{' '}
        {context.length
          ? uiText('app.matchLength', { count: context.length })
          : `${uiText('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}
      </p>
      <p>{p('practicePrompt')}</p>
      <div className={`similar-practice-board${answer ? '' : ' play-board'}`}>
        <Board
          position={{
            ...position,
            board,
            dice: answer ? position.dice : dice,
            notation: '',
          }}
          white={item.white}
          black={item.black}
          theme={theme}
          preview={preview}
          previewMode={answer ? 'arrows' : 'before'}
          previewLabel={answer ? previewLabels[previewChoice] : undefined}
          interaction={
            answer
              ? undefined
              : {
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
                      ? play('off')
                      : point === 0 || point === 25
                        ? play('bar')
                        : `${play('point')} ${relativePoint(point, player)}`,
                  dice: {
                    canSwap,
                    swap: swapDice,
                    label: `${play('swapDice')} · ${dice.join(', ')}`,
                    used:
                      dice[0] === dice[1]
                        ? [
                            Math.min(1, steps.length / 2),
                            Math.max(0, (steps.length - 2) / 2),
                          ]
                        : [steps.length >= 1, steps.length >= 2],
                    active:
                      loading || canCheck
                        ? null
                        : dice[0] === dice[1]
                          ? steps.length < 2
                            ? 0
                            : 1
                          : steps.length,
                  },
                }
          }
        />
      </div>
      {loading && <p role="status">{p('loadingPracticeAnswer')}</p>}
      {!!error && <p role="alert">{formatError(error)}</p>}
      {answer ? (
        <div ref={revealed} tabIndex={-1}>
          {score && (
            <>
              <div
                className="board-view-buttons text-navigation text-navigation--compact"
                role="group"
                aria-label={review('views')}
              >
                {(['played', 'best'] as const).map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    aria-pressed={previewChoice === choice}
                    onClick={() => setPreviewChoice(choice)}
                  >
                    {previewLabels[choice]}
                  </button>
                ))}
              </div>
              <p>
                {p('practiceLoss')} ·{' '}
                <strong>
                  {formatNumber(score.equity_loss, {
                    minimumFractionDigits: 3,
                    maximumFractionDigits: 6,
                  })}
                </strong>
              </p>
              <p>
                {p('practiceYourMove')} ·{' '}
                <strong>{formatDecision(score.played)}</strong> ·{' '}
                {formatEquity(score.played_equity)}
              </p>
            </>
          )}
          <p>
            {p('practiceBest')} · <strong>{formatDecision(answer.best)}</strong>
            {score && ` · ${formatEquity(score.best_equity)}`}
          </p>
          <p>{[answer.engine, answer.depth].filter(Boolean).join(' · ')}</p>
        </div>
      ) : (
        <div className="play-actions">
          <ActionButton
            type="button"
            icon="undo"
            label={play('undo')}
            disabled={loading || !steps.length}
            onClick={() => {
              setSteps((current) => current.slice(0, -1));
              setHovered(null);
              setError(undefined);
            }}
          />
          <ActionButton
            type="button"
            icon="confirm"
            label={forcedPass ? play('pass') : p('practiceCheckMove')}
            disabled={loading || !canCheck}
            onClick={() => void reveal(true)}
          />
          <ActionButton
            type="button"
            icon="view"
            label={p('practiceReveal')}
            disabled={loading}
            onClick={() => void reveal(false)}
          />
        </div>
      )}
      <div className="play-actions">
        <ActionButton
          type="button"
          icon="next"
          label={
            pendingAction === 'score'
              ? p('practiceSkipScoring')
              : p(last ? 'practiceFinish' : 'practiceNext')
          }
          onClick={() => onAdvance(score?.equity_loss)}
        />
        {closeButton}
      </div>
    </>
  );
}

function projectPracticeSteps(
  position: Hit['position'],
  steps: CheckerStep[],
  dice: number[],
): MovePreview {
  const { player } = position;
  const opponent = player === 0 ? -1 : 1;
  let board = position.board;
  const previewSteps = steps.map((step, index): MovePreviewStep => {
    const { from, to } = step;
    const hit = board[to] === opponent;
    board = applySteps(board, [step], player);
    return {
      from: from === 0 || from === 25 ? 'bar' : from,
      to: to < 0 || to > 25 ? 'off' : to,
      die: dice[index],
      hit,
    };
  });
  return { ok: true, board, steps: previewSteps, forced: !steps.length };
}
