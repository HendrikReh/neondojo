import { useEffect, useRef, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import Board from '../../../shared/board/Board';
import { useCardSuggestionText } from './card-suggestion-messages';
import { useGamePlanText } from '../../analysis/game-plans/game-plan-messages';
import { useI18n } from '../../../shared/i18n/i18n';
import { AppError, interpolate } from '../../../shared/i18n/i18n-core';
import { useLearningText } from './learning-messages';
import {
  loadCardSuggestions,
  saveCardSuggestions,
  type CardSuggestions,
} from './learning-api';
import { useMistakeText } from '../mistakes/recurring-mistakes-messages';
import type { Theme } from '../../../shared/board/themes';

type Props = {
  deckId: string;
  matchId: string;
  player: string;
  minLoss: number;
  white: string;
  black: string;
  theme: Theme;
  disabled: boolean;
  onChanged: () => void;
};

export default function CardSuggestionsPanel({
  deckId,
  matchId,
  player,
  minLoss,
  white,
  black,
  theme,
  disabled,
  onChanged,
}: Props) {
  const text = useCardSuggestionText();
  const t = useLearningText();
  const planText = useGamePlanText();
  const mistakeText = useMistakeText();
  const { formatError, formatNumber, locale } = useI18n();
  const [result, setResult] = useState<CardSuggestions>();
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  const [pending, setPending] = useState<'load' | 'save'>();
  const [error, setError] = useState<unknown>();
  const [processed, setProcessed] = useState<number>();
  const [stale, setStale] = useState(false);
  const active = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => active.current?.abort(), []);

  async function load() {
    const controller = new AbortController();
    active.current?.abort();
    active.current = controller;
    setPending('load');
    setResult(undefined);
    setExcluded(new Set());
    setProcessed(undefined);
    setError(undefined);
    setStale(false);
    try {
      const next = await loadCardSuggestions(
        deckId,
        matchId,
        player,
        minLoss,
        controller.signal,
      );
      if (!controller.signal.aborted) setResult(next);
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure);
    } finally {
      if (!controller.signal.aborted) setPending(undefined);
    }
  }

  const selected =
    result?.items.filter(
      (item, index) => !item.in_deck && !excluded.has(index),
    ) ?? [];

  async function save() {
    const controller = new AbortController();
    active.current?.abort();
    active.current = controller;
    setPending('save');
    setError(undefined);
    try {
      const saved = await saveCardSuggestions(deckId, {
        match_id: matchId,
        decisions: selected.map((item) => item.reference),
      });
      if (!controller.signal.aborted) {
        setResult(undefined);
        setProcessed(saved.added + saved.updated);
      }
      onChanged();
    } catch (failure) {
      if (!controller.signal.aborted) {
        if (failure instanceof AppError && failure.params.status === '409') {
          setStale(true);
        } else {
          setError(failure);
        }
      }
    } finally {
      if (!controller.signal.aborted) setPending(undefined);
    }
  }

  return (
    <div className="card-suggestions">
      <div className="learning-controls">
        <ActionButton
          icon="search"
          label={text.suggest}
          disabled={disabled || !!pending}
          onClick={() => void load()}
        >
          {text.suggest}
        </ActionButton>
        {!!result?.items.length && (
          <ActionButton
            icon="add"
            label={text.add}
            disabled={disabled || !!pending || stale || !selected.length}
            onClick={() => void save()}
          >
            {text.add} ({formatNumber(selected.length)})
          </ActionButton>
        )}
      </div>
      {pending && (
        <p role="status">{pending === 'load' ? text.loading : t('adding')}</p>
      )}
      {processed !== undefined && (
        <p role="status">{t('added', { count: processed })}</p>
      )}
      {stale && <p role="alert">{text.changed}</p>}
      {!!error && <p role="alert">{formatError(error)}</p>}
      {result && !result.history_available && (
        <p role="status">{text.historyUnavailable}</p>
      )}
      {result && !result.items.length && (
        <p role="status">
          {result.has_analysis ? text.empty : text.noAnalysis}
        </p>
      )}
      {!!result?.items.length && (
        <ul className="card-suggestion-list" aria-label={text.suggest}>
          {result.items.map((item, index) => {
            const ref = item.reference;
            const decision = interpolate(text.decision, {
              game: formatNumber(ref.game_no),
              decision: formatNumber(ref.decision_no),
            });
            return (
              <li key={`${ref.job_id}:${ref.game_no}:${ref.decision_no}`}>
                <div className="card-suggestion-board">
                  <Board
                    position={item.position}
                    white={white}
                    black={black}
                    theme={theme}
                    previewLabel={decision}
                    showPipCounts={false}
                  />
                </div>
                <div className="card-suggestion-detail">
                  <label>
                    <input
                      type="checkbox"
                      checked={!item.in_deck && !excluded.has(index)}
                      disabled={disabled || !!pending || stale || item.in_deck}
                      onChange={(event) => {
                        const checked = event.target.checked;
                        setExcluded((previous) => {
                          const next = new Set(previous);
                          if (checked) next.delete(index);
                          else next.add(index);
                          return next;
                        });
                      }}
                    />
                    <strong>
                      {decision} · {item.player_name}
                    </strong>
                  </label>
                  <span>
                    {mistakeText(item.mistake_kind)} ·{' '}
                    {planText(item.game_plan)}
                  </span>
                  <span>
                    {t('loss')}{' '}
                    {formatNumber(item.equity_loss, {
                      maximumFractionDigits: 3,
                    })}
                  </span>
                  <span>
                    {item.recurring_matches
                      ? interpolate(
                          text.recurring,
                          { count: item.recurring_matches },
                          locale,
                        )
                      : text.largestLoss}
                  </span>
                  {item.card_id && (
                    <span>{item.in_deck ? text.inDeck : text.known}</span>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
