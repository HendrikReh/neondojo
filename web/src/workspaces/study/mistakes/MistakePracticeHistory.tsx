import { useEffect, useState } from 'react';
import { request } from '../../../shared/http/api';
import { useDecisionSearchText } from '../../positions/decision-search-messages';
import { useGamePlanText } from '../../analysis/game-plans/game-plan-messages';
import type { GamePlan } from '../../analysis/game-plans/game-plan-api';
import type { components } from '../../../generated-api';
import { useI18n } from '../../../shared/i18n/i18n';
import { useMistakeText } from './recurring-mistakes-messages';
import type { MistakeKind } from './recurring-mistakes-api';
import { dateTimeFormat } from '../../../shared/i18n/i18n-core';

type History = components['schemas']['MistakePracticeHistory'];

/** Checked answers from mistake and search practice, by day and group. */
export default function MistakePracticeHistory() {
  const t = useDecisionSearchText();
  const mistake = useMistakeText();
  const plan = useGamePlanText();
  const { locale, formatNumber, formatPercent } = useI18n();
  const [items, setItems] = useState<History['items']>();
  useEffect(() => {
    const abort = new AbortController();
    void request<History>('/study/mistakes/history', { signal: abort.signal })
      .then((history) => {
        if (!abort.signal.aborted) setItems(history.items);
      })
      // The section is supplementary; the round history below stays usable.
      .catch(() => undefined);
    return () => abort.abort();
  }, []);
  if (!items?.length) return null;
  const date = dateTimeFormat(locale, { dateStyle: 'medium' });
  return (
    <section
      className="study-report-empty mistake-practice-history"
      aria-labelledby="mistake-practice-history"
    >
      <h2 id="mistake-practice-history">{t('historyTitle')}</h2>
      <div className="study-report-table-scroll">
        <table className="study-report-table">
          <thead>
            <tr>
              <th scope="col">{t('historyDay')}</th>
              <th scope="col">{mistake('player')}</th>
              <th scope="col">{t('plan')}</th>
              <th scope="col">{t('mistake')}</th>
              <th scope="col">{t('historyAnswers')}</th>
              <th scope="col">{t('historyBest')}</th>
              <th scope="col">{t('historyLoss')}</th>
            </tr>
          </thead>
          <tbody>
            {items.map((row) => (
              <tr
                key={`${row.day}|${row.player}|${row.game_plan}|${row.mistake_kind}`}
              >
                <td>{date.format(new Date(`${row.day}T12:00:00`))}</td>
                <td>{row.player}</td>
                <td>{plan(row.game_plan as GamePlan)}</td>
                <td>{mistake(row.mistake_kind as MistakeKind)}</td>
                <td>{formatNumber(row.attempts)}</td>
                <td>
                  {formatNumber(row.best)} (
                  {formatPercent(row.best / row.attempts)})
                </td>
                <td>
                  {formatNumber(row.average_equity_loss, {
                    minimumFractionDigits: 3,
                    maximumFractionDigits: 3,
                  })}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}
