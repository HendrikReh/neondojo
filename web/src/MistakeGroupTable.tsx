import ActionButton from './shared/ui/ActionButton';
import { useI18n } from './shared/i18n/i18n';
import { useGamePlanText } from './workspaces/analysis/game-plans/game-plan-messages';
import { useMistakeText } from './recurring-mistakes-messages';
import type { MistakeGroup } from './recurring-mistakes-api';

export default function MistakeGroupTable({
  groups,
  onPractice,
  compact = false,
}: {
  groups: MistakeGroup[];
  /** Leaves out the raw counts and the total loss; the rates stay. */
  compact?: boolean;
  onPractice: (group: MistakeGroup, button: HTMLButtonElement) => void;
}) {
  const t = useMistakeText();
  const planText = useGamePlanText();
  const { formatNumber, formatPercent } = useI18n();
  const lossDigits = { minimumFractionDigits: 3, maximumFractionDigits: 3 };
  const smallSample = (group: MistakeGroup) =>
    group.rates?.small_sample && (
      <small className="mistake-small-sample">{t('smallSample')}</small>
    );
  return (
    <div className="study-report-table-scroll">
      <table className="study-report-table">
        <thead>
          <tr>
            <th scope="col">{planText('title')}</th>
            <th scope="col">{t('kind')}</th>
            {!compact && <th scope="col">{t('occurrences')}</th>}
            {!compact && <th scope="col">{t('opportunities')}</th>}
            <th scope="col">{t('errorRate')}</th>
            <th scope="col" title={t('trendHelp')}>
              {t('trend')}
            </th>
            {!compact && <th scope="col">{t('matches')}</th>}
            {!compact && <th scope="col">{t('totalLoss')}</th>}
            <th scope="col">{t('lossPer100')}</th>
            <th scope="col">{t('practice')}</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((group) => (
            <tr key={`${group.game_plan}:${group.mistake_kind}`}>
              <td>{planText(group.game_plan)}</td>
              <td>
                {t(group.mistake_kind)}
                {compact && smallSample(group)}
              </td>
              {!compact && <td>{formatNumber(group.count)}</td>}
              {!compact && (
                <td>
                  {group.rates ? formatNumber(group.rates.decision_count) : '—'}
                  {smallSample(group)}
                </td>
              )}
              <td>
                {group.rates ? formatPercent(group.rates.error_rate, 1) : '—'}
              </td>
              <td data-mistake-trend>
                {group.rates?.trend
                  ? `${formatPercent(group.rates.trend.earlier_error_rate, 1)} → ${formatPercent(group.rates.trend.recent_error_rate, 1)}`
                  : '—'}
              </td>
              {!compact && <td>{formatNumber(group.match_count)}</td>}
              {!compact && (
                <td>{formatNumber(group.total_loss, lossDigits)}</td>
              )}
              <td>
                {group.rates
                  ? formatNumber(group.rates.loss_per_100, lossDigits)
                  : '—'}
              </td>
              <td>
                <ActionButton
                  icon="next"
                  label={t('practice')}
                  onClick={(event) => onPractice(group, event.currentTarget)}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
