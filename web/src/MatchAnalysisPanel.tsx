import AnalysisRemainingTime from './AnalysisRemainingTime';
import AnalysisTimeEstimate from './AnalysisTimeEstimate';
import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  request,
  terminal,
  type Job,
  type Match,
  type MatchAnalysisSummary,
  type MoveAnalysis,
  type MoveAnalysisPage,
} from './api';
import { useMatchAnalysis } from './useMatchAnalysis';
import { useI18n, type TranslationKey } from './i18n';
import MatchStatisticsView, {
  PlayerStatisticsSummary,
} from './MatchStatisticsView';
import {
  buildStatisticsFilter,
  decisionFilterQuery,
  statisticsRevision,
  type JobAnalysisStatistics,
  type MatchDecisionFilter,
  type StatisticsAction,
} from './match-statistics-api';
import { useStatisticsText } from './review-statistics-messages';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';

const defaultFilter: MatchDecisionFilter = {
  player: '',
  kind: '',
  severity: '',
  errors_only: true,
};

const severityKeys: Record<string, TranslationKey> = {
  none: 'severity.none',
  doubtful: 'severity.doubtful',
  bad: 'severity.bad',
  very_bad: 'severity.very_bad',
};

export default function MatchAnalysisPanel({
  match,
  gameNo,
  onPosition,
  controlsTarget,
  showResults = true,
  showStatistics = false,
  onShowErrors,
  onGame,
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  configured = true,
  analysisInstallationKey,
  onJob,
  onAccepted,
  onFilterChange,
  overview = false,
  selectedPositionId,
}: {
  match: Match;
  overview?: boolean;
  selectedPositionId?: string;
  gameNo?: number;
  onPosition: (id: string) => void;
  controlsTarget?: HTMLElement | null;
  showResults?: boolean;
  showStatistics?: boolean;
  onShowErrors?: () => void;
  onGame?: (gameNo: number) => void;
  defaultPlies?: number;
  configured?: boolean;
  analysisInstallationKey?: string;
  onJob?: (job: Job | undefined) => void;
  onAccepted?: (job: Job) => void;
  onFilterChange?: (filter: {
    player: string;
    kind: string;
    severity?: string;
    game_no?: number;
  }) => void;
}) {
  const profile = useOptionalConfiguration();
  const {
    t,
    formatNumber,
    formatPercent,
    formatEquity,
    formatDecision,
    formatError,
  } = useI18n();
  const text = useStatisticsText();
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const [scope, setScope] = useProfileValue<'game' | 'match'>(
    (c) => c.analysis.scope,
    (c, scope) => ({ ...c, analysis: { ...c.analysis, scope } }),
    'game',
  );
  const scoped = scope === 'game';
  const { job, error, submitting, run } = useMatchAnalysis(
    match.id,
    scoped ? gameNo : undefined,
    !scoped || gameNo !== undefined,
    onAccepted,
  );
  const [plies, setPlies] = useState(defaultPlies);
  useEffect(() => setPlies(defaultPlies), [defaultPlies]);
  useEffect(() => onJob?.(job), [job, onJob]);
  const filterScope = `${match.id}:${scoped ? (gameNo ?? 'none') : 'match'}`;
  const [selection, setLocalSelection] = useState<{
    scope: string;
    filter: MatchDecisionFilter;
  }>({ scope: filterScope, filter: defaultFilter });
  useEffect(
    () => setLocalSelection({ scope: filterScope, filter: defaultFilter }),
    [filterScope],
  );
  const selected = {
    ...(profile?.current?.review ??
      (selection.scope === filterScope ? selection.filter : defaultFilter)),
    ...(selection.scope === filterScope &&
    selection.filter.game_no !== undefined
      ? { game_no: selection.filter.game_no }
      : {}),
    // A single-game job always has this scope, including after reset or a
    // metric action. Keep the visible selection and request explicit.
    ...(scoped ? { game_no: gameNo } : {}),
  };
  const {
    player,
    kind,
    severity,
    game_no: filteredGame,
    errors_only: errorsOnly,
  } = selected;
  const setSelection = (next: {
    scope: string;
    filter: MatchDecisionFilter;
  }) => {
    setLocalSelection(next);
    profile?.updateCurrent((c) => ({
      ...c,
      review: {
        player: next.filter.player,
        kind: next.filter.kind,
        severity: next.filter.severity ?? '',
        errors_only: next.filter.errors_only,
      },
    }));
  };
  const updateFilter = (patch: Partial<MatchDecisionFilter>) =>
    setSelection({ scope: filterScope, filter: { ...selected, ...patch } });
  function selectMetric(action: StatisticsAction) {
    setSelection({ scope: filterScope, filter: buildStatisticsFilter(action) });
    if (action.gameNo !== undefined && action.gameNo !== gameNo)
      onGame?.(action.gameNo);
    onShowErrors?.();
  }
  useEffect(
    () => onFilterChange?.({ player, kind, severity, game_no: filteredGame }),
    [player, kind, severity, filteredGame, onFilterChange],
  );
  const statisticsKey = job ? statisticsRevision(job) : '';
  const [statisticsResult, setStatisticsResult] = useState<{
    key: string;
    value: JobAnalysisStatistics;
  }>();
  const [statisticsError, setStatisticsError] = useState<{
    key: string;
    error: unknown;
  }>();
  const [statisticsLoaded, setStatisticsLoaded] = useState<string>();
  const statistics =
    statisticsResult?.key === statisticsKey
      ? statisticsResult.value
      : undefined;
  useEffect(() => {
    if (!job?.id) return;
    const abort = new AbortController();
    void request<JobAnalysisStatistics>(`/jobs/${job.id}/statistics`, {
      signal: abort.signal,
    })
      .then((value) => {
        if (abort.signal.aborted || value.job_id !== job.id) return;
        setStatisticsResult({ key: statisticsKey, value });
        setStatisticsError(undefined);
      })
      .catch((error) => {
        if (!abort.signal.aborted)
          setStatisticsError({ key: statisticsKey, error });
      })
      .finally(() => {
        if (!abort.signal.aborted) setStatisticsLoaded(statisticsKey);
      });
    return () => abort.abort();
  }, [job?.id, statisticsKey]);
  const [results, setResults] = useState<{
    key: string;
    rows: MoveAnalysis[];
    cursor: number | null;
  }>();
  const [page, setPage] = useState<{ key: string; cursor: number }>();
  const [pageError, setPageError] = useState<{ key: string; error: unknown }>();
  const [loaded, setLoaded] = useState<string>();
  const active = job && !terminal(job);
  const summary = job?.result as MatchAnalysisSummary | undefined;
  const names = [match.white, match.black];
  const filter = decisionFilterQuery(selected);
  // Filters, checkpoints and scope changes each start their own pagination chain.
  const resultKey = `${statisticsKey}:${filter}`;
  const after = page?.key === resultKey ? page.cursor : null;
  const requestKey = `${resultKey}:${after}`;
  const rows = results?.key === resultKey ? results.rows : [];
  const cursor = results?.key === resultKey ? results.cursor : null;
  const loading = !!job && loaded !== requestKey;
  useEffect(() => {
    setPage(undefined);
  }, [resultKey]);
  useEffect(() => {
    if (!job?.id) return;
    const abort = new AbortController();
    void request<MoveAnalysisPage>(
      `/jobs/${job.id}/moves?limit=50&${filter}${after ? `&after=${after}` : ''}`,
      { signal: abort.signal },
    )
      .then((result) => {
        if (abort.signal.aborted) return;
        setResults((previous) => ({
          key: resultKey,
          rows:
            after && previous?.key === resultKey
              ? [...previous.rows, ...result.items]
              : result.items,
          cursor: result.next_cursor ?? null,
        }));
        setPageError(undefined);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setPageError({ key: resultKey, error });
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoaded(requestKey);
      });
    return () => abort.abort();
  }, [job?.id, filter, after, resultKey, requestKey]);

  const progress = job && (
    <div className="match-analysis-progress" role="status" aria-live="polite">
      <div>
        <strong>
          {job.status === 'completed'
            ? t(scoped ? 'gameAnalysis.completed' : 'matchAnalysis.completed')
            : job.status === 'failed'
              ? t(scoped ? 'gameAnalysis.failed' : 'matchAnalysis.failed')
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
      {active && <AnalysisRemainingTime key={job.id} job={job} />}
      {job.error && (
        <p role="alert">{t('error.engine', { details: job.error })}</p>
      )}
    </div>
  );

  const heading = (
    <div className="match-analysis-heading">
      <div>
        <span className="eyebrow">{t('matchAnalysis.eyebrow')}</span>
        <h3>
          {scoped
            ? t('gameAnalysis.title', { game: gameNo ?? '…' })
            : t('matchAnalysis.title')}
        </h3>
        <p>
          {t(scoped ? 'gameAnalysis.description' : 'matchAnalysis.description')}
        </p>
      </div>
      <div className="match-analysis-controls">
        <label>
          {t('gameAnalysis.scope')}{' '}
          <select
            aria-label={t('gameAnalysis.scope')}
            value={scope}
            onChange={(event) =>
              setScope(event.target.value as 'game' | 'match')
            }
          >
            <option value="game">{t('gameAnalysis.scopeGame')}</option>
            <option value="match">{t('gameAnalysis.scopeMatch')}</option>
          </select>
        </label>
        <label>
          {t('matchAnalysis.depth')}{' '}
          <select
            aria-label={t('matchAnalysis.depthLabel')}
            value={plies}
            disabled={
              !configured ||
              !!active ||
              submitting ||
              (scoped && gameNo === undefined)
            }
            onChange={(event) => setPlies(Number(event.target.value))}
          >
            <option value={0}>{t('matchAnalysis.fast')}</option>
            <option value={1}>1-Ply</option>
            <option value={2}>{t('matchAnalysis.thorough')}</option>
            <option value={3}>3-ply</option>
            <option value={4}>4-ply</option>
          </select>
        </label>
        <ActionButton
          icon="analysis"
          label={
            submitting
              ? t('matchAnalysis.submitting')
              : active
                ? t('matchAnalysis.running')
                : job
                  ? t(scoped ? 'gameAnalysis.again' : 'matchAnalysis.again')
                  : t(scoped ? 'gameAnalysis.start' : 'matchAnalysis.start')
          }
          className="primary"
          onClick={() => void run(plies)}
          disabled={
            !configured ||
            !!active ||
            submitting ||
            (scoped && gameNo === undefined)
          }
        />
        {!active && !submitting && (
          <AnalysisTimeEstimate
            key={match.position_count}
            matchId={match.id}
            gameNo={gameNo}
            scoped={scoped}
            plies={plies}
            configured={configured}
            installationKey={analysisInstallationKey}
          />
        )}
      </div>
    </div>
  );
  return (
    <>
      <div
        id={controlsTarget ? 'inspector-panel-errors' : undefined}
        className={overview ? 'overview-errors' : undefined}
        role={controlsTarget ? (overview ? 'region' : 'tabpanel') : undefined}
        aria-labelledby={controlsTarget ? 'inspector-tab-errors' : undefined}
        hidden={!showResults}
      >
        <section
          className="match-analysis"
          data-job-id={job?.id}
          aria-label={t(scoped ? 'gameAnalysis.label' : 'matchAnalysis.label')}
        >
          {overview && !job && (
            <h2 className="desktop-pane-title">
              {errorsOnly
                ? t('matchAnalysis.overview')
                : text('decisionsByMove')}
            </h2>
          )}
          {!controlsTarget && heading}
          {overview && !controlsTarget && progress}
          {!!error && !controlsTarget && (
            <p className="analysis-error" role="alert">
              {formatError(error)}
            </p>
          )}
          {!job && <p className="analysis-empty">{text('noAnalysis')}</p>}
          {job && (
            <>
              {showResults && !overview && progress}
              {summary && !overview && (
                <>
                  <PlayerStatisticsSummary
                    summary={summary}
                    errorBreakdown={statistics?.error_breakdown}
                    names={names}
                    onSelect={selectMetric}
                    gameNo={job.game_no ?? undefined}
                  />
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
                <summary>
                  {errorsOnly
                    ? t('matchAnalysis.overview')
                    : text('decisionsByMove')}
                </summary>
                <div className="match-analysis-filters">
                  <label>
                    <input
                      type="checkbox"
                      checked={errorsOnly}
                      onChange={(event) =>
                        updateFilter({
                          errors_only: event.target.checked,
                          ...(event.target.checked && severity === 'none'
                            ? { severity: '' }
                            : {}),
                        })
                      }
                    />{' '}
                    {t('matchAnalysis.errorsOnly', {
                      threshold: formatNumber(0.04),
                    })}
                  </label>
                  <select
                    aria-label={t('matchAnalysis.filterPlayer')}
                    value={player}
                    onChange={(event) =>
                      updateFilter({ player: event.target.value })
                    }
                  >
                    <option value="">{t('matchAnalysis.bothPlayers')}</option>
                    <option value="0">{match.white}</option>
                    <option value="1">{match.black}</option>
                  </select>
                  <select
                    aria-label={t('matchAnalysis.filterDecision')}
                    value={kind}
                    onChange={(event) =>
                      updateFilter({ kind: event.target.value })
                    }
                  >
                    <option value="">{t('matchAnalysis.bothKinds')}</option>
                    <option value="checker">{t('common.checker')}</option>
                    <option value="cube">{t('common.cube')}</option>
                  </select>
                  <select
                    aria-label={text('filterSeverity')}
                    value={severity ?? ''}
                    onChange={(event) =>
                      updateFilter({
                        severity: event.target.value,
                        ...(event.target.value === 'none'
                          ? { errors_only: false }
                          : {}),
                      })
                    }
                  >
                    <option value="">{text('allSeverities')}</option>
                    {Object.entries(severityKeys).map(([value, key]) => (
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
                      updateFilter({ game_no: game });
                      if (game !== undefined) onGame?.(game);
                    }}
                  >
                    {!scoped && <option value="">{text('allGames')}</option>}
                    {(
                      statistics?.games ??
                      (scoped && gameNo !== undefined
                        ? [{ game_no: gameNo }]
                        : [])
                    ).map((game) => (
                      <option key={game.game_no} value={game.game_no}>
                        {text('game', { game: game.game_no })}
                      </option>
                    ))}
                  </select>
                  <ActionButton
                    icon="clear"
                    label={text('resetFilters')}
                    compact
                    type="button"
                    className="statistics-reset"
                    onClick={() =>
                      setSelection({
                        scope: filterScope,
                        filter: defaultFilter,
                      })
                    }
                  />
                </div>
                {pageError?.key === resultKey && (
                  <p role="alert">{formatError(pageError.error)}</p>
                )}
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
                        <tr
                          key={row.id}
                          aria-current={
                            row.position_uuid &&
                            row.position_uuid === selectedPositionId
                              ? 'true'
                              : undefined
                          }
                        >
                          <td>
                            {row.position_uuid ? (
                              <ActionButton
                                icon="open"
                                label={t('matchAnalysis.openPosition', {
                                  game: row.game_no,
                                  move: row.checker_no ?? '—',
                                })}
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
                                  : formatNumber(row.checker_no)}
                              </ActionButton>
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
                            <span
                              className={`severity severity-${row.severity}`}
                            >
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
                  <ActionButton
                    icon="more"
                    label={t('matchAnalysis.more')}
                    onClick={() => setPage({ key: resultKey, cursor })}
                    disabled={loading}
                  />
                )}
              </details>
            </>
          )}
        </section>
      </div>
      <MatchStatisticsView
        overview={overview}
        job={job}
        statistics={statistics}
        names={names}
        onSelect={selectMetric}
        onGame={onGame}
        hidden={!showStatistics}
        loading={!!job && statisticsLoaded !== statisticsKey}
        error={
          statisticsError?.key === statisticsKey
            ? statisticsError.error
            : undefined
        }
        progress={showStatistics && !overview ? progress : undefined}
      />
      {controlsTarget &&
        createPortal(
          <>
            {heading}
            {overview && progress}
            {!!error && (
              <p className="analysis-error" role="alert">
                {formatError(error)}
              </p>
            )}
          </>,
          controlsTarget,
        )}
    </>
  );
}
