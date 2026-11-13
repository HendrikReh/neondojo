import ActionButton from './ActionButton';
import EngineLabel from './EngineLabel';
import type { ReactNode } from 'react';
import type { Job, MatchAnalysisSummary } from './api';
import { useI18n } from './i18n';
import {
  nativePerformanceRating,
  type JobAnalysisStatistics,
  type PlayerErrorBreakdown,
  type StatisticsAction,
  type StatisticsMetric,
} from './match-statistics-api';
import { useStatisticsText } from './review-statistics-messages';
import './match-statistics.css';

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
  const { formatNumber } = useI18n();
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const metric = (
    key: StatisticsMetric,
    label: string,
    value: string | number | undefined,
    kind?: 'checker' | 'cube',
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
    return (
      <button
        type="button"
        className="statistics-metric"
        data-statistics-metric={key}
        data-statistics-kind={kind}
        aria-label={accessibleLabel}
        onClick={() =>
          onSelect({
            player: stats.player,
            metric: key,
            gameNo,
            ...(kind ? { kind } : {}),
          })
        }
      >
        {!cell && <span>{label}</span>}
        <strong>
          {typeof value === 'number' ? formatNumber(value) : value}
        </strong>
      </button>
    );
  };
  const rating = nativePerformanceRating(summary, stats.player);
  const errorTotals = errorBreakdown
    ? {
        doubtful:
          errorBreakdown.checker.doubtful + errorBreakdown.cube.doubtful,
        bad: errorBreakdown.checker.bad + errorBreakdown.cube.bad,
        very_bad:
          errorBreakdown.checker.very_bad + errorBreakdown.cube.very_bad,
      }
    : stats;
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
          metric('pr', text('pr'), decimal(rating))
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
              [
                'inaccuracies',
                text('inaccuracies'),
                errorTotals.doubtful + errorTotals.bad + errorTotals.very_bad,
              ],
              ['doubtful', text('doubtful'), errorTotals.doubtful],
              ['bad', text('bad'), errorTotals.bad],
              ['very_bad', text('veryBad'), errorTotals.very_bad],
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
                    ? counts.doubtful + counts.bad + counts.very_bad
                    : counts[key]);
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
            <span>
              {text('forced', { count: formatNumber(stats.forced_moves) })}
            </span>
          )}
          {stats.unavailable > 0 && (
            <span>
              {text('unavailableDecisions', {
                count: formatNumber(stats.unavailable),
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
  job,
  statistics,
  names,
  onSelect,
  onGame,
  hidden = false,
  loading = false,
  error,
  progress,
  comparison,
}: {
  job?: Job;
  statistics?: JobAnalysisStatistics;
  names: string[];
  onSelect: SelectMetric;
  onGame?: (gameNo: number) => void;
  hidden?: boolean;
  loading?: boolean;
  error?: unknown;
  progress?: ReactNode;
  comparison?: ReactNode;
}) {
  const text = useStatisticsText();
  const { formatNumber, formatError } = useI18n();
  // Defense in depth for callers: a new scope never displays an old job's payload.
  const current = job && statistics?.job_id === job.id ? statistics : undefined;
  const summary = current?.summary;
  const engine =
    summary?.engine ??
    job?.engine_snapshot?.engine_version ??
    (job?.engine_snapshot?.engine_id === 'bgsage' ? 'Open Sage' : 'GNU Backgammon');
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
      {!job ? (
        <p className="analysis-empty">{text('noJob')}</p>
      ) : (
        <>
          {progress}
          <p className="statistics-source">
            <EngineLabel
              details={text('source', {
                engine,
                plies: summary?.plies ?? job.plies,
              })}
            />
          </p>
          <p className="statistics-coverage">
            {text('coverage', {
              completed: formatNumber(
                summary?.completed_games ?? job.completed_games,
              ),
              total: formatNumber(summary?.total_games ?? job.total_games),
            })}
          </p>
          {summary && (
            <>
              <p className="statistics-coverage">
                {text('decisionsCoverage', {
                  analyzed: formatNumber(summary.analyzed_moves),
                  total: formatNumber(summary.total_moves),
                })}
              </p>
              <PlayerStatisticsSummary
                summary={summary}
                errorBreakdown={current?.error_breakdown}
                names={names}
                onSelect={onSelect}
                gameNo={job.game_no ?? undefined}
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
