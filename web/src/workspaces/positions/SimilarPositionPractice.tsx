import { useCheckerTurns } from '../study/practice/useCheckerTurns';
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import EngineLabel from '../../shared/engine/EngineLabel';
import Board from '../../shared/board/Board';
import CheckerPracticeBoard, {
  type CheckerDraft,
} from '../study/practice/CheckerPracticeBoard';
import PlayBoardDialog from '../../shared/board/PlayBoardDialog';
import { request } from '../../shared/api/api';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import { matchContext } from '../../shared/board/boardMath';
import type { components } from '../../generated-api';
import { useI18n } from '../../shared/i18n/i18n';
import {
  projectMove,
  projectPracticeSteps,
} from '../../shared/board/move-preview';
import { usePositionText } from './position-browser-messages';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import {
  savePracticeSummary,
  type PracticeSummaryInput,
} from '../study/practice/similar-practice-history-api';
import type { Theme } from '../../shared/board/themes';

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
  const t = useMistakeText();
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
      label={t('close')}
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
            {t('complete')}
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
  const t = useMistakeText();
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
  const [draft, setDraft] = useState<CheckerDraft>({ steps: [] });
  const revealed = useRef<HTMLDivElement>(null);
  const pending = useRef<AbortController | null>(null);
  useEffect(() => () => pending.current?.abort(), []);
  useEffect(() => {
    if (answer) revealed.current?.focus();
  }, [answer]);
  const { position } = item;
  const checkerTurns = useCheckerTurns(position);
  const context = matchContext(position.gnubg_match_id);
  const score = answer && 'equity_loss' in answer ? answer : undefined;
  const previewLabels = {
    played: p('practiceYourMove'),
    best: p('practiceBestPlay'),
  };
  const preview =
    score && previewChoice === 'played'
      ? projectPracticeSteps(position, draft.steps, checkerTurns.turns)
      : answer
        ? projectMove(position, answer.best)
        : undefined;

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
            body: JSON.stringify({ steps: draft.steps }),
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
      {answer ? (
        <div className="similar-practice-board">
          <Board
            position={{ ...position, notation: '' }}
            white={item.white}
            black={item.black}
            theme={theme}
            preview={preview}
            previewMode="arrows"
            previewLabel={previewLabels[previewChoice]}
          />
        </div>
      ) : (
        <CheckerPracticeBoard
          position={position}
          checkerTurns={checkerTurns}
          players={[item.white, item.black]}
          theme={theme}
          boardClassName="similar-practice-board"
          draft={draft}
          onChange={(value) => {
            setDraft(value);
            setError(undefined);
          }}
          busy={loading}
          checkLabel={p('practiceCheckMove')}
          onCheck={() => void reveal(true)}
        >
          <ActionButton
            type="button"
            icon="view"
            label={t('reveal')}
            disabled={loading}
            onClick={() => void reveal(false)}
          />
        </CheckerPracticeBoard>
      )}
      {loading && <p role="status">{p('loadingPracticeAnswer')}</p>}
      {!!error && <p role="alert">{formatError(error)}</p>}
      {answer && (
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
                {t('loss')} ·{' '}
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
            {t('best')} · <strong>{formatDecision(answer.best)}</strong>
            {score && ` · ${formatEquity(score.best_equity)}`}
          </p>
          <p>
            <EngineLabel
              details={[answer.engine, answer.depth]
                .filter(Boolean)
                .join(' · ')}
            />
          </p>
        </div>
      )}
      <div className="play-actions">
        <ActionButton
          type="button"
          icon="next"
          label={
            pendingAction === 'score'
              ? p('practiceSkipScoring')
              : t(last ? 'finish' : 'next')
          }
          onClick={() => onAdvance(score?.equity_loss)}
        />
        {closeButton}
      </div>
    </>
  );
}
