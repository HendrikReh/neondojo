import { useI18n } from '../../../shared/i18n/i18n';
import type { Match, MatchGame } from '../../../shared/http/api';
import { useAnalysisCatalogText } from '../analysis-catalog-messages';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import MatchAnalysisComparison from './MatchAnalysisComparison';
import MatchDecisionFilters from './MatchDecisionFilters';
import MatchStatisticsView from './MatchStatisticsView';
import ReviewEmptyFeedback from './ReviewEmptyFeedback';
import type { MatchReview } from './useMatchReview';

/** The Cube review and the statistics of the displayed analysis. */
export default function MatchAnalysisPanel({
  review,
  match,
  games,
  gameNo,
  catalog,
  showResults,
  showStatistics,
}: {
  review: MatchReview;
  match: Match;
  games?: MatchGame[];
  gameNo?: number;
  catalog: AnalysisCatalog;
  showResults: boolean;
  showStatistics: boolean;
}) {
  const { t } = useI18n();
  const catalogText = useAnalysisCatalogText();
  const { scoped, analysisId } = review;
  const names = [match.white, match.black];
  return (
    <>
      <div
        id="inspector-panel-errors"
        role="tabpanel"
        aria-labelledby="inspector-tab-errors"
        hidden={!showResults}
      >
        <section
          className="match-analysis"
          data-job-id={review.job?.id}
          aria-label={t(scoped ? 'gameAnalysis.label' : 'matchAnalysis.label')}
        >
          {showResults && (
            <MatchDecisionFilters
              review={review}
              match={match}
              games={games}
              gameNo={gameNo}
            />
          )}
          {!analysisId && !catalog.loading && !catalog.saving && (
            <p className="analysis-empty" role="status">
              {catalogText(catalog.selectedId ? 'unavailable' : 'none')}
            </p>
          )}
          <ReviewEmptyFeedback review={review} catalog={catalog} />
          {(review.loading || review.emptyPending) && (
            <p role="status">{t('matchAnalysis.loading')}</p>
          )}
        </section>
      </div>
      <MatchStatisticsView
        analysisEntry={catalog.selected}
        statistics={review.statistics}
        names={names}
        onSelect={review.selectMetric}
        onGame={review.onGame}
        hidden={!showStatistics}
        loading={catalog.loading || catalog.saving || review.statisticsLoading}
        error={review.statisticsError}
        comparison={
          match.source_available &&
          (!scoped || gameNo !== undefined) && (
            <MatchAnalysisComparison
              key={match.id}
              matchId={match.id}
              catalog={catalog}
              gameNo={scoped ? gameNo : undefined}
              names={names}
              visible={showStatistics}
            />
          )
        }
      />
    </>
  );
}
