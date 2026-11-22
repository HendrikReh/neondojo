import { useEffect, useId, useState } from 'react';
import ActionButton from './ActionButton';
import { request } from './api';
import { useCardPracticeText } from './card-practice-messages';
import { useCardSearchText } from './card-search-messages';
import { useGamePlanText } from './game-plan-messages';
import { useI18n } from './i18n';
import type {
  PracticeRound,
  TransferRequest,
  TransferSummary,
} from './learning-api';
import { SearchFilterInput, SearchFilterSelect } from './SearchFilterForm';

const plans = [
  'opening',
  'pure_race',
  'racing',
  'attacking',
  'priming',
  'anchoring',
] as const;
const kinds = ['checker', 'cube_offer', 'cube_response'] as const;
const path = '/study/transfer-rounds';

export default function TransferPractice({
  onStart,
}: {
  onStart: (round: PracticeRound) => void;
}) {
  const t = useCardPracticeText();
  const gamePlan = useGamePlanText();
  const search = useCardSearchText();
  const { formatError, formatNumber, locale } = useI18n();
  const [player, setPlayer] = useState('');
  const [plan, setPlan] = useState<TransferRequest['game_plan']>('pure_race');
  const [kind, setKind] = useState<TransferRequest['kind']>('cube_offer');
  const heading = useId();
  const [history, setHistory] = useState<TransferSummary[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  useEffect(() => {
    const abort = new AbortController();
    void request<TransferSummary[]>(path, { signal: abort.signal })
      .then(setHistory)
      .catch((failure) => {
        if (!abort.signal.aborted) setError(failure);
      });
    return () => abort.abort();
  }, []);
  async function start(follow_up: boolean) {
    setBusy(true);
    setError(undefined);
    try {
      const round = await request<PracticeRound>(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          player,
          game_plan: plan,
          kind,
          follow_up,
        } satisfies TransferRequest),
      });
      onStart(round);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="learning-extra" aria-labelledby={heading}>
      <h3 id={heading}>{t('transfer')}</h3>
      <div className="learning-controls">
        <SearchFilterInput
          label={t('exactPlayer')}
          value={player}
          onChange={setPlayer}
          maxLength={240}
          disabled={busy}
        />
        <SearchFilterSelect
          label={gamePlan('title')}
          value={plan}
          onChange={(value) =>
            setPlan(plans.find((item) => item === value) ?? 'pure_race')
          }
        >
          {plans.map((value) => (
            <option value={value} key={value}>
              {gamePlan(value)}
            </option>
          ))}
        </SearchFilterSelect>
        <SearchFilterSelect
          label={search('kind')}
          value={kind}
          onChange={(value) =>
            setKind(kinds.find((item) => item === value) ?? 'checker')
          }
        >
          {kinds.map((value) => (
            <option value={value} key={value}>
              {search(value)}
            </option>
          ))}
        </SearchFilterSelect>
        <ActionButton
          icon="play"
          label={t('baseline')}
          disabled={busy || !player.trim()}
          onClick={() => void start(false)}
        >
          {t('baseline')}
        </ActionButton>
        <ActionButton
          icon="play"
          label={t('followUp')}
          disabled={busy || !player.trim()}
          onClick={() => void start(true)}
        >
          {t('followUp')}
        </ActionButton>
      </div>
      {!!error && <p role="alert">{formatError(error)}</p>}
      {!!history.length && (
        <ul>
          {history.map((entry) => (
            <li key={entry.id}>
              {new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(
                new Date(entry.completed_at),
              )}{' '}
              · {entry.scope.player} · {gamePlan(entry.scope.game_plan)} ·{' '}
              {search(entry.scope.kind)} ·{' '}
              {t(entry.scope.follow_up ? 'followUp' : 'baseline')} ·{' '}
              {t('transferSummary', {
                scored: entry.scored,
                correct: entry.correct,
                revealed: entry.revealed,
                loss:
                  entry.mean_loss == null
                    ? '—'
                    : formatNumber(entry.mean_loss, {
                        maximumFractionDigits: 3,
                      }),
              })}
              {' · '}
              {t('unsure')}: {formatNumber(entry.unsure)}
              {' · '}
              {formatNumber(entry.duration_seconds / 60, {
                style: 'unit',
                unit: 'minute',
                maximumFractionDigits: 1,
              })}
              {!!entry.lessons.length && (
                <ul>
                  {entry.lessons.map((lesson, index) => (
                    <li key={index}>{lesson}</li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
