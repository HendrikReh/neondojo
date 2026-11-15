import { useCandidateText } from './candidate-messages';
import type { components } from './generated-api';
import { useI18n } from './i18n';

export default function AnalysisDecisionSummary({
  decision,
  source,
}: {
  decision: Pick<
    components['schemas']['Decision'],
    'played' | 'best' | 'equity_loss' | 'evaluation_status'
  >;
  source: string;
}) {
  const text = useCandidateText();
  const { t, formatDecision, formatNumber } = useI18n();
  return (
    <div className="analysis-saved-decision">
      <p className="fineprint">
        {source} · {text('normalizedCubeful')}
      </p>
      <dl>
        <dt>{t('common.played')}</dt>
        <dd>{formatDecision(decision.played)}</dd>
        <dt>{t('matchAnalysis.bestDecision')}</dt>
        <dd>{formatDecision(decision.best)}</dd>
        <dt>{t('matchAnalysis.loss')}</dt>
        <dd>
          {decision.evaluation_status === 'evaluated'
            ? formatNumber(decision.equity_loss, {
                minimumFractionDigits: 3,
                maximumFractionDigits: 3,
              })
            : '—'}
        </dd>
      </dl>
    </div>
  );
}
