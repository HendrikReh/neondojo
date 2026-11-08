import type { ReactNode } from 'react';
import type { Job, MatchAnalysisSummary } from './api';
import { useI18n } from './i18n';
import {
  nativePerformanceRating,
  type JobAnalysisStatistics,
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
}: {
  summary: MatchAnalysisSummary;
  stats: PlayerStats;
  name: string;
  gameNo?: number;
  onSelect: SelectMetric;
  compact?: boolean;
}) {
  const text = useStatisticsText();
  const { formatNumber } = useI18n();
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const metric = (
    key: StatisticsMetric,
    label: string,
    value: string | number,
  ) => (
    <button
      type="button"
      className="statistics-metric"
      data-statistics-metric={key}
      aria-label={
        gameNo === undefined
          ? text('metricAction', { player: name, metric: label })
          : text('metricGameAction', {
              game: gameNo,
              player: name,
              metric: label,
            })
      }
      onClick={() => onSelect({ player: stats.player, metric: key, gameNo })}
    >
      <span>{label}</span>
      <strong>{typeof value === 'number' ? formatNumber(value) : value}</strong>
    </button>
  );
  const rating = nativePerformanceRating(summary, stats.player);
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
        {metric(
          'inaccuracies',
          text('inaccuracies'),
          stats.doubtful + stats.bad + stats.very_bad,
        )}
        {metric('doubtful', text('doubtful'), stats.doubtful)}
        {metric('bad', text('bad'), stats.bad)}
        {metric('very_bad', text('veryBad'), stats.very_bad)}
        {rating === undefined ? (
          <div className="statistics-metric statistics-metric-unavailable">
            <span>{text('pr')}</span>
            <strong>{text('unavailable')}</strong>
          </div>
        ) : (
          metric('pr', text('pr'), decimal(rating))
        )}
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

export function PlayerStatisticsSummary({
  summary,
  names,
  onSelect,
  gameNo,
}: {
  summary: MatchAnalysisSummary;
  names: string[];
  onSelect: SelectMetric;
  gameNo?: number;
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
  overview = false,
}: {
  job?: Job;
  overview?: boolean;
  statistics?: JobAnalysisStatistics;
  names: string[];
  onSelect: SelectMetric;
  onGame?: (gameNo: number) => void;
  hidden?: boolean;
  loading?: boolean;
  error?: unknown;
  progress?: ReactNode;
}) {
  const text = useStatisticsText();
  const { formatNumber, formatError } = useI18n();
  // Defense in depth for callers: a new scope never displays an old job's payload.
  const current = job && statistics?.job_id === job.id ? statistics : undefined;
  const summary = current?.summary;
  return (
    <section
      className={`match-statistics${overview ? ' overview-statistics' : ''}`}
      id="inspector-panel-statistics"
      role={overview ? 'region' : 'tabpanel'}
      aria-labelledby="inspector-tab-statistics"
      hidden={hidden}
      data-job-id={job?.id}
    >
      <h3>{text('title')}</h3>
      {!job ? (
        <p className="analysis-empty">{text('noJob')}</p>
      ) : (
        <>
          {progress}
          <p className="statistics-source">
            {text('source', {
              engine: summary?.engine ?? 'GNU Backgammon',
              plies: summary?.plies ?? job.plies,
            })}
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
                names={names}
                onSelect={onSelect}
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
                          <button
                            type="button"
                            className="position-link"
                            aria-label={text('openGame', {
                              game: game.game_no,
                            })}
                            onClick={() => onGame(game.game_no)}
                          >
                            {text('game', { game: game.game_no })} ↗
                          </button>
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
