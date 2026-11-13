import ActionButton from './ActionButton';
import { useI18n } from './i18n';
import { useGamePlanText } from './game-plan-messages';
import { useMistakeText } from './recurring-mistakes-messages';
import type { MistakeGroup } from './recurring-mistakes-api';

export default function MistakeGroupTable({
  groups,
  onPractice,
}: {
  groups: MistakeGroup[];
  onPractice: (group: MistakeGroup, button: HTMLButtonElement) => void;
}) {
  const t = useMistakeText();
  const planText = useGamePlanText();
  const { formatNumber, formatPercent } = useI18n();
  const lossDigits = { minimumFractionDigits: 3, maximumFractionDigits: 3 };
  return (
    <div className="study-report-table-scroll">
      <table className="study-report-table">
        <thead>
          <tr>
            <th scope="col">{planText('title')}</th>
            <th scope="col">{t('kind')}</th>
            <th scope="col">{t('occurrences')}</th>
            <th scope="col">{t('opportunities')}</th>
            <th scope="col">{t('errorRate')}</th>
            <th scope="col">{t('matches')}</th>
            <th scope="col">{t('totalLoss')}</th>
            <th scope="col">{t('lossPer100')}</th>
            <th scope="col">{t('practice')}</th>
          </tr>
        </thead>
        <tbody>
          {groups.map((group) => (
            <tr key={`${group.game_plan}:${group.mistake_kind}`}>
              <td>{planText(group.game_plan)}</td>
              <td>{t(group.mistake_kind)}</td>
              <td>{formatNumber(group.count)}</td>
              <td>
                {group.rates ? formatNumber(group.rates.decision_count) : '—'}
                {group.rates?.small_sample && (
                  <small className="mistake-small-sample">
                    {t('smallSample')}
                  </small>
                )}
              </td>
              <td>
                {group.rates ? formatPercent(group.rates.error_rate, 1) : '—'}
              </td>
              <td>{formatNumber(group.match_count)}</td>
              <td>{formatNumber(group.total_loss, lossDigits)}</td>
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
