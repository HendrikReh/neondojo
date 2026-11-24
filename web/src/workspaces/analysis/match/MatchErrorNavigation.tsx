import { BoardErrorNavigation } from '../BoardDecisionReview';
import { useI18n } from '../../../shared/i18n/i18n';
import type { Match } from '../../../shared/http/api';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import ReviewEmptyFeedback from './ReviewEmptyFeedback';
import type { MatchReview } from './useMatchReview';

/** Steps the board through the decisions of the review. */
export default function MatchErrorNavigation({
  review,
  catalog,
  match,
}: {
  review: MatchReview;
  catalog: AnalysisCatalog;
  match: Match;
}) {
  const { formatError } = useI18n();
  const { navigation } = review;
  if (!navigation) return null;
  const { player, severity, game_no, errors_only } = review.selected;
  return (
    <BoardErrorNavigation
      game={game_no}
      player={player ? [match.white, match.black][Number(player)] : undefined}
      kind={navigation.kind}
      severity={severity}
      errorsOnly={errors_only}
      total={navigation.total}
      ordinal={navigation.ordinal}
      busy={navigation.busy}
      onNavigate={navigation.onNavigate}
    >
      {navigation.showEmptyFeedback && (
        <ReviewEmptyFeedback review={review} catalog={catalog} />
      )}
      {!!navigation.error && (
        <small role="alert">{formatError(navigation.error)}</small>
      )}
    </BoardErrorNavigation>
  );
}
