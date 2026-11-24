import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import { useStatisticsText } from '../../../shared/review/review-statistics-messages';
import { useAnalysisCatalogText } from '../analysis-catalog-messages';
import { useSourceText } from '../candidates/analysis-source-messages';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import type { MatchReview } from './useMatchReview';

/** Explains why the review has no decisions, with the action that helps. */
export default function ReviewEmptyFeedback({
  review,
  catalog,
}: {
  review: MatchReview;
  catalog: AnalysisCatalog;
}) {
  const { t, formatError } = useI18n();
  const text = useStatisticsText();
  const sourceText = useSourceText();
  const catalogText = useAnalysisCatalogText();
  const { emptyReason: reason } = review;
  const error = review.pageError || review.emptyError;
  if (reason === 'missing')
    return (
      <div className="analysis-empty review-empty">
        <p role="status">
          {text('missing')}{' '}
          {catalog.entries.some((entry) => entry.id !== review.analysisId) &&
            catalogText('alternatives')}
        </p>
      </div>
    );
  if (reason === 'filtered' || reason === 'clean') {
    const filtered = reason === 'filtered';
    return (
      <div className="analysis-empty review-empty">
        <p role="status">
          {filtered ? t('matchAnalysis.noDecisions') : text('clean')}
        </p>
        <ActionButton
          type="button"
          icon={filtered ? 'clear' : 'view'}
          label={text(filtered ? 'removeSelectionFilters' : 'showAllDecisions')}
          onClick={
            filtered ? review.removeSelectionFilters : review.showAllDecisions
          }
        />
      </div>
    );
  }
  if (!error) return null;
  return (
    <div className="analysis-empty review-empty">
      <p className="analysis-error" role="alert">
        {formatError(error)}
      </p>
      <ActionButton
        type="button"
        icon="refresh"
        label={sourceText('retry')}
        onClick={review.retry}
      />
    </div>
  );
}
