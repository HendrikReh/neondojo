import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import AnalysisRemainingTime from './AnalysisRemainingTime';
import AnalysisTimeEstimate from './AnalysisTimeEstimate';
import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import { request, terminal, type Job, type Match, type MatchGame } from './api';
import {
  canOpenReviewDecision,
  findReviewNeighbor,
  reviewDecisions,
  reviewEmptyReason,
  type ReviewDecision,
  type ReviewEmptyReason,
  type ReviewListEntry,
  type ReviewNavigationOrigin,
} from './review-decisions-api';
import { BoardErrorNavigation } from './BoardDecisionReview';
import SearchPagination from './SearchPagination';
import { useSearchPages } from './useSearchPages';
import { useSourceText } from './analysis-source-messages';
import { useMatchAnalysis, type MatchAnalysisEngine } from './useMatchAnalysis';
import { useI18n } from './i18n';
import { loadEngines, type EngineInfo } from './settings-api';
import { useSettingsText } from './settings-messages';
import MatchStatisticsView from './MatchStatisticsView';
import MatchAnalysisComparison from './MatchAnalysisComparison';
import {
  buildStatisticsFilter,
  decisionFilterQuery,
  statisticsRevision,
  type JobAnalysisStatistics,
  type MatchDecisionFilter,
  type StatisticsAction,
} from './match-statistics-api';
import {
  severityLabels,
  useStatisticsText,
} from './review-statistics-messages';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import EngineLabel from './EngineLabel';

const defaultFilter: MatchDecisionFilter = {
  player: '',
  kind: '',
  severity: '',
  errors_only: true,
};
const reviewPageSize = 50;

export type BoardReviewTarget = {
  element: HTMLElement;
  origin: ReviewNavigationOrigin;
  intent: number;
  currentIntent: () => number;
  busy: boolean;
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
  defaultEngine = 'gnubg',
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  configured = true,
  analysisInstallationKey,
  onJob,
  onAccepted,
  boardReview,
  overview = false,
  selectedPositionId,
  selectedDecisionId,
  analysisRevision,
  games,
  onDecision,
}: {
  match: Match;
  games?: MatchGame[];
  onDecision?: (decision: ReviewDecision) => void;
  overview?: boolean;
  selectedPositionId?: string;
  selectedDecisionId?: string;
  gameNo?: number;
  onPosition: (id: string) => void;
  controlsTarget?: HTMLElement | null;
  showResults?: boolean;
  showStatistics?: boolean;
  onShowErrors?: () => void;
  onGame?: (gameNo: number) => void;
  defaultEngine?: MatchAnalysisEngine;
  defaultPlies?: number;
  configured?: boolean;
  analysisInstallationKey?: string;
  onJob?: (job: Job | undefined) => void;
  onAccepted?: (job: Job) => void;
  boardReview?: BoardReviewTarget;
  analysisRevision?: string;
}) {
  const profile = useOptionalConfiguration();
  const [engineId, setEngineId] = useState(defaultEngine);
  const [requestedPlies, setPlies] = useState(defaultPlies);
  const [showSettings, setShowSettings] = useState(false);
  const settingsId = useId();
  const settingsButton = useRef<HTMLButtonElement>(null);
  const scopeSelect = useRef<HTMLSelectElement>(null);
  const errorsToggle = useRef<HTMLInputElement>(null);
  const resetButton = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    setEngineId(defaultEngine);
    setPlies(defaultPlies);
  }, [defaultEngine, defaultPlies, profile?.confirmed?.active_profile.id]);
  const [engineCatalog, setEngineCatalog] = useState<{
    key?: string;
    engines: EngineInfo[];
    error?: unknown;
  }>();
  useEffect(() => {
    if (!configured) return;
    const abort = new AbortController();
    void loadEngines(abort.signal)
      .then((engines) => {
        if (!abort.signal.aborted)
          setEngineCatalog({ key: analysisInstallationKey, engines });
      })
      .catch((error) => {
        if (!abort.signal.aborted)
          setEngineCatalog({
            key: analysisInstallationKey,
            engines: [],
            error,
          });
      });
    return () => abort.abort();
  }, [configured, analysisInstallationKey]);
  const catalog =
    engineCatalog?.key === analysisInstallationKey ? engineCatalog : undefined;
  const engine = catalog?.engines.find((item) => item.id === engineId);
  const supportedPlies = configurationOptions.engine_profiles[
    engineId
  ].analysis.filter((depth) => !engine || engine.plies.includes(depth));
  const plies = supportedPlies.includes(requestedPlies)
    ? requestedPlies
    : supportedPlies.includes(DEFAULT_ANALYSIS_PLIES)
      ? DEFAULT_ANALYSIS_PLIES
      : (supportedPlies[0] ?? requestedPlies);
  const {
    t,
    formatNumber,
    formatPercent,
    formatEquity,
    formatDecision,
    formatError,
  } = useI18n();
  const text = useStatisticsText();
  const settingsText = useSettingsText();
  const sourceText = useSourceText();
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
    engineId,
    scoped ? gameNo : undefined,
    !scoped || gameNo !== undefined,
    onAccepted,
  );
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
  const active = job && !terminal(job);
  const controlsDisabled = !configured || !!active || submitting;
  const analysisConfigured =
    configured && !!engine?.available && supportedPlies.includes(plies);
  const analysisDisabled =
    controlsDisabled || !analysisConfigured || (scoped && gameNo === undefined);
  const names = [match.white, match.black];
  const filter = decisionFilterQuery(selected);
  const scopeGame = scoped ? gameNo : undefined;
  const showDecisions =
    (showResults || !!boardReview) && (!scoped || scopeGame !== undefined);
  const reviewScope = `${match.id}:${job?.id ?? ''}:${filter}`;
  const reviewRevision = JSON.stringify([statisticsKey, analysisRevision]);
  const loadDecisionPage = useCallback(
    (signal: AbortSignal, page: number) =>
      reviewDecisions(match.id, selected, { jobId: job?.id, page, signal }),
    [match.id, job?.id, filter],
  );
  const decisions = useSearchPages({
    scope: reviewScope,
    enabled: showDecisions,
    revision: reviewRevision,
    pageSize: reviewPageSize,
    loadPage: loadDecisionPage,
  });
  const { items: rows, loading, error: pageError } = decisions;
  const reviewIdentity = (intent?: number, cubeId?: string) =>
    JSON.stringify([reviewScope, reviewRevision, intent, cubeId]);
  const cubeId =
    boardReview?.origin.row?.kind === 'cube' ? selectedDecisionId : undefined;
  const selectionKey = reviewIdentity(boardReview?.intent, cubeId);
  const latestSelection = useRef(selectionKey);
  latestSelection.current = selectionKey;
  const [reviewJump, setReviewJump] = useState<{
    key: string;
    entry?: ReviewListEntry;
    busy?: boolean;
    error?: unknown;
  }>();
  const currentJump = reviewJump?.key === selectionKey ? reviewJump : undefined;
  const pendingJump = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => pendingJump.current?.abort(), [selectionKey]);

  const pageOffset = (decisions.page - 1) * reviewPageSize;
  const isSelected = (row: ReviewDecision) =>
    selectedDecisionId
      ? row.id === selectedDecisionId
      : row.kind === 'checker' && row.position_uuid === selectedPositionId;
  const selectedIndex = rows.findIndex(isSelected);
  const lastEntry = currentJump?.entry;
  const ordinal =
    selectedIndex >= 0
      ? pageOffset + selectedIndex + 1
      : lastEntry && isSelected(lastEntry.row)
        ? lastEntry.ordinal
        : undefined;

  function openListedDecision(entry: ReviewListEntry) {
    if (!onDecision) return;
    pendingJump.current?.abort();
    onDecision(entry.row);
    if (!boardReview) return;
    const nextIntent = boardReview.currentIntent();
    setReviewJump({
      key: reviewIdentity(
        nextIntent,
        entry.row.kind === 'cube' ? entry.row.id : undefined,
      ),
      entry,
    });
    const page = Math.ceil(entry.ordinal / reviewPageSize);
    if (page !== decisions.page) decisions.setPage(page);
  }

  async function navigateReview(direction: 'previous' | 'next') {
    if (
      !boardReview ||
      loading ||
      boardReview.busy ||
      currentJump?.busy ||
      pageError ||
      !decisions.total
    )
      return;
    if (selectedIndex >= 0) {
      const step = direction === 'next' ? 1 : -1;
      for (let index = selectedIndex + step; rows[index]; index += step) {
        if (!canOpenReviewDecision(rows[index])) continue;
        openListedDecision({
          row: rows[index],
          ordinal: pageOffset + index + 1,
        });
        return;
      }
    }
    pendingJump.current?.abort();
    const abort = new AbortController();
    pendingJump.current = abort;
    setReviewJump({ key: selectionKey, entry: currentJump?.entry, busy: true });
    const current = () =>
      !abort.signal.aborted &&
      latestSelection.current === selectionKey &&
      boardReview.currentIntent() === boardReview.intent;
    try {
      const entry = await findReviewNeighbor({
        matchId: match.id,
        filter: selected,
        jobId: job?.id,
        origin: boardReview.origin,
        direction,
        signal: abort.signal,
      });
      if (!current()) return;
      if (entry && entry.row.id !== boardReview.origin.row?.id)
        openListedDecision(entry);
      else setReviewJump({ key: selectionKey, entry: currentJump?.entry });
    } catch (error) {
      if (current())
        setReviewJump({ key: selectionKey, entry: currentJump?.entry, error });
    }
  }
  const narrowed = !!(player || kind || severity || filteredGame !== scopeGame);
  const emptyPage =
    showDecisions && !loading && !pageError && decisions.total === 0
      ? decisions.result
      : undefined;
  const [emptyReview, setEmptyReview] = useState<{
    key: string;
    reason?: ReviewEmptyReason;
    error?: unknown;
  }>();
  const emptyKey = emptyPage?.key;
  const emptyState = emptyReview?.key === emptyKey ? emptyReview : undefined;
  useEffect(() => {
    if (!emptyKey) return;
    const abort = new AbortController();
    void reviewEmptyReason(
      match.id,
      { game_no: scopeGame, errors_only: errorsOnly },
      { jobId: job?.id, narrowed, signal: abort.signal },
    ).then(
      (reason) => {
        if (!abort.signal.aborted) setEmptyReview({ key: emptyKey, reason });
      },
      (error) => {
        if (!abort.signal.aborted) setEmptyReview({ key: emptyKey, error });
      },
    );
    return () => abort.abort();
  }, [emptyKey, match.id, job?.id, scopeGame, errorsOnly, narrowed]);

  const calculationLabel = `${t('analysis.configureMatch')} …`;
  const emptyActions = {
    missing: {
      icon: 'analysis',
      message: text('missing'),
      label: calculationLabel,
      run: () => {
        setShowSettings(true);
        requestAnimationFrame(() => scopeSelect.current?.focus());
      },
    },
    filtered: {
      icon: 'clear',
      message: t('matchAnalysis.noDecisions'),
      label: text('removeSelectionFilters'),
      run: () => {
        setSelection({
          scope: filterScope,
          filter: { ...defaultFilter, errors_only: errorsOnly },
        });
        if (showResults) resetButton.current?.focus();
        else {
          onShowErrors?.();
          requestAnimationFrame(() => resetButton.current?.focus());
        }
      },
    },
    clean: {
      icon: 'view',
      message: text('clean'),
      label: text('showAllDecisions'),
      run: () => {
        setSelection({
          scope: filterScope,
          filter: { ...defaultFilter, errors_only: false },
        });
        if (showResults) errorsToggle.current?.focus();
        else {
          onShowErrors?.();
          requestAnimationFrame(() => errorsToggle.current?.focus());
        }
      },
    },
  } as const;
  const emptyAction = emptyState?.reason && emptyActions[emptyState.reason];
  const reviewError = pageError || emptyState?.error;
  const emptyFeedback = emptyAction ? (
    <div className="analysis-empty review-empty">
      <p role="status">{emptyAction.message}</p>
      <ActionButton
        type="button"
        icon={emptyAction.icon}
        label={emptyAction.label}
        onClick={emptyAction.run}
      />
    </div>
  ) : reviewError ? (
    <div className="analysis-empty review-empty">
      <p className="analysis-error" role="alert">{formatError(reviewError)}</p>
      <ActionButton
        type="button"
        icon="refresh"
        label={sourceText('retry')}
        onClick={decisions.retry}
      />
    </div>
  ) : null;

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
                : t('analysis.movesRunning')}
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
      {controlsTarget && (
        <ActionButton
          ref={settingsButton}
          type="button"
          icon="analysis"
          label={calculationLabel}
          aria-expanded={showSettings}
          aria-controls={settingsId}
          onClick={() => setShowSettings((open) => !open)}
        />
      )}
      <div
        id={settingsId}
        className="match-analysis-settings"
        hidden={!!controlsTarget && !showSettings}
      >
        <div className="match-analysis-controls">
          <label>
            {t('gameAnalysis.scope')}{' '}
            <select
              ref={scopeSelect}
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
            {settingsText('analysisEngine')}
            <select
              aria-label={settingsText('analysisEngine')}
              value={engineId}
              disabled={!configured || submitting}
              onChange={(event) => {
                const value = event.target.value;
                if (value !== 'gnubg' && value !== 'bgsage') return;
                setEngineId(value);
                setPlies(plies);
              }}
            >
              {(['gnubg', 'bgsage'] as const).map((id) => {
                const item = catalog?.engines.find((entry) => entry.id === id);
                const name = id === 'gnubg' ? 'GNUbg' : 'Open Sage';
                return (
                  <option key={id} value={id}>
                    {item?.name ?? name}
                    {catalog && !item?.available
                      ? ` — ${settingsText('unavailable')}`
                      : ''}
                  </option>
                );
              })}
            </select>
          </label>
          <label>
            {t('matchAnalysis.depth')}{' '}
            <select
              aria-label={t('matchAnalysis.depthLabel')}
              value={plies}
              disabled={analysisDisabled}
              onChange={(event) => setPlies(Number(event.target.value))}
            >
              <option value={0} disabled={!supportedPlies.includes(0)}>
                {t('matchAnalysis.fast')}
              </option>
              <option value={1} disabled={!supportedPlies.includes(1)}>
                1-Ply
              </option>
              <option value={2} disabled={!supportedPlies.includes(2)}>
                {t('matchAnalysis.thorough')}
              </option>
              <option value={3} disabled={!supportedPlies.includes(3)}>
                3-ply
              </option>
              <option value={4} disabled={!supportedPlies.includes(4)}>
                4-ply
              </option>
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
            onClick={() => {
              if (controlsTarget) {
                setShowSettings(false);
                settingsButton.current?.focus();
              }
              void run(plies);
            }}
            disabled={analysisDisabled}
          />
          {(!controlsTarget || showSettings) && !active && !submitting && (
            <AnalysisTimeEstimate
              key={match.position_count}
              matchId={match.id}
              gameNo={gameNo}
              scoped={scoped}
              engineId={engineId}
              plies={plies}
              configured={analysisConfigured}
              installationKey={analysisInstallationKey}
            />
          )}
        </div>
        {!!catalog?.error && (
          <p className="analysis-error" role="alert">
            {formatError(catalog.error)}
          </p>
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
          {!controlsTarget && heading}
          {overview && !controlsTarget && progress}
          {!!error && !controlsTarget && (
            <p className="analysis-error" role="alert">
              {formatError(error)}
            </p>
          )}
          <>
            <p className="review-source-note">{text('reviewSources')}</p>
            {showResults && !overview && !controlsTarget && progress}
            <details className="match-move-overview" open>
              <summary>
                {errorsOnly
                  ? t('matchAnalysis.overview')
                  : text('decisionsByMove')}
              </summary>
              <div className="match-analysis-filters">
                <label>
                  <input
                    ref={errorsToggle}
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
                    updateFilter({ game_no: game });
                    if (game !== undefined) onGame?.(game);
                  }}
                >
                  {!scoped && <option value="">{text('allGames')}</option>}
                  {(
                    (scoped ? undefined : games) ??
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
                  ref={resetButton}
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
              <div className="match-moves-scroll" hidden={rows.length === 0}>
                <table className="match-moves-table">
                  <thead>
                    <tr>
                      <th>{t('matchAnalysis.gameMove')}</th>
                      <th>{t('matchAnalysis.playerKind')}</th>
                      <th>{t('common.played')}</th>
                      <th>{t('matchAnalysis.bestDecision')}</th>
                      <th>{t('matchAnalysis.loss')}</th>
                      <th>{t('matchAnalysis.rating')}</th>
                      <th>{sourceText('sources')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((row, index) => (
                      <tr
                        key={row.id}
                        aria-current={isSelected(row) ? 'true' : undefined}
                      >
                        <td>
                          {(
                            onDecision
                              ? canOpenReviewDecision(row)
                              : !!row.position_uuid
                          ) ? (
                            <ActionButton
                              icon="open"
                              label={t('matchAnalysis.openPosition', {
                                game: row.game_no,
                                move: row.checker_no ?? row.decision_no,
                              })}
                              className="position-link"
                              aria-label={t('matchAnalysis.openPosition', {
                                game: row.game_no,
                                move: row.checker_no ?? row.decision_no,
                              })}
                              onClick={() =>
                                onDecision
                                  ? openListedDecision({
                                      row,
                                      ordinal: pageOffset + index + 1,
                                    })
                                  : row.position_uuid &&
                                    onPosition(row.position_uuid)
                              }
                            >
                              {formatNumber(row.game_no)} ·{' '}
                              {row.checker_no == null
                                ? formatNumber(row.decision_no)
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
                          <span className={`severity severity-${row.severity}`}>
                            {row.evaluation_status === 'forced'
                              ? t('matchAnalysis.forced')
                              : row.evaluation_status === 'unavailable'
                                ? t('matchAnalysis.unavailable')
                                : severityLabels[row.severity]
                                  ? t(severityLabels[row.severity])
                                  : row.severity}
                          </span>
                        </td>
                        <td className="review-provenance">
                          <span>
                            {sourceText(
                              row.source === 'imported' ? 'imported' : 'engine',
                            )}
                          </span>
                          <small>
                            <EngineLabel
                              details={`${row.engine} · ${row.depth ?? sourceText('unknownDepth')}`}
                            />
                          </small>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {emptyFeedback}
              {(loading || (emptyPage && !emptyState)) && (
                <p role="status">{t('matchAnalysis.loading')}</p>
              )}
              <SearchPagination
                page={decisions.page}
                lastPage={decisions.lastPage}
                busy={loading}
                onPageChange={decisions.setPage}
              />
            </details>
          </>
        </section>
      </div>
      <MatchStatisticsView
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
        progress={showStatistics && !controlsTarget ? progress : undefined}
        comparison={
          match.source_available &&
          (!scoped || gameNo !== undefined) && (
            <MatchAnalysisComparison
              key={match.id}
              matchId={match.id}
              gameNo={scoped ? gameNo : undefined}
              names={names}
              visible={showStatistics}
            />
          )
        }
      />
      {boardReview &&
        createPortal(
          <BoardErrorNavigation
            game={filteredGame}
            player={player ? names[Number(player)] : undefined}
            kind={kind}
            severity={severity}
            errorsOnly={errorsOnly}
            total={pageError || !showDecisions ? 0 : decisions.total}
            ordinal={loading ? undefined : ordinal}
            busy={loading || boardReview.busy || !!currentJump?.busy}
            onNavigate={(direction) => void navigateReview(direction)}
          >
            {emptyFeedback}
            {!!currentJump?.error && (
              <small role="alert">{formatError(currentJump.error)}</small>
            )}
          </BoardErrorNavigation>,
          boardReview.element,
        )}
      {controlsTarget &&
        createPortal(
          <>
            {heading}
            {progress}
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
