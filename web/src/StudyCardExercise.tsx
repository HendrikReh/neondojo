import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import AnkiCardFace from './AnkiCardFace';
import Board from './Board';
import CheckerPracticeBoard, {
  type CheckerDraft,
} from './CheckerPracticeBoard';
import { request } from './api';
import { useBoardReviewText } from './board-review-messages';
import { matchContext } from './boardMath';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import type {
  AttemptedCard,
  CardAttemptRequest,
  CardQuestion,
  PracticeAnswer,
  Rating,
  RevealedCard,
} from './learning-api';
import { useLearningText } from './learning-messages';
import { projectMove, projectPracticeSteps } from './move-preview';
import CubePracticeChoices from './CubePracticeChoices';
import PracticeResult from './PracticeResult';
import { usePositionText } from './position-browser-messages';
import type { Theme } from './themes';

export default function StudyCardExercise({
  card,
  name,
  theme,
  activeAttempt,
  revealed,
  busy,
  visible,
  onAttempt,
  onAttemptBusy,
  onConflict,
  onReveal,
  onGrade,
}: {
  card: CardQuestion;
  name?: string;
  theme: Theme;
  activeAttempt?: AttemptedCard | null;
  revealed: RevealedCard | null;
  busy: boolean;
  visible: boolean;
  onAttempt: (result: AttemptedCard) => void;
  onAttemptBusy: (busy: boolean) => void;
  onConflict: () => void;
  onReveal: () => void;
  onGrade: (answer: RevealedCard, rating: Rating) => void;
}) {
  const t = useLearningText();
  const review = useBoardReviewText();
  const p = usePositionText();
  const { t: uiT, formatError, formatNumber } = useI18n();
  const [draft, setDraft] = useState<CheckerDraft>({ steps: [] });
  const [previewChoice, setPreviewChoice] = useState<'chosen' | 'best'>('best');
  const [error, setError] = useState<unknown>();
  const pending = useRef<AbortController | null>(null);
  const lastRequest = useRef<{ answer: string; id: string } | null>(null);
  const questionHeading = useRef<HTMLHeadingElement>(null);
  const answerHeading = useRef<HTMLHeadingElement>(null);
  const answer = activeAttempt?.revealed ?? revealed;
  const hasAnswer = !!answer;

  useEffect(() => {
    if (!visible) return;
    (hasAnswer ? answerHeading : questionHeading).current?.focus();
  }, [visible, hasAnswer]);
  useEffect(
    () => () => {
      if (pending.current) {
        const controller = pending.current;
        pending.current = null;
        controller.abort();
        onAttemptBusy(false);
      }
    },
    [visible, onAttemptBusy],
  );

  async function check(input: PracticeAnswer) {
    if (pending.current || busy || !visible) return;
    const submitted = JSON.stringify(input);
    if (lastRequest.current?.answer !== submitted) {
      lastRequest.current = { answer: submitted, id: crypto.randomUUID() };
    }
    const body: CardAttemptRequest = {
      expected_version: card.version,
      request_id: lastRequest.current.id,
      answer: input,
    };
    const controller = new AbortController();
    pending.current = controller;
    onAttemptBusy(true);
    setError(undefined);
    try {
      const result = await request<AttemptedCard>(`/cards/${card.id}/attempt`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (
        !controller.signal.aborted &&
        result.revealed.card_id === card.id &&
        result.revealed.version === card.version
      )
        onAttempt(result);
    } catch (failure) {
      if (controller.signal.aborted) return;
      if (failure instanceof AppError && failure.params.status === '409') {
        onConflict();
      } else {
        setError(failure);
      }
    } finally {
      if (pending.current === controller) {
        pending.current = null;
        onAttemptBusy(false);
      }
    }
  }

  const ratings = answer && (
    <div className="study-ratings">
      {(['again', 'hard', 'good', 'easy'] as const).map((rating) => (
        <ActionButton
          icon={rating === 'again' ? 'reset' : 'confirm'}
          label={t(rating)}
          key={rating}
          disabled={busy}
          onClick={() => onGrade(answer, rating)}
        >
          {t(rating)}
          <small>{interval(answer.intervals[rating])}</small>
        </ActionButton>
      ))}
    </div>
  );
  if (!card.position) {
    return (
      <article className="study-card study-card-anki" data-card-id={card.id}>
        <h3 ref={hasAnswer ? answerHeading : questionHeading} tabIndex={-1}>
          {card.prompt}
        </h3>
        {answer ? (
          ratings
        ) : (
          <ActionButton
            icon="solution"
            label={t('reveal')}
            disabled={busy}
            onClick={onReveal}
          >
            {t('reveal')}
          </ActionButton>
        )}
        <AnkiCardFace
          id={card.id}
          title={answer ? t('solution') : card.prompt}
          side={answer ? 'back' : 'front'}
        />
      </article>
    );
  }

  const checker = card.kind === 'checker';
  const cubeResponse = card.kind === 'cube_response';
  const context = matchContext(card.position.gnubg_match_id);
  const position = {
    ...card.position,
    dice: checker ? card.position.dice : [],
    notation: '',
  };
  const question = t(
    checker
      ? 'questionChecker'
      : cubeResponse
        ? 'questionResponse'
        : 'questionOffer',
  );
  const result = activeAttempt?.evaluation;
  const chosenPreview =
    activeAttempt?.submitted.kind === 'checker'
      ? projectPracticeSteps(position, activeAttempt.submitted.steps)
      : undefined;
  const bestNotation =
    result?.best ??
    answer?.answer.solution?.decision?.best ??
    answer?.answer.solution?.candidates.find(
      (candidate) => candidate.rank === 1,
    )?.notation;
  const bestPreview =
    checker && bestNotation ? projectMove(position, bestNotation) : undefined;
  const preview = previewChoice === 'chosen' ? chosenPreview : bestPreview;
  const previewLabels = {
    chosen: p('practiceYourMove'),
    best: p('practiceBestPlay'),
  };
  const revealButton = (
    <ActionButton
      type="button"
      icon="solution"
      label={t('reveal')}
      disabled={busy}
      onClick={() => {
        setError(undefined);
        onReveal();
      }}
    >
      {t('reveal')}
    </ActionButton>
  );

  return (
    <article className="study-card" data-card-id={card.id}>
      <span className="eyebrow">
        {name} · {t('reviewCount', { count: card.review_count })}
      </span>
      <h3 ref={questionHeading} tabIndex={-1}>
        {question}
      </h3>
      <p className="study-context">
        {context.length
          ? uiT('app.matchLength', { count: context.length })
          : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}{' '}
        · {uiT('app.toMove', { player: card.players[card.position.player] })}
      </p>
      {cubeResponse && <p>{t('offeredCube', { value: context.cube * 2 })}</p>}
      {checker && !answer ? (
        <CheckerPracticeBoard
          position={card.position}
          players={[card.players[0], card.players[1]]}
          theme={theme}
          draft={draft}
          onChange={(value) => {
            setDraft(value);
            setError(undefined);
          }}
          busy={busy}
          checkLabel={t('checkMove')}
          onCheck={(steps) => void check({ kind: 'checker', steps })}
        >
          {revealButton}
        </CheckerPracticeBoard>
      ) : (
        <>
          <Board
            position={position}
            white={card.players[0]}
            black={card.players[1]}
            theme={theme}
            cubeOfferPending={cubeResponse}
            preview={preview}
            previewMode={preview ? 'arrows' : 'before'}
            previewLabel={preview ? previewLabels[previewChoice] : undefined}
          />
          {!answer && (
            <CubePracticeChoices
              kind={cubeResponse ? 'cube_response' : 'cube_offer'}
              label={question}
              busy={busy}
              onCheck={(choice) => void check(choice)}
            >
              {revealButton}
            </CubePracticeChoices>
          )}
        </>
      )}
      {busy && !answer && <p role="status">{t('checking')}</p>}
      {!!error && <p role="alert">{formatError(error)}</p>}
      {answer && (
        <section className="study-answer" aria-label={t('solution')}>
          <h3 ref={answerHeading} tabIndex={-1}>
            {t('solution')}
          </h3>
          {result && (
            <div className="study-attempt-result">
              {chosenPreview?.ok &&
                bestPreview?.ok &&
                !chosenPreview.forced && (
                  <div
                    className="board-view-buttons text-navigation text-navigation--compact"
                    role="group"
                    aria-label={review('views')}
                  >
                    {(['chosen', 'best'] as const).map((choice) => (
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
                )}
              <PracticeResult evaluation={result} />
            </div>
          )}
          <CardSolution
            answer={answer}
            label={result ? t('cardAnalysis') : t('engine')}
          />
          {ratings}
          {busy && <p role="status">{t('saving')}</p>}
        </section>
      )}
    </article>
  );

  function interval(seconds: number) {
    const days = seconds / 86400;
    return formatNumber(
      days >= 1 ? Math.round(days) : Math.max(1, Math.round(seconds / 60)),
      {
        style: 'unit',
        unit: days >= 1 ? 'day' : 'minute',
        unitDisplay: 'short',
      },
    );
  }
}

function CardSolution({
  answer,
  label,
}: {
  answer: RevealedCard;
  label: string;
}) {
  const t = useLearningText();
  const { formatDecision, formatNumber, formatEquity } = useI18n();
  if (!answer.answer.solution) return null;
  const { decision, candidates, engine, plies } = answer.answer.solution;
  return (
    <div className="study-solution-source">
      <p>
        {label}: {engine} · {formatNumber(plies)}-ply
      </p>
      {decision && (
        <div className="study-decision">
          <strong>
            {t('best')}: {formatDecision(decision.best)}
          </strong>
          <p>
            {t('played')}: {formatDecision(decision.played)} · {t('loss')}:{' '}
            {formatNumber(decision.equity_loss, {
              minimumFractionDigits: 3,
              maximumFractionDigits: 3,
            })}
          </p>
        </div>
      )}
      {!!candidates.length && (
        <table>
          <thead>
            <tr>
              <th>{t('best')}</th>
              <th>Equity</th>
            </tr>
          </thead>
          <tbody>
            {candidates.map((candidate) => (
              <tr key={candidate.rank}>
                <td>{formatDecision(candidate.notation)}</td>
                <td>{formatEquity(candidate.equity)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {answer.answer.notes && (
        <div>
          <h4>{t('notes')}</h4>
          <p className="plaintext-comment">{answer.answer.notes}</p>
        </div>
      )}
      {!!answer.answer.tags.length && (
        <p>
          {t('tags')}: {answer.answer.tags.join(' · ')}
        </p>
      )}
    </div>
  );
}
