import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import type { Match, MatchGame } from '../../../shared/http/api';
import {
  severityLabels,
  useStatisticsText,
} from '../../../shared/review/review-statistics-messages';
import type { MatchReview } from './useMatchReview';

/** The review filters. Only one instance may be mounted at a time. */
export default function MatchDecisionFilters({
  review,
  match,
  games,
  gameNo,
}: {
  review: MatchReview;
  match: Match;
  games?: MatchGame[];
  gameNo?: number;
}) {
  const { t, formatNumber } = useI18n();
  const text = useStatisticsText();
  const { scoped } = review;
  const {
    player,
    severity,
    game_no: filteredGame,
    errors_only: errorsOnly,
  } = review.selected;
  return (
    <div className="match-analysis-filters">
      <label>
        <input
          ref={review.errorsToggle}
          type="checkbox"
          checked={errorsOnly}
          onChange={(event) =>
            review.updateFilter({
              errors_only: event.target.checked,
              ...(event.target.checked && severity === 'none'
                ? { severity: '' }
                : {}),
            })
          }
        />{' '}
        {t('matchAnalysis.errorsOnly', {
          threshold: formatNumber(review.thresholds.questionable, {
            maximumFractionDigits: 20,
          }),
        })}
      </label>
      <select
        aria-label={t('matchAnalysis.filterPlayer')}
        value={player}
        onChange={(event) =>
          review.updateFilter({ player: event.target.value })
        }
      >
        <option value="">{t('matchAnalysis.bothPlayers')}</option>
        <option value="0">{match.white}</option>
        <option value="1">{match.black}</option>
      </select>
      <select
        aria-label={text('filterSeverity')}
        value={severity ?? ''}
        onChange={(event) =>
          review.updateFilter({
            severity: event.target.value,
            ...(event.target.value === 'none' ? { errors_only: false } : {}),
          })
        }
      >
        <option value="">{text('allSeverities')}</option>
        {Object.entries(severityLabels).map(([value, key]) => (
          <option key={value} value={value}>
            {t(key)}
          </option>
        ))}
      </select>
      <select
        aria-label={text('filterGame')}
        value={filteredGame ?? ''}
        onChange={(event) => {
          const game = event.target.value
            ? Number(event.target.value)
            : undefined;
          review.updateFilter({ game_no: game });
          if (game !== undefined) review.onGame(game);
        }}
      >
        {!scoped && <option value="">{text('allGames')}</option>}
        {(
          (scoped ? undefined : games) ??
          review.statistics?.games ??
          (scoped && gameNo !== undefined ? [{ game_no: gameNo }] : [])
        ).map((game) => (
          <option key={game.game_no} value={game.game_no}>
            {text('game', { game: game.game_no })}
          </option>
        ))}
      </select>
      <ActionButton
        ref={review.resetButton}
        icon="clear"
        label={text('resetFilters')}
        compact
        type="button"
        className="statistics-reset"
        onClick={review.resetFilters}
      />
    </div>
  );
}
