import { useEffect, useMemo, useRef, useState, type RefObject } from 'react';
import Board from './Board';
import { matchContext } from './boardMath';
import ActionButton from './ActionButton';
import PlayBoardDialog from './PlayBoardDialog';
import { useI18n } from './i18n';
import type { Theme } from './themes';
import { projectMove, type BoardView } from './move-preview';
import { useBoardReviewText } from './board-review-messages';
import { useMistakeText } from './recurring-mistakes-messages';
import {
  mistakeProvenance,
  mistakeReferenceKey,
} from './recurring-mistakes-provenance';
import {
  loadMistakePractice,
  revealMistake,
  type MistakeAnswer,
  type MistakePracticeRequest,
  type MistakeQuestion,
} from './recurring-mistakes-api';
import './recurring-mistakes.css';

export default function RecurringMistakePractice({
  request,
  theme,
  returnFocus,
  onClose,
}: {
  request: MistakePracticeRequest;
  theme: Theme;
  returnFocus: RefObject<HTMLElement | null>;
  onClose: () => void;
}) {
  const t = useMistakeText(),
    { formatError } = useI18n();
  const [items, setItems] = useState<MistakeQuestion[]>();
  const [error, setError] = useState<unknown>();
  const [loading, setLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [index, setIndex] = useState(0);
  const generation = useRef(0);
  useEffect(() => {
    const abort = new AbortController(),
      version = ++generation.current;
    setLoading(true);
    setError(undefined);
    setItems(undefined);
    setIndex(0);
    void loadMistakePractice(request, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted && version === generation.current)
          setItems(result.items.slice(0, 20));
      })
      .catch((failure) => {
        if (!abort.signal.aborted && version === generation.current)
          setError(failure);
      })
      .finally(() => {
        if (!abort.signal.aborted && version === generation.current)
          setLoading(false);
      });
    return () => {
      abort.abort();
      generation.current++;
    };
  }, [request, revision]);
  const item = items?.[index];
  return (
    <PlayBoardDialog
      heading={t('practiceTitle')}
      busy={loading}
      onClose={onClose}
      returnFocus={returnFocus}
      className="recurring-practice-dialog"
    >
      <p className="recurring-help">{t('practiceHelp')}</p>
      {loading && <p role="status">{t('loadingPractice')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={t('retry')}
            onClick={() => setRevision((value) => value + 1)}
          >
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {!loading &&
        !error &&
        items &&
        (!items.length ? (
          <p role="status">{t('noQuestions')}</p>
        ) : item ? (
          <>
            <p role="status">
              {index + 1} / {items.length}
            </p>
            <MistakeQuestion
              key={mistakeReferenceKey(item.reference)}
              item={item}
              theme={theme}
              onNext={() => setIndex((value) => value + 1)}
              last={index === items.length - 1}
            />
          </>
        ) : (
          <div role="status">
            <h3>{t('complete')}</h3>
            <p>
              {items.length} / {items.length}
            </p>
          </div>
        ))}
      <ActionButton icon="close" label={t('close')} onClick={onClose}>
        {t('close')}
      </ActionButton>
    </PlayBoardDialog>
  );
}

function MistakeQuestion({
  item,
  theme,
  onNext,
  last,
}: {
  item: MistakeQuestion;
  theme: Theme;
  onNext: () => void;
  last: boolean;
}) {
  const t = useMistakeText(),
    review = useBoardReviewText();
  const { t: uiT, formatError, formatDecision } = useI18n();
  const context = matchContext(item.position.gnubg_match_id);
  const [answer, setAnswer] = useState<MistakeAnswer>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [view, setView] = useState<BoardView>('before');
  const abort = useRef<AbortController | null>(null),
    generation = useRef(0);
  useEffect(
    () => () => {
      abort.current?.abort();
      generation.current++;
    },
    [],
  );
  async function reveal() {
    abort.current?.abort();
    const controller = new AbortController(),
      version = ++generation.current;
    abort.current = controller;
    setLoading(true);
    setError(undefined);
    try {
      const result = await revealMistake(item.reference, controller.signal);
      if (!controller.signal.aborted && version === generation.current)
        setAnswer(result);
    } catch (failure) {
      if (!controller.signal.aborted && version === generation.current)
        setError(failure);
    } finally {
      if (!controller.signal.aborted && version === generation.current)
        setLoading(false);
    }
  }
  const projection = useMemo(
    () =>
      answer && item.kind === 'checker'
        ? projectMove(item.position, answer.best)
        : undefined,
    [answer, item],
  );
  return (
    <div className="recurring-question">
      <p>
        <strong>{t(item.kind)}</strong> · {item.reference.game_no}.
        {item.reference.decision_no}
      </p>
      <p className="recurring-source">
        {t('source')}: {mistakeProvenance(item, t)}
      </p>
      <p className="study-context">
        {context.length
          ? uiT('app.matchLength', { count: context.length })
          : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}
      </p>
      {answer && projection?.ok && (
        <div
          className="board-view-buttons text-navigation text-navigation--compact"
          role="group"
          aria-label={review('views')}
        >
          {(['before', 'arrows', 'after'] as BoardView[]).map((mode) => (
            <button
              key={mode}
              type="button"
              aria-pressed={view === mode}
              onClick={() => setView(mode)}
            >
              {review(mode)}
            </button>
          ))}
        </div>
      )}
      <div className="play-board recurring-practice-board">
        <Board
          position={{ ...item.position, notation: '' }}
          white={item.players[0]}
          black={item.players[1]}
          theme={theme}
          preview={answer ? projection : undefined}
          previewMode={answer ? view : 'before'}
        />
      </div>
      {loading && <p role="status">{t('revealing')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={t('retry')}
            onClick={() => void reveal()}
          >
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {answer ? (
        <dl className="recurring-answer" aria-live="polite">
          <dt>{t('best')}</dt>
          <dd>{formatDecision(answer.best)}</dd>
          <dt>{t('played')}</dt>
          <dd>{formatDecision(answer.played)}</dd>
          <dt>{t('loss')}</dt>
          <dd>{answer.equity_loss.toFixed(3)}</dd>
          <dt>{t('source')}</dt>
          <dd>{mistakeProvenance(answer, t)}</dd>
        </dl>
      ) : (
        !error && (
          <ActionButton
            icon="view"
            label={t('reveal')}
            disabled={loading}
            onClick={() => void reveal()}
          >
            {t('reveal')}
          </ActionButton>
        )
      )}
      <ActionButton
        icon="next"
        label={t(last ? 'finish' : 'next')}
        onClick={onNext}
      >
        {t(last ? 'finish' : 'next')}
      </ActionButton>
    </div>
  );
}
