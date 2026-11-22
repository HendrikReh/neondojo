import { useEffect, useRef, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import CardTagEditor from './CardTagEditor';
import { request } from './shared/api/api';
import { useCardPracticeText } from './card-practice-messages';
import { useI18n } from './shared/i18n/i18n';
import { AppError } from './shared/i18n/i18n-core';
import { useLearningText } from './learning-messages';
import type {
  CardQuestion,
  PracticeRound,
  RoundAnswer,
  RoundAdvance,
  RoundVersion,
} from './learning-api';
import { CardExercise } from './StudyCardExercise';
import {
  SearchFilterInput,
  SearchFilterSelect,
} from './shared/ui/SearchFilterForm';
import type { Theme } from './shared/board/themes';

export const roundPath = (id: string) => `/study/card-rounds/${id}`;

export default function CardPracticeRound({
  round,
  theme,
  visible,
  onChange,
  onClose,
}: {
  round: PracticeRound;
  theme: Theme;
  visible: boolean;
  onChange: (round: PracticeRound) => void;
  onClose: () => void;
}) {
  const t = useCardPracticeText();
  const learning = useLearningText();
  const { formatError, formatNumber } = useI18n();
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<unknown>();
  const [reflection, setReflection] = useState<{
    key: string;
    confidence?: boolean;
    diagnosis?: string;
    lesson?: string;
  }>();
  const pending = useRef<AbortController | null>(null);
  const revealId = useRef<{ version: number; id: string } | undefined>(
    undefined,
  );
  const heading = useRef<HTMLHeadingElement>(null);
  const complete = round.index >= round.total;
  const reflectionKey = `${round.pass}:${round.index}`;
  const currentReflection =
    reflection?.key === reflectionKey ? reflection : undefined;
  const updateReflection = (value: Partial<NonNullable<typeof reflection>>) =>
    setReflection({ ...currentReflection, ...value, key: reflectionKey });
  const waiting = busy || checking;
  const path = roundPath(round.id);

  useEffect(() => () => pending.current?.abort(), []);
  useEffect(() => {
    if (!visible && pending.current) {
      pending.current.abort();
      pending.current = null;
      setBusy(false);
    }
  }, [visible]);
  useEffect(() => {
    if (complete && visible) heading.current?.focus();
  }, [complete, visible]);

  async function send(
    action: 'answer' | 'advance' | 'mark' | 'retry',
    body: RoundAnswer | RoundAdvance | RoundVersion,
    signal: AbortSignal,
  ) {
    const next = await request<PracticeRound>(`${path}/${action}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal,
    });
    if (!signal.aborted) onChange(next);
  }

  async function update(
    action?: 'answer' | 'advance' | 'mark' | 'retry',
    body?: RoundAnswer | RoundAdvance | RoundVersion,
  ) {
    if (pending.current) return;
    const controller = new AbortController();
    pending.current = controller;
    setBusy(true);
    setError(undefined);
    try {
      if (action && body) await send(action, body, controller.signal);
      else {
        const next = await request<PracticeRound>(path, {
          signal: controller.signal,
        });
        if (!controller.signal.aborted) onChange(next);
      }
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure);
    } finally {
      if (pending.current === controller) {
        pending.current = null;
        setBusy(false);
      }
    }
  }

  const card: CardQuestion | undefined = round.card ?? undefined;
  const result = round.result;
  const nextButton = (
    <ActionButton
      icon="play"
      label={t('next')}
      shortcut={result ? 'Enter' : undefined}
      disabled={waiting}
      onClick={() =>
        void update('advance', {
          version: round.version,
          marked: round.marked,
          diagnosis: currentReflection?.diagnosis,
          lesson: currentReflection?.lesson,
        })
      }
    >
      {t('next')}
    </ActionButton>
  );

  return (
    <section className="card-practice-round" aria-label={t('start')}>
      <div className="learning-controls">
        <ActionButton icon="back" label={learning('back')} onClick={onClose}>
          {learning('back')}
        </ActionButton>
        <span>{t('mode')}</span>
      </div>
      {!!error && (
        <p role="alert">
          {formatError(error)}{' '}
          {error instanceof AppError &&
            ['404', '409'].includes(String(error.params.status)) && (
              <ActionButton
                icon="refresh"
                label={t('resume')}
                onClick={() => void update()}
              >
                {t('resume')}
              </ActionButton>
            )}
        </p>
      )}
      {round.pass === 2 && <p>{t('retryTitle')}</p>}
      {complete ? (
        <>
          <h2 ref={heading} tabIndex={-1}>
            {t('complete')}
          </h2>
          <p>
            {t('summary', {
              correct: round.summary.correct,
              errors: round.summary.errors,
              revealed: round.summary.revealed,
              skipped: round.summary.skipped,
            })}
          </p>
          {round.pass === 1 && round.summary.retry_count > 0 && (
            <ActionButton
              icon="reset"
              label={t('retry', {
                count: formatNumber(round.summary.retry_count),
              })}
              disabled={waiting}
              onClick={() => void update('retry', { version: round.version })}
            >
              {t('retry', { count: formatNumber(round.summary.retry_count) })}
            </ActionButton>
          )}
        </>
      ) : (
        <p role="status">
          {t('progress', {
            index: formatNumber(round.index + 1),
            total: formatNumber(round.total),
          })}
        </p>
      )}
      {round.missing && (
        <>
          <p role="status">{t('missing')}</p>
          {nextButton}
        </>
      )}
      {card && (
        <CardExercise
          key={`${round.pass}:${round.index}:${card.id}:${card.version}`}
          card={card}
          theme={theme}
          visible={visible}
          busy={waiting}
          answer={result?.answer}
          evaluation={result?.evaluation}
          submitted={result?.submitted}
          beforeAnswer={
            <SearchFilterSelect
              label={t('confidence')}
              value={String(currentReflection?.confidence ?? '')}
              onChange={(value) =>
                updateReflection({
                  confidence: value ? value === 'true' : undefined,
                })
              }
            >
              <option value="">—</option>
              <option value="true">{t('sure')}</option>
              <option value="false">{t('unsure')}</option>
            </SearchFilterSelect>
          }
          onCheck={(body, signal) =>
            send(
              'answer',
              {
                version: round.version,
                card_version: body.expected_version,
                request_id: body.request_id,
                answer: body.answer,
                confidence: currentReflection?.confidence,
              },
              signal,
            )
          }
          onAttemptBusy={setChecking}
          onConflict={() => void update()}
          onReveal={() => {
            if (revealId.current?.version !== round.version)
              revealId.current = {
                version: round.version,
                id: crypto.randomUUID(),
              };
            void update('answer', {
              version: round.version,
              card_version: card.version,
              request_id: revealId.current.id,
              answer: null,
            });
          }}
        >
          {!round.transfer && (
            <CardTagEditor
              cards={[{ id: card.id, tags_version: card.tags_version ?? 0 }]}
            />
          )}
          <div className="learning-controls">
            <SearchFilterSelect
              label={t('diagnosis')}
              value={currentReflection?.diagnosis ?? ''}
              onChange={(diagnosis) => updateReflection({ diagnosis })}
            >
              <option value="">—</option>
              {(['pattern', 'principle', 'calculation'] as const).map(
                (value) => (
                  <option value={value} key={value}>
                    {t(value)}
                  </option>
                ),
              )}
            </SearchFilterSelect>
            <SearchFilterInput
              label={t('lesson')}
              value={currentReflection?.lesson ?? ''}
              maxLength={500}
              onChange={(lesson) => updateReflection({ lesson })}
            />
          </div>
          <div className="learning-controls">
            {round.pass === 1 && (
              <label>
                <input
                  type="checkbox"
                  checked={round.marked}
                  disabled={waiting}
                  onChange={(event) =>
                    void update('mark', {
                      version: round.version,
                      marked: event.target.checked,
                    })
                  }
                />{' '}
                {t('mark')}
              </label>
            )}
            {nextButton}
          </div>
        </CardExercise>
      )}
    </section>
  );
}
