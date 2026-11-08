import { useEffect, useState } from 'react';
import {
  request,
  terminal,
  type Match,
  type MatchAnalysisSummary,
  type MoveAnalysis,
  type MoveAnalysisPage,
} from './api';
import { useMatchAnalysis } from './useMatchAnalysis';
import { useI18n, type TranslationKey } from './i18n';

const severityKeys: Record<string, TranslationKey> = {
  none: 'severity.none',
  doubtful: 'severity.doubtful',
  bad: 'severity.bad',
  very_bad: 'severity.very_bad',
};

export default function MatchAnalysisPanel({
  match,
  onPosition,
}: {
  match: Match;
  onPosition: (id: string) => void;
}) {
  const {
    t,
    formatNumber,
    formatPercent,
    formatEquity,
    formatDecision,
    formatError,
  } = useI18n();
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const { job, error, submitting, run } = useMatchAnalysis(match.id);
  const [plies, setPlies] = useState(0);
  const [errorsOnly, setErrorsOnly] = useState(false);
  const [player, setPlayer] = useState('');
  const [kind, setKind] = useState('');
  const [rows, setRows] = useState<MoveAnalysis[]>([]);
  const [cursor, setCursor] = useState<number | null>(null);
  const [pageError, setPageError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [page, setPage] = useState<number | null>(null);
  const [version, setVersion] = useState(0);
  const active = job && !terminal(job);
  const summary = job?.result as MatchAnalysisSummary | undefined;
  const names = [match.white, match.black];
  const filter = `errors_only=${errorsOnly}${player ? `&player=${player}` : ''}${kind ? `&kind=${kind}` : ''}`;

  // A completed game changes the result set atomically. Start a fresh page chain.
  useEffect(() => {
    setPage(null);
    setRows([]);
    setVersion((n) => n + 1);
  }, [job?.id, job?.completed_games, filter]);
  useEffect(() => {
    if (!job?.id) return;
    const abort = new AbortController();
    setLoading(true);
    setPageError(null);
    void request<MoveAnalysisPage>(
      `/jobs/${job.id}/moves?limit=50&${filter}${page ? `&after=${page}` : ''}`,
      { signal: abort.signal },
    )
      .then((result) => {
        if (abort.signal.aborted) return;
        setRows((previous) =>
          page ? [...previous, ...result.items] : result.items,
        );
        setCursor(result.next_cursor ?? null);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setPageError(error);
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [job?.id, filter, page, version]);

  return (
    <section
      className="match-analysis"
      data-job-id={job?.id}
      aria-label={t('matchAnalysis.label')}
    >
      <div className="match-analysis-heading">
        <div>
          <span className="eyebrow">{t('matchAnalysis.eyebrow')}</span>
          <h3>{t('matchAnalysis.title')}</h3>
          <p>{t('matchAnalysis.description')}</p>
        </div>
        <div className="match-analysis-controls">
          <label>
            {t('matchAnalysis.depth')}{' '}
            <select
              aria-label={t('matchAnalysis.depthLabel')}
              value={plies}
              disabled={!!active || submitting}
              onChange={(event) => setPlies(Number(event.target.value))}
            >
              <option value={0}>{t('matchAnalysis.fast')}</option>
              <option value={1}>1-Ply</option>
              <option value={2}>{t('matchAnalysis.thorough')}</option>
            </select>
          </label>
          <button
            className="primary"
            onClick={() => void run(plies)}
            disabled={!!active || submitting}
          >
            {submitting
              ? t('matchAnalysis.submitting')
              : active
                ? t('matchAnalysis.running')
                : job
                  ? t('matchAnalysis.again')
                  : t('matchAnalysis.start')}
          </button>
        </div>
      </div>
      {!!error && (
        <p className="analysis-error" role="alert">
          {formatError(error)}
        </p>
      )}
      {job && (
        <>
          <div
            className="match-analysis-progress"
            role="status"
            aria-live="polite"
          >
            <div>
              <strong>
                {job.status === 'completed'
                  ? t('matchAnalysis.completed')
                  : job.status === 'failed'
                    ? t('matchAnalysis.failed')
                    : job.status === 'queued'
                      ? t('matchAnalysis.queued')
                      : t('matchAnalysis.engineRunning')}
              </strong>
              <span>
                {t('matchAnalysis.progressText', {
                  completed: job.completed_games,
                  total: job.total_games,
                  plies: job.plies,
                  progress: formatPercent(job.progress / 100),
                })}
              </span>
            </div>
            <progress
              aria-label={t('matchAnalysis.progressLabel')}
              value={job.progress}
              max={100}
            />
            {active && <small>{t('matchAnalysis.progressHint')}</small>}
            {job.error && (
              <p role="alert">{t('error.engine', { details: job.error })}</p>
            )}
          </div>
          {summary && (
            <>
              <div className="match-analysis-summary">
                {summary.players.map((stats) => (
                  <article key={stats.player}>
                    <h4>{names[stats.player]}</h4>
                    <div className="match-summary-numbers">
                      <span>
                        <b>
                          {formatNumber(
                            stats.doubtful + stats.bad + stats.very_bad,
                          )}
                        </b>{' '}
                        {t('matchAnalysis.inaccuracies')}
                      </span>
                      <span>
                        <b>
                          {decimal(
                            stats.checker_equity_loss + stats.cube_equity_loss,
                          )}
                        </b>{' '}
                        {t('matchAnalysis.equityLoss')}
                      </span>
                    </div>
                    <p>
                      {t('matchAnalysis.decisionsCount', {
                        moves: stats.checker_decisions,
                        cube: stats.cube_decisions,
                      })}
                    </p>
                    <small>
                      {t('matchAnalysis.severityCount', {
                        doubtful: stats.doubtful,
                        bad: stats.bad,
                        veryBad: stats.very_bad,
                      })}
                      {stats.unavailable
                        ? t('matchAnalysis.unavailableCount', {
                            count: stats.unavailable,
                          })
                        : ''}
                    </small>
                  </article>
                ))}
              </div>
              <p className="analysis-method">
                {t('matchAnalysis.method', {
                  analyzed: summary.analyzed_moves,
                  total: summary.total_moves,
                  doubtful: formatNumber(0.04),
                  bad: formatNumber(0.08),
                  veryBad: formatNumber(0.16),
                })}
              </p>
            </>
          )}
          <details className="match-move-overview" open>
            <summary>{t('matchAnalysis.overview')}</summary>
            <div className="match-analysis-filters">
              <label>
                <input
                  type="checkbox"
                  checked={errorsOnly}
                  onChange={(event) => setErrorsOnly(event.target.checked)}
                />{' '}
                {t('matchAnalysis.errorsOnly', {
                  threshold: formatNumber(0.04),
                })}
              </label>
              <select
                aria-label={t('matchAnalysis.filterPlayer')}
                value={player}
                onChange={(event) => setPlayer(event.target.value)}
              >
                <option value="">{t('matchAnalysis.bothPlayers')}</option>
                <option value="0">{match.white}</option>
                <option value="1">{match.black}</option>
              </select>
              <select
                aria-label={t('matchAnalysis.filterDecision')}
                value={kind}
                onChange={(event) => setKind(event.target.value)}
              >
                <option value="">{t('matchAnalysis.bothKinds')}</option>
                <option value="checker">{t('common.checker')}</option>
                <option value="cube">{t('common.cube')}</option>
              </select>
            </div>
            {!!pageError && <p role="alert">{formatError(pageError)}</p>}
            <div className="match-moves-scroll">
              <table className="match-moves-table">
                <thead>
                  <tr>
                    <th>{t('matchAnalysis.gameMove')}</th>
                    <th>{t('matchAnalysis.playerKind')}</th>
                    <th>{t('common.played')}</th>
                    <th>{t('matchAnalysis.bestDecision')}</th>
                    <th>{t('matchAnalysis.loss')}</th>
                    <th>{t('matchAnalysis.rating')}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id}>
                      <td>
                        {row.position_uuid ? (
                          <button
                            className="position-link"
                            aria-label={t('matchAnalysis.openPosition', {
                              game: row.game_no,
                              move: row.checker_no ?? '—',
                            })}
                            onClick={() => onPosition(row.position_uuid!)}
                          >
                            {formatNumber(row.game_no)} ·{' '}
                            {row.checker_no == null
                              ? '—'
                              : formatNumber(row.checker_no)}{' '}
                            ↗
                          </button>
                        ) : (
                          t('matchAnalysis.cubeMove', {
                            game: row.game_no,
                            decision: row.decision_no,
                          })
                        )}
                      </td>
                      <td>
                        {names[row.player]}
                        <small>
                          {row.kind === 'checker'
                            ? `${t('common.checker')} · ${row.dice?.join('–')}`
                            : t('common.cube')}
                        </small>
                      </td>
                      <td>
                        {formatDecision(row.played)}
                        <small>
                          {row.played_equity != null
                            ? formatEquity(row.played_equity)
                            : '—'}
                        </small>
                      </td>
                      <td>
                        {formatDecision(row.best)}
                        <small>
                          {row.best_equity != null
                            ? formatEquity(row.best_equity)
                            : '—'}
                        </small>
                      </td>
                      <td className="equity-loss">
                        {row.evaluation_status === 'unavailable'
                          ? '—'
                          : decimal(row.equity_loss)}
                      </td>
                      <td>
                        <span className={`severity severity-${row.severity}`}>
                          {row.evaluation_status === 'forced'
                            ? t('matchAnalysis.forced')
                            : row.evaluation_status === 'unavailable'
                              ? t('matchAnalysis.unavailable')
                              : severityKeys[row.severity]
                                ? t(severityKeys[row.severity])
                                : row.severity}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {rows.length === 0 && !loading && (
              <p className="analysis-empty">
                {job.completed_games === 0
                  ? t('matchAnalysis.firstGameHint')
                  : t('matchAnalysis.noDecisions')}
              </p>
            )}
            {loading && <p role="status">{t('matchAnalysis.loading')}</p>}
            {cursor && (
              <button onClick={() => setPage(cursor)} disabled={loading}>
                {t('matchAnalysis.more')}
              </button>
            )}
          </details>
        </>
      )}
    </section>
  );
}
