import ActionButton from '../../../shared/ui/ActionButton';
import EngineLabel from '../../../shared/engine/EngineLabel';
import { useId, type ReactNode } from 'react';
import type { components } from '../../../generated-api';
import type {
  AnalysisCatalogEntry,
  MatchAnalysisStatistics,
} from '../analysis-catalog-api';
import { useAnalysisCatalogText } from '../analysis-catalog-messages';
type MatchAnalysisSummary = components['schemas']['CatalogAnalysisSummary'];
import { useI18n } from '../../../shared/i18n/i18n';
import {
  nativePerformanceRating,
  type DecisionKind,
  type PlayerErrorBreakdown,
  type StatisticsAction,
  type StatisticsMetric,
} from './match-statistics-api';
import { errorGrades } from '../../../shared/review/move-severity';
import {
  severityLabels,
  useStatisticsText,
} from '../../../shared/review/review-statistics-messages';
import './match-statistics.css';
import { prDigits } from '../../../shared/format/pr-format';

type PlayerStats = MatchAnalysisSummary['players'][number];
type SelectMetric = (action: StatisticsAction) => void;

function PlayerStatisticsCard({
  summary,
  stats,
  name,
  gameNo,
  onSelect,
  compact = false,
  errorBreakdown,
}: {
  summary: MatchAnalysisSummary;
  stats: PlayerStats;
  name: string;
  gameNo?: number;
  onSelect: SelectMetric;
  compact?: boolean;
  errorBreakdown?: PlayerErrorBreakdown;
}) {
  const text = useStatisticsText();
  const { formatNumber, t } = useI18n();
  const metricId = useId();
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const metric = (
    key: StatisticsMetric,
    label: string,
    value: string | number | undefined,
    kind?: DecisionKind,
    cell = false,
  ) => {
    const actionLabel = kind ? `${text(kind)} · ${label}` : label;
    const accessibleLabel =
      gameNo === undefined
        ? text('metricAction', { player: name, metric: actionLabel })
        : text('metricGameAction', {
            game: gameNo,
            player: name,
            metric: actionLabel,
          });
    if (value === undefined)
      return (
        <span
          className="statistics-error-unavailable"
          role="img"
          title={text('typedUnavailable')}
          aria-label={`${accessibleLabel}: ${text('typedUnavailable')}`}
        >
          —
        </span>
      );
    const decisionKind = key === 'checker' || key === 'cube' ? key : kind;
    const select = (kind: DecisionKind) =>
      onSelect({ player: stats.player, metric: key, gameNo, kind });
    const popoverId = decisionKind ? undefined : `${metricId}-${key}`;
    const button = (
      <button
        type="button"
        className="statistics-metric"
        data-statistics-metric={key}
        data-statistics-kind={kind}
        aria-label={accessibleLabel}
        popoverTarget={popoverId}
        onClick={decisionKind ? () => select(decisionKind) : undefined}
      >
        {!cell && <span>{label}</span>}
        <strong>
          {typeof value === 'number' ? formatNumber(value) : value}
        </strong>
      </button>
    );
    return popoverId ? (
      <div className="statistics-metric-choice">
        {button}
        <div
          id={popoverId}
          popover="auto"
          className="statistics-destinations"
          role="group"
          aria-label={accessibleLabel}
        >
          {(['checker', 'cube'] as const).map((destination) => (
            <button
              key={destination}
              type="button"
              popoverTarget={popoverId}
              popoverTargetAction="hide"
              onClick={() => select(destination)}
            >
              {t(destination === 'cube' ? 'common.cube' : 'common.checker')}
            </button>
          ))}
        </div>
      </div>
    ) : (
      button
    );
  };
  const rating = nativePerformanceRating(summary, stats.player);
  const gradeTotal = (grade: (typeof errorGrades)[number]) =>
    errorBreakdown
      ? (errorBreakdown.checker[grade] ?? 0) + (errorBreakdown.cube[grade] ?? 0)
      : undefined;
  const errorTotal = errorBreakdown
    ? errorGrades.reduce((sum, grade) => sum + (gradeTotal(grade) ?? 0), 0)
    : undefined;
  return (
    <article
      className={`statistics-player${compact ? ' statistics-player-compact' : ''}`}
    >
      {!compact && <h4>{name}</h4>}
      <div className="statistics-metrics">
        {metric(
          'decisions',
          text('decisions'),
          stats.checker_decisions + stats.cube_decisions,
        )}
        {metric('checker', text('checker'), stats.checker_decisions)}
        {metric('cube', text('cube'), stats.cube_decisions)}
        {metric(
          'loss',
          text('loss'),
          decimal(stats.checker_equity_loss + stats.cube_equity_loss),
        )}
        {rating === undefined ? (
          <div className="statistics-metric statistics-metric-unavailable">
            <span>{text('pr')}</span>
            <strong>{text('unavailable')}</strong>
          </div>
        ) : (
          metric('pr', text('pr'), formatNumber(rating, prDigits))
        )}
      </div>
      <div
        role="table"
        className="statistics-error-matrix"
        aria-label={text('errorsByKind')}
      >
        <div role="rowgroup">
          <div role="row">
            <span role="columnheader">{text('errors')}</span>
            <span role="columnheader">{text('total')}</span>
            <span role="columnheader">{text('checker')}</span>
            <span role="columnheader">{text('cube')}</span>
          </div>
        </div>
        <div role="rowgroup">
          {(
            [
              ['inaccuracies', text('inaccuracies'), errorTotal],
              ...errorGrades.map(
                (grade) =>
                  [grade, t(severityLabels[grade]), gradeTotal(grade)] as const,
              ),
            ] as const
          ).map(([key, label, count]) => (
            <div role="row" key={key}>
              <span role="rowheader">{label}</span>
              <div role="cell">
                {metric(key, label, count, undefined, true)}
              </div>
              {(['checker', 'cube'] as const).map((kind) => {
                const counts = errorBreakdown?.[kind];
                const value =
                  counts &&
                  (key === 'inaccuracies'
                    ? errorGrades.reduce(
                        (sum, grade) => sum + (counts[grade] ?? 0),
                        0,
                      )
                    : (counts[key] ?? 0));
                return (
                  <div role="cell" key={kind}>
                    {metric(key, label, value, kind, true)}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>
      {(stats.forced_moves > 0 || stats.unavailable > 0) && (
        <p className="statistics-footnote">
          {stats.forced_moves > 0 && (
            <span>{text('forced', { count: stats.forced_moves })}</span>
          )}
          {stats.unavailable > 0 && (
            <span>
              {text('unavailableDecisions', {
                count: stats.unavailable,
              })}
            </span>
          )}
        </p>
      )}
    </article>
  );
}

function PlayerStatisticsSummary({
  summary,
  names,
  onSelect,
  gameNo,
  errorBreakdown,
}: {
  summary: MatchAnalysisSummary;
  names: string[];
  onSelect: SelectMetric;
  gameNo?: number;
  errorBreakdown?: PlayerErrorBreakdown[] | null;
}) {
  return (
    <div className="match-analysis-summary statistics-summary">
      {summary.players.map((stats) => (
        <PlayerStatisticsCard
          key={stats.player}
          summary={summary}
          stats={stats}
          name={names[stats.player]}
          onSelect={onSelect}
          gameNo={gameNo}
          errorBreakdown={errorBreakdown?.find(
            (entry) => entry.player === stats.player,
          )}
        />
      ))}
    </div>
  );
}

export default function MatchStatisticsView({
  analysisEntry,
  statistics,
  names,
  onSelect,
  onGame,
  hidden = false,
  loading = false,
  error,
  comparison,
}: {
  analysisEntry?: AnalysisCatalogEntry;
  statistics?: MatchAnalysisStatistics;
  names: string[];
  onSelect: SelectMetric;
  onGame?: (gameNo: number) => void;
  hidden?: boolean;
  loading?: boolean;
  error?: unknown;
  comparison?: ReactNode;
}) {
  const text = useStatisticsText();
  const { formatError } = useI18n();
  const catalogText = useAnalysisCatalogText();
  const current =
    analysisEntry && statistics?.analysis_id === analysisEntry.id
      ? statistics
      : undefined;
  const job =
    analysisEntry?.job?.kind === 'match' ? analysisEntry.job : undefined;
  const summary = current?.summary;
  const engine = summary?.engine ?? analysisEntry?.engine;
  return (
    <section
      className="match-statistics"
      id="inspector-panel-statistics"
      role="tabpanel"
      aria-labelledby="inspector-tab-statistics"
      hidden={hidden}
      data-job-id={job?.id}
    >
      <h3>{text('title')}</h3>
      {comparison}
      {!analysisEntry ? (
        <p className="analysis-empty">{catalogText('none')}</p>
      ) : (
        <>
          <p className="statistics-source">
            <EngineLabel
              details={[engine, analysisEntry.depth]
                .filter(Boolean)
                .join(' · ')}
            />
          </p>
          {(summary || job) && (
            <p className="statistics-coverage">
              {text('coverage', {
                completed:
                  summary?.completed_games ?? job?.completed_games ?? 0,
                total: summary?.total_games ?? job?.total_games ?? 0,
              })}
            </p>
          )}
          {summary && (
            <>
              <p className="statistics-coverage">
                {text('decisionsCoverage', {
                  analyzed: summary.analyzed_moves,
                  total: summary.total_moves,
                })}
              </p>
              <PlayerStatisticsSummary
                summary={summary}
                errorBreakdown={current?.error_breakdown}
                names={names}
                onSelect={onSelect}
                gameNo={analysisEntry.game_no ?? undefined}
              />
              <p className="analysis-method">{text('prMethod')}</p>
            </>
          )}
          {loading && <p role="status">{text('loading')}</p>}
          {!!error && <p role="alert">{formatError(error)}</p>}
          {!summary && !loading && !error && (
            <p className="analysis-empty">{text('noSummary')}</p>
          )}
          {!!current?.games.length && (
            <div className="statistics-games-scroll">
              <table className="statistics-games-table">
                <caption>{text('games')}</caption>
                <thead>
                  <tr>
                    <th>{text('games')}</th>
                    <th>{text('state')}</th>
                    {names.map((name, index) => (
                      <th key={index}>{name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {current.games.map((game) => (
                    <tr key={game.game_no}>
                      <th scope="row">
                        {onGame ? (
                          <ActionButton
                            icon="open"
                            label={text('openGame', { game: game.game_no })}
                            type="button"
                            className="position-link"
                            aria-label={text('openGame', {
                              game: game.game_no,
                            })}
                            onClick={() => onGame(game.game_no)}
                          >
                            {text('game', { game: game.game_no })}
                          </ActionButton>
                        ) : (
                          text('game', { game: game.game_no })
                        )}
                        <small>
                          {text('score')}: {game.white_score}:{game.black_score}
                        </small>
                        {game.crawford && <small>{text('crawford')}</small>}
                      </th>
                      <td>
                        {text(game.analyzed ? 'analyzed' : 'notAnalyzed')}
                      </td>
                      {names.map((name, player) => {
                        const stats = game.summary?.players.find(
                          (entry) => entry.player === player,
                        );
                        return (
                          <td key={player}>
                            {game.analyzed && game.summary && stats ? (
                              <PlayerStatisticsCard
                                compact
                                errorBreakdown={game.error_breakdown?.find(
                                  (entry) => entry.player === player,
                                )}
                                summary={game.summary}
                                stats={stats}
                                name={name}
                                gameNo={game.game_no}
                                onSelect={onSelect}
                              />
                            ) : (
                              <span className="statistics-missing">—</span>
                            )}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </>
      )}
    </section>
  );
}
