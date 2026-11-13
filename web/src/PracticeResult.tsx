import type { components } from './generated-api';
import EngineLabel from './EngineLabel';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';

const verdictKeys = {
  best: 'bestAnswer',
  playable: 'verdictPlayable',
  error: 'verdictError',
  forced: 'verdictForced',
} as const;

export default function PracticeResult({
  evaluation: result,
  storedSource,
}: {
  evaluation: components['schemas']['PracticeEvaluation'];
  storedSource?: string;
}) {
  const t = useLearningText();
  const { formatDecision, formatEquity, formatNumber } = useI18n();
  const { provenance } = result;
  const source =
    provenance.kind === 'stored' && storedSource
      ? storedSource
      : [
          provenance.engine,
          provenance.plies == null
            ? null
            : `${formatNumber(provenance.plies)}-ply`,
        ]
          .filter(Boolean)
          .join(' · ');
  return (
    <>
      <p>
        <strong>{t(verdictKeys[result.verdict])}</strong>
      </p>
      <p>
        {t('chosenAnswer')}: {formatDecision(result.chosen)}
        {result.chosen_equity != null &&
          ` · ${formatEquity(result.chosen_equity)}`}
      </p>
      <p>
        {t('best')}: {formatDecision(result.best)}
        {result.best_equity != null && ` · ${formatEquity(result.best_equity)}`}
      </p>
      <p>
        {t('loss')}:{' '}
        {formatNumber(result.equity_loss, {
          minimumFractionDigits: 3,
          maximumFractionDigits: 6,
        })}
      </p>
      {provenance.kind !== 'forced' && (
        <p>
          {t(
            provenance.kind === 'stored' ? 'sourceStored' : 'sourceRecomputed',
          )}
          {source && (
            <>
              {' · '}
              <EngineLabel
                details={provenance.engine ?? source}
                tooltip={source !== provenance.engine ? source : undefined}
              />
            </>
          )}
        </p>
      )}
    </>
  );
}
