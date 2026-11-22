import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import AnalysisRemainingTime from './AnalysisRemainingTime';
import AnalysisTimeEstimate from './AnalysisTimeEstimate';
import { sessionFields, useWorkspaceSession } from './useWorkspaceSession';
import { useConfirmedConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  cancelJob,
  isCancelledJob,
  request,
  terminal,
  type Job,
  type Match,
  type MatchGame,
} from './api';
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
import { useSearchPages } from './useSearchPages';
import { useSourceText } from './analysis-source-messages';
import { useMatchAnalysis, type MatchAnalysisEngine } from './useMatchAnalysis';
import { useI18n } from './i18n';
import {
  MatchAnalysisFields,
  useMatchAnalysisOptions,
} from './MatchAnalysisFields';
import MatchStatisticsView from './MatchStatisticsView';
import MatchAnalysisComparison from './MatchAnalysisComparison';
import {
  buildStatisticsFilter,
  decisionFilterQuery,
  type DecisionKind,
  type MatchDecisionFilter,
  type StatisticsAction,
} from './match-statistics-api';
import {
  severityLabels,
  useStatisticsText,
} from './review-statistics-messages';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import type { AnalysisCatalog } from './useAnalysisCatalog';
import { useAnalysisCatalogText } from './analysis-catalog-messages';
import {
  analysisStatisticsPath,
  type MatchAnalysisStatistics,
} from './analysis-catalog-api';

const defaultFilter = {
  player: '',
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
  controlsTarget,
  positionControlsRef,
  showResults = true,
  showStatistics = false,
  reviewKind = 'cube',
  checkerControls,
  onShowReview,
  onGame,
  defaultEngine = 'gnubg',
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  configured = true,
  analysisInstallationKey,
  onJob,
  onAccepted,
  boardReview,
  selectedPositionId,
  selectedDecisionId,
  analysisRevision,
  catalog,
  games,
  onDecision,
}: {
  match: Match;
  games?: MatchGame[];
  onDecision?: (decision: ReviewDecision) => void;
  selectedPositionId?: string;
  selectedDecisionId?: string;
  gameNo?: number;
  controlsTarget?: HTMLElement | null;
  positionControlsRef?: (element: HTMLDivElement | null) => void;
  showResults?: boolean;
  showStatistics?: boolean;
  reviewKind?: DecisionKind;
  checkerControls?: HTMLElement | null;
  onShowReview?: (kind: DecisionKind) => void;
  onGame?: (gameNo: number) => void;
  defaultEngine?: MatchAnalysisEngine;
  defaultPlies?: number;
  configured?: boolean;
  analysisInstallationKey?: string;
  onJob?: (job: Job | undefined) => void;
  onAccepted?: (job: Job) => void;
  boardReview?: BoardReviewTarget;
  analysisRevision?: string;
  catalog: AnalysisCatalog;
}) {
  const confirmed = useConfirmedConfiguration();
  const options = useMatchAnalysisOptions({
    defaultEngine,
    defaultPlies,
    configured,
    analysisInstallationKey,
  });
  const {
    engineId,
    plies,
    catalog: engineCatalog,
    analysisConfigured,
  } = options;
  const [showSettings, setShowSettings] = useState(false);
  const settingsId = useId();
  const settingsButton = useRef<HTMLButtonElement>(null);
  const settingsPanel = useRef<HTMLDivElement>(null);
  const settingsPopover = !!controlsTarget && showSettings;
  // The header options float above the board; dismiss them like a menu.
  useEffect(() => {
    if (!settingsPopover) return;
    const inside = (target: EventTarget | null) =>
      target instanceof Node &&
      (settingsPanel.current?.contains(target) ||
        settingsButton.current?.contains(target));
    const pointer = (event: PointerEvent) => {
      if (!inside(event.target)) setShowSettings(false);
    };
    const key = (event: KeyboardEvent) => {
      // A recalculation dialog opened from the options handles its own Escape.
      if (
        event.key !== 'Escape' ||
        event.defaultPrevented ||
        settingsPanel.current?.querySelector('dialog[open]')
      )
        return;
      const refocus = inside(document.activeElement);
      setShowSettings(false);
      if (refocus) settingsButton.current?.focus();
    };
    // Once a recalculation starts, its dialog closes and the options must
    // not keep covering the analysis; a cancelled dialog returns to them.
    const panel = settingsPanel.current;
    let started = false;
    const click = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      started = !!target
        ?.closest('dialog')
        ?.contains(target.closest('button.primary'));
    };
    let dialogOpen = !!panel?.querySelector('dialog');
    const dialogs = new MutationObserver(() => {
      const open = !!panel?.querySelector('dialog');
      if (dialogOpen && !open && started) {
        setShowSettings(false);
        settingsButton.current?.focus();
      }
      if (!open) started = false;
      dialogOpen = open;
    });
    if (panel) dialogs.observe(panel, { childList: true, subtree: true });
    panel?.addEventListener('click', click, true);
    document.addEventListener('pointerdown', pointer);
    document.addEventListener('keydown', key);
    return () => {
      dialogs.disconnect();
      panel?.removeEventListener('click', click, true);
      document.removeEventListener('pointerdown', pointer);
      document.removeEventListener('keydown', key);
    };
  }, [settingsPopover]);
  const errorsToggle = useRef<HTMLInputElement>(null);
  const resetButton = useRef<HTMLButtonElement>(null);
  const { t, formatNumber, formatPercent, formatError } = useI18n();
  const text = useStatisticsText();
  const sourceText = useSourceText();
  const catalogText = useAnalysisCatalogText();
  const scoped = catalog.selected?.scope === 'game';
  const analysisId = catalog.analysisId;
  const job =
    catalog.selected?.job?.kind === 'match' ? catalog.selected.job : undefined;
  const gameAnalysis = useMatchAnalysis(
    match.id,
    engineId,
    gameNo,
    gameNo !== undefined,
    onAccepted,
    analysisRevision,
  );
  const { job: gameJob, submitting, run, error } = gameAnalysis;
  useEffect(() => onJob?.(gameJob), [gameJob, onJob]);
  const [cancelling, setCancelling] = useState<string>();
  const [cancelError, setCancelError] = useState<unknown>();
  const filterScope = `${match.id}:${analysisId ?? ''}:${scoped ? (gameNo ?? 'none') : 'match'}`;
  const [cubeFilters, setCubeFilters] = useWorkspaceSession(
    'analysis.cubeReview',
    undefined,
    defaultFilter,
    (value) => sessionFields(value, defaultFilter),
  );
  const [checkerFilters, setCheckerFilters] = useState(defaultFilter);
  const [selectedGames, setSelectedGames] = useState<
    Partial<Record<DecisionKind, { scope: string; game?: number }>>
  >({});
  const selectedGame = selectedGames[reviewKind];
  const selected: MatchDecisionFilter = {
    ...(reviewKind === 'cube' ? cubeFilters : checkerFilters),
    kind: reviewKind,
    // A single-game job always has this scope, including after reset or a
    // metric action. Keep the visible selection and request explicit.
    game_no: scoped
      ? gameNo
      : selectedGame?.scope === filterScope
        ? selectedGame.game
        : undefined,
  };
  const {
    player,
    severity,
    game_no: filteredGame,
    errors_only: errorsOnly,
  } = selected;
  function setSelection(
    kind: DecisionKind,
    filter: Omit<MatchDecisionFilter, 'kind'>,
  ) {
    setSelectedGames((games) => ({
      ...games,
      [kind]: { scope: filterScope, game: filter.game_no },
    }));
    const save = kind === 'cube' ? setCubeFilters : setCheckerFilters;
    save({
      player: filter.player,
      severity: filter.severity ?? '',
      errors_only: filter.errors_only,
    });
  }
  const updateFilter = (patch: Partial<Omit<MatchDecisionFilter, 'kind'>>) =>
    setSelection(reviewKind, { ...selected, ...patch });
  function selectMetric(action: StatisticsAction) {
    const filter = buildStatisticsFilter(action);
    const kind = filter.kind === 'cube' ? 'cube' : 'checker';
    setSelection(kind, filter);
    if (action.gameNo !== undefined && action.gameNo !== gameNo)
      onGame?.(action.gameNo);
    onShowReview?.(kind);
  }
  const thresholds =
    confirmed?.analysis.error_thresholds ??
    configurationOptions.analysis_error_thresholds;
  const thresholdKey = JSON.stringify(thresholds);
  const statisticsKey = analysisId
    ? JSON.stringify([
        analysisId,
        job?.status,
        job?.completed_games,
        analysisRevision,
        thresholdKey,
      ])
    : '';

  const [statisticsResult, setStatisticsResult] = useState<{
    key: string;
    value: MatchAnalysisStatistics;
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
    if (!analysisId) return;
    const abort = new AbortController();
    void request<MatchAnalysisStatistics>(
      analysisStatisticsPath(match.id, analysisId),
      {
        signal: abort.signal,
      },
    )
      .then((value) => {
        if (abort.signal.aborted || value.analysis_id !== analysisId) return;
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
  }, [match.id, analysisId, statisticsKey]);
  const active = job && !terminal(job);
  const gameRunning = gameJob && !terminal(gameJob);
  const showEstimate =
    !gameRunning && !submitting && (!controlsTarget || showSettings);
  const analysisDisabled =
    !analysisConfigured || !!gameRunning || submitting || gameNo === undefined;
  const names = [match.white, match.black];
  const filter = decisionFilterQuery(selected);
  const scopeGame = scoped ? gameNo : undefined;
  const showDecisions =
    !!analysisId &&
    (showResults || !!checkerControls) &&
    (!scoped || scopeGame !== undefined);
  const reviewScope = `${match.id}:${analysisId ?? ''}:${filter}`;
  const reviewRevision = JSON.stringify([
    statisticsKey,
    analysisRevision,
    thresholdKey,
  ]);
  const loadDecisionPage = useCallback(
    (signal: AbortSignal, page: number) =>
      reviewDecisions(match.id, selected, { analysisId, page, signal }),
    [match.id, analysisId, filter],
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
        analysisId,
        origin: boardReview.origin,
        direction,
        signal: abort.signal,
        currentPage:
          selectedIndex >= 0
            ? {
                page: decisions.page,
                pageSize: reviewPageSize,
                total: decisions.total,
              }
            : undefined,
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
  const narrowed = !!(player || severity || filteredGame !== scopeGame);
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
      { game_no: scopeGame, errors_only: errorsOnly, kind: reviewKind },
      { analysisId, narrowed, signal: abort.signal },
    ).then(
      (reason) => {
        if (!abort.signal.aborted) setEmptyReview({ key: emptyKey, reason });
      },
      (error) => {
        if (!abort.signal.aborted) setEmptyReview({ key: emptyKey, error });
      },
    );
    return () => abort.abort();
  }, [
    emptyKey,
    match.id,
    analysisId,
    scopeGame,
    errorsOnly,
    reviewKind,
    narrowed,
    thresholdKey,
  ]);

  const emptyActions = {
    filtered: {
      icon: 'clear',
      message: t('matchAnalysis.noDecisions'),
      label: text('removeSelectionFilters'),
      run: () => {
        setSelection(reviewKind, { ...defaultFilter, errors_only: errorsOnly });
        resetButton.current?.focus();
      },
    },
    clean: {
      icon: 'view',
      message: text('clean'),
      label: text('showAllDecisions'),
      run: () => {
        updateFilter({ errors_only: false });
        errorsToggle.current?.focus();
      },
    },
  } as const;
  const reason = emptyState?.reason;
  const emptyAction =
    reason && reason !== 'missing' ? emptyActions[reason] : undefined;
  const reviewError = pageError || emptyState?.error;
  const emptyFeedback =
    reason === 'missing' ? (
      <div className="analysis-empty review-empty">
        <p role="status">
          {text('missing')}{' '}
          {catalog.entries.some((entry) => entry.id !== analysisId) &&
            catalogText('alternatives')}
        </p>
      </div>
    ) : emptyAction ? (
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
        <p className="analysis-error" role="alert">
          {formatError(reviewError)}
        </p>
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
            : isCancelledJob(job)
              ? t(scoped ? 'gameAnalysis.cancelled' : 'matchAnalysis.cancelled')
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
      {active && (
        <ActionButton
          icon="cancel"
          label={t('analysis.cancelJob')}
          compact
          disabled={cancelling === job.id}
          onClick={() => {
            const id = job.id;
            setCancelling(id);
            setCancelError(undefined);
            // The job's event stream reports the cancelled state.
            cancelJob(id)
              .catch(setCancelError)
              .finally(() =>
                setCancelling((current) =>
                  current === id ? undefined : current,
                ),
              );
          }}
        />
      )}
      {!!cancelError && <p role="alert">{formatError(cancelError)}</p>}
      {job.error && !isCancelledJob(job) && (
        <p role="alert">{t('error.engine', { details: job.error })}</p>
      )}
    </div>
  );

  const heading = (
    <div className="match-analysis-heading">
      {!controlsTarget && (
        <div>
          <span className="eyebrow">{t('matchAnalysis.eyebrow')}</span>
          <h3>
            {scoped
              ? t('gameAnalysis.title', { game: gameNo ?? '…' })
              : t('matchAnalysis.title')}
          </h3>
          <p>
            {t(
              scoped ? 'gameAnalysis.description' : 'matchAnalysis.description',
            )}
          </p>
        </div>
      )}
      <div className="match-analysis-controls">
        <div className="match-analysis-actions">
          <ActionButton
            type="button"
            icon="analysis"
            label={
              submitting
                ? t('matchAnalysis.submitting')
                : gameRunning
                  ? t('matchAnalysis.running')
                  : gameJob
                    ? t('gameAnalysis.again')
                    : t('gameAnalysis.start')
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
          {controlsTarget && (
            <ActionButton
              ref={settingsButton}
              type="button"
              icon="settings"
              label={t('analysis.matchOptions')}
              aria-expanded={showSettings}
              aria-controls={settingsId}
              onClick={() => setShowSettings((open) => !open)}
            />
          )}
        </div>
        <div
          id={settingsId}
          ref={settingsPanel}
          className={
            controlsTarget
              ? 'match-analysis-popover'
              : 'match-analysis-inline-options'
          }
          hidden={!!controlsTarget && !showSettings}
        >
          <div className="match-analysis-options">
            <MatchAnalysisFields
              options={options}
              disabled={submitting || !configured}
            />
            {positionControlsRef && (
              <div
                className="position-analysis-actions"
                ref={positionControlsRef}
              />
            )}
          </div>
          {showEstimate && (
            <AnalysisTimeEstimate
              key={match.position_count}
              matchId={match.id}
              gameNo={gameNo}
              scoped
              engineId={engineId}
              plies={plies}
              configured={analysisConfigured}
              installationKey={analysisInstallationKey}
            />
          )}
        </div>
        {!!engineCatalog?.error && (
          <p className="analysis-error" role="alert">
            {formatError(engineCatalog.error)}
          </p>
        )}
      </div>
    </div>
  );
  const filters = (
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
          threshold: formatNumber(thresholds.questionable, {
            maximumFractionDigits: 20,
          }),
        })}
      </label>
      <select
        aria-label={t('matchAnalysis.filterPlayer')}
        value={player}
        onChange={(event) => updateFilter({ player: event.target.value })}
      >
        <option value="">{t('matchAnalysis.bothPlayers')}</option>
        <option value="0">{match.white}</option>
        <option value="1">{match.black}</option>
      </select>
      <select
        aria-label={text('filterSeverity')}
        value={severity ?? ''}
        onChange={(event) =>
          updateFilter({
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
          updateFilter({ game_no: game });
          if (game !== undefined) onGame?.(game);
        }}
      >
        {!scoped && <option value="">{text('allGames')}</option>}
        {(
          (scoped ? undefined : games) ??
          statistics?.games ??
          (scoped && gameNo !== undefined ? [{ game_no: gameNo }] : [])
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
        onClick={() => setSelection(reviewKind, defaultFilter)}
      />
    </div>
  );
  return (
    <>
      <div
        id={controlsTarget ? 'inspector-panel-errors' : undefined}
        role={controlsTarget ? 'tabpanel' : undefined}
        aria-labelledby={controlsTarget ? 'inspector-tab-errors' : undefined}
        hidden={!showResults}
      >
        <section
          className="match-analysis"
          data-job-id={job?.id}
          aria-label={t(scoped ? 'gameAnalysis.label' : 'matchAnalysis.label')}
        >
          {!controlsTarget && heading}
          {!!error && !controlsTarget && (
            <p className="analysis-error" role="alert">
              {formatError(error)}
            </p>
          )}
          {showResults && !controlsTarget && progress}
          {showResults && filters}
          {!analysisId && !catalog.loading && !catalog.saving && (
            <p className="analysis-empty" role="status">
              {catalogText(catalog.selectedId ? 'unavailable' : 'none')}
            </p>
          )}
          {emptyFeedback}
          {(loading || (emptyPage && !emptyState)) && (
            <p role="status">{t('matchAnalysis.loading')}</p>
          )}
        </section>
      </div>
      {checkerControls && createPortal(filters, checkerControls)}
      <MatchStatisticsView
        analysisEntry={catalog.selected}
        statistics={statistics}
        names={names}
        onSelect={selectMetric}
        onGame={onGame}
        hidden={!showStatistics}
        loading={
          catalog.loading ||
          catalog.saving ||
          (!!analysisId && statisticsLoaded !== statisticsKey)
        }
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
              catalog={catalog}
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
            kind={reviewKind}
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
