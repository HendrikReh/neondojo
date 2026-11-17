import { interpolate } from './i18n-core';
import { useEffect, useRef, useState, type RefObject } from 'react';
import Board from './Board';
import CheckerPracticeBoard, {
  type CheckerDraft,
} from './CheckerPracticeBoard';
import CubePracticeChoices from './CubePracticeChoices';
import PracticeResult from './PracticeResult';
import { CubeContextLine } from './CubeDecisionDetails';
import { matchContext } from './boardMath';
import ActionButton from './ActionButton';
import EngineLabel from './EngineLabel';
import PlayBoardDialog from './PlayBoardDialog';
import { useI18n } from './i18n';
import type { Theme } from './themes';
import {
  projectMove,
  projectPracticeSteps,
  type BoardView,
} from './move-preview';
import { useBoardReviewText } from './board-review-messages';
import { useLearningText } from './learning-messages';
import { usePositionText } from './position-browser-messages';
import type { PracticeAnswer } from './learning-api';
import { useMistakeText } from './recurring-mistakes-messages';
import {
  mistakeProvenance,
  mistakeReferenceKey,
} from './recurring-mistakes-provenance';
import {
  checkMistake,
  loadMistakePractice,
  revealMistake,
  type MistakeAnswer,
  type MistakeAttempt,
  type MistakePracticeRequest,
  type MistakeQuestion,
} from './recurring-mistakes-api';
import './recurring-mistakes.css';

export default function RecurringMistakePractice({
  request,
  items: given,
  theme,
  returnFocus,
  onClose,
}: {
  /** Load questions for a mistake group. */
  request?: MistakePracticeRequest;
  /** Questions chosen elsewhere, such as search results; skips loading. */
  items?: MistakeQuestion[];
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
    if (given || !request) {
      setItems((given ?? []).slice(0, 20));
      setLoading(false);
      return () => {
        abort.abort();
        generation.current++;
      };
    }
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
  }, [request, given, revision]);
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
  const learn = useLearningText();
  const labels = usePositionText();
  const { t: uiT, formatError, formatDecision, formatNumber } = useI18n();
  const context = matchContext(item.position.gnubg_match_id);
  const [result, setResult] = useState<{
    original: MistakeAnswer;
    checked?: {
      answer: PracticeAnswer;
      evaluation: MistakeAttempt['evaluation'];
    };
  }>();
  const [draft, setDraft] = useState<CheckerDraft>({ steps: [] });
  const [pending, setPending] = useState<'check' | 'reveal'>();
  const loading = pending !== undefined;
  const [error, setError] = useState<unknown>();
  const [view, setView] = useState<BoardView>('before');
  const [preview, setPreview] = useState<'best' | 'chosen'>('best');
  const abort = useRef<AbortController | null>(null);
  const [failedAction, setFailedAction] = useState<'check' | 'reveal'>();
  const feedback = useRef<HTMLDivElement>(null);
  useEffect(() => () => abort.current?.abort(), []);
  useEffect(() => {
    if (result) feedback.current?.focus();
  }, [result]);

  async function answerQuestion(answer?: PracticeAnswer) {
    if (abort.current) return;
    const controller = new AbortController();
    abort.current = controller;
    const action = answer ? 'check' : 'reveal';
    setPending(action);
    setError(undefined);
    setFailedAction(undefined);
    try {
      if (answer) {
        const { original, evaluation } = await checkMistake(
          { reference: item.reference, answer },
          controller.signal,
        );
        if (!controller.signal.aborted) {
          setResult({ original, checked: { answer, evaluation } });
          setView('arrows');
        }
      } else {
        const original = await revealMistake(item.reference, controller.signal);
        if (!controller.signal.aborted) setResult({ original });
      }
    } catch (failure) {
      if (!controller.signal.aborted) {
        setError(failure);
        setFailedAction(action);
      }
    } finally {
      abort.current = null;
      if (!controller.signal.aborted) setPending(undefined);
    }
  }
  const original = result?.original;
  const checked = result?.checked;
  const bestProjection =
    original && item.kind === 'checker'
      ? projectMove(item.position, checked?.evaluation.best ?? original.best)
      : undefined;
  const chosenProjection =
    checked?.answer.kind === 'checker'
      ? projectPracticeSteps(item.position, checked.answer.steps)
      : undefined;
  const projection = preview === 'chosen' ? chosenProjection : bestProjection;
  const revealButton = (
    <ActionButton
      icon="view"
      label={t('reveal')}
      disabled={loading}
      onClick={() => void answerQuestion()}
    >
      {t('reveal')}
    </ActionButton>
  );
  return (
    <div className="recurring-question">
      <p>
        <strong>{t(item.kind)}</strong> ·{' '}
        {interpolate(t('location'), {
          game: item.reference.game_no,
          decision: item.reference.decision_no,
        })}
      </p>
      <p className="recurring-source">
        {/* Origin, engine, depth and method stay readable without hovering:
            an imported rollout and a 0-ply evaluation are not the same evidence. */}
        {t('source')}: {mistakeProvenance(item, t)}
      </p>
      <p className="study-context">
        {context.length
          ? uiT('app.matchLength', { count: context.length })
          : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}
      </p>
      {item.kind === 'cube_response' && (
        <p>{learn('offeredCube', { value: context.cube * 2 })}</p>
      )}
      <CubeContextLine
        matchId={item.position.gnubg_match_id}
        players={[item.players[0], item.players[1]]}
        withFormat={false}
      />
      {result &&
        chosenProjection?.ok &&
        bestProjection?.ok &&
        !chosenProjection.forced && (
          <div
            className="board-view-buttons text-navigation text-navigation--compact"
            role="group"
            aria-label={review('views')}
          >
            {(['chosen', 'best'] as const).map((choice) => (
              <button
                key={choice}
                type="button"
                aria-pressed={preview === choice}
                onClick={() => setPreview(choice)}
              >
                {choice === 'chosen'
                  ? labels('practiceYourMove')
                  : labels('practiceBestPlay')}
              </button>
            ))}
          </div>
        )}
      {result && projection?.ok && (
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
      {!result && item.kind === 'checker' ? (
        <CheckerPracticeBoard
          position={item.position}
          players={item.players}
          theme={theme}
          boardClassName="recurring-practice-board"
          draft={draft}
          onChange={setDraft}
          busy={loading}
          checkLabel={learn('checkMove')}
          onCheck={(steps) => void answerQuestion({ kind: 'checker', steps })}
        >
          {revealButton}
        </CheckerPracticeBoard>
      ) : (
        <div className="play-board recurring-practice-board">
          <Board
            position={{ ...item.position, notation: '' }}
            white={item.players[0]}
            black={item.players[1]}
            theme={theme}
            cubeOfferPending={item.kind === 'cube_response'}
            preview={projection}
            previewMode={result ? view : 'before'}
          />
        </div>
      )}
      {!result && item.kind !== 'checker' && (
        <CubePracticeChoices
          kind={item.kind}
          label={t(item.kind)}
          busy={loading}
          onCheck={(answer) => void answerQuestion(answer)}
        >
          {revealButton}
        </CubePracticeChoices>
      )}
      {loading && (
        <p role="status">
          {pending === 'check' ? learn('checking') : t('revealing')}
        </p>
      )}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          {failedAction === 'reveal' && (
            <ActionButton
              icon="refresh"
              label={t('retry')}
              onClick={() => void answerQuestion()}
            >
              {t('retry')}
            </ActionButton>
          )}
        </div>
      )}
      {original && (
        <div ref={feedback} tabIndex={-1}>
          {checked && (
            <div className="recurring-attempt-result" aria-live="polite">
              <PracticeResult
                evaluation={checked.evaluation}
                checker={item.kind === 'checker' ? item.position : undefined}
                cube={
                  item.kind === 'checker'
                    ? undefined
                    : {
                        matchId: item.position.gnubg_match_id,
                        player: item.position.player,
                        response: item.kind === 'cube_response',
                        players: [item.players[0], item.players[1]],
                        position: item.position,
                      }
                }
                storedSource={
                  original.source === 'native'
                    ? undefined
                    : mistakeProvenance(original, t)
                }
              />
            </div>
          )}
          {checked && <h4>{t('originalAnalysis')}</h4>}
          <dl className="recurring-answer" aria-live="polite">
            <dt>{t('best')}</dt>
            <dd>{formatDecision(original.best)}</dd>
            <dt>{t('played')}</dt>
            <dd>{formatDecision(original.played)}</dd>
            <dt>{t('loss')}</dt>
            <dd>
              {formatNumber(original.equity_loss, {
                minimumFractionDigits: 3,
                maximumFractionDigits: 3,
              })}
            </dd>
            <dt>{t('source')}</dt>
            <dd>
              <EngineLabel
                details={original.engine}
                tooltip={mistakeProvenance(original, t)}
              />
            </dd>
          </dl>
        </div>
      )}
      <ActionButton
        icon="next"
        label={t(last ? 'finish' : 'next')}
        shortcut={result ? 'Enter' : undefined}
        onClick={onNext}
      >
        {t(last ? 'finish' : 'next')}
      </ActionButton>
    </div>
  );
}
