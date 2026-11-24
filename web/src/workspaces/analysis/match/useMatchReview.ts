import configurationOptions from '../../../../../shared/configuration-options.json' with { type: 'json' };
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  sessionFields,
  useWorkspaceSession,
} from '../../../shared/hooks/useWorkspaceSession';
import { useEpoch, useLatest } from '../../../shared/hooks/useLatest';
import { useSearchPages } from '../../../shared/hooks/useSearchPages';
import { request, type Match } from '../../../shared/http/api';
import { useConfirmedConfiguration } from '../../../settings/configuration/ConfigurationProvider';
import {
  canOpenReviewDecision,
  findReviewNeighbor,
  reviewDecisions,
  reviewEmptyReason,
  type ReviewDecision,
  type ReviewEmptyReason,
  type ReviewListEntry,
  type ReviewNavigationOrigin,
} from '../review-decisions-api';
import {
  analysisStatisticsPath,
  type MatchAnalysisStatistics,
} from '../analysis-catalog-api';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import {
  buildStatisticsFilter,
  decisionFilterQuery,
  type DecisionKind,
  type MatchDecisionFilter,
  type StatisticsAction,
} from './match-statistics-api';

const defaultFilter = {
  player: '',
  severity: '',
  errors_only: true,
};
const reviewPageSize = 50;

/** The board the review steps through; undefined while it is not steppable. */
export type BoardReview = {
  origin: ReviewNavigationOrigin;
  intent: number;
  currentIntent: () => number;
  busy: boolean;
};

export type MatchReview = ReturnType<typeof useMatchReview>;

/**
 * Filters, pages, error navigation and statistics of the displayed analysis.
 * The inspector, the board and the statistics show parts of the same review.
 * Every visit of a match starts with a fresh one; without a match it is idle.
 */
export function useMatchReview({
  match,
  catalog,
  analysisRevision,
  gameNo,
  reviewKind,
  showResults,
  checkerFiltersOpen,
  board,
  selectedPositionId,
  selectedDecisionId,
  onDecision,
  onShowReview,
  onGame,
}: {
  match?: Match;
  catalog: AnalysisCatalog;
  analysisRevision?: string;
  gameNo?: number;
  reviewKind: DecisionKind;
  showResults: boolean;
  /** Checker shows its review filters in the inspector. */
  checkerFiltersOpen: boolean;
  board?: BoardReview;
  selectedPositionId?: string;
  selectedDecisionId?: string;
  onDecision: (decision: ReviewDecision) => void;
  onShowReview: (kind: DecisionKind) => void;
  onGame: (gameNo: number) => void;
}) {
  const confirmed = useConfirmedConfiguration();
  const matchId = match?.id;
  const visit = useEpoch(matchId);
  const boardReview = match ? board : undefined;
  const scoped = catalog.selected?.scope === 'game';
  const analysisId = match ? catalog.analysisId : undefined;
  const job =
    match && catalog.selected?.job?.kind === 'match'
      ? catalog.selected.job
      : undefined;
  const errorsToggle = useRef<HTMLInputElement>(null);
  const resetButton = useRef<HTMLButtonElement>(null);
  const filterScope = `${visit}:${analysisId ?? ''}:${scoped ? (gameNo ?? 'none') : 'match'}`;
  const [cubeFilters, setCubeFilters] = useWorkspaceSession(
    'analysis.cubeReview',
    undefined,
    defaultFilter,
    (value) => sessionFields(value, defaultFilter),
  );
  const [checkerReview, setCheckerReview] = useState({
    visit,
    filter: defaultFilter,
  });
  const checkerFilters =
    checkerReview.visit === visit ? checkerReview.filter : defaultFilter;
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
    const saved = {
      player: filter.player,
      severity: filter.severity ?? '',
      errors_only: filter.errors_only,
    };
    if (kind === 'cube') setCubeFilters(saved);
    else setCheckerReview({ visit, filter: saved });
  }
  const updateFilter = (patch: Partial<Omit<MatchDecisionFilter, 'kind'>>) =>
    setSelection(reviewKind, { ...selected, ...patch });
  function selectMetric(action: StatisticsAction) {
    const filter = buildStatisticsFilter(action);
    const kind = filter.kind === 'cube' ? 'cube' : 'checker';
    setSelection(kind, filter);
    if (action.gameNo !== undefined && action.gameNo !== gameNo)
      onGame(action.gameNo);
    onShowReview(kind);
  }
  const thresholds =
    confirmed?.analysis.error_thresholds ??
    configurationOptions.analysis_error_thresholds;
  const thresholdKey = JSON.stringify(thresholds);
  const statisticsKey = analysisId
    ? JSON.stringify([
        visit,
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
    if (!matchId || !analysisId) return;
    const abort = new AbortController();
    void request<MatchAnalysisStatistics>(
      analysisStatisticsPath(matchId, analysisId),
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
  }, [matchId, analysisId, statisticsKey]);
  const filter = decisionFilterQuery(selected);
  const scopeGame = scoped ? gameNo : undefined;
  // Checker steps through its errors on the board without the review filters.
  const directCheckerReview =
    reviewKind === 'checker' && !!boardReview && !checkerFiltersOpen;
  const showDecisions =
    !!analysisId &&
    (showResults || checkerFiltersOpen || directCheckerReview) &&
    (!scoped || scopeGame !== undefined);
  const reviewScope = `${visit}:${analysisId ?? ''}:${filter}`;
  const reviewRevision = JSON.stringify([
    statisticsKey,
    analysisRevision,
    thresholdKey,
  ]);
  const loadDecisionPage = useCallback(
    (signal: AbortSignal, page: number) =>
      reviewDecisions(matchId!, selected, { analysisId, page, signal }),
    [matchId, analysisId, filter],
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
  const latestSelection = useLatest(selectionKey);
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

  /** Resolves to false when a newer selection or revision ended the search. */
  async function navigateReview(
    direction: 'previous' | 'next',
  ): Promise<boolean> {
    if (
      !matchId ||
      !boardReview ||
      loading ||
      boardReview.busy ||
      currentJump?.busy ||
      pageError ||
      !decisions.total
    )
      return true;
    if (selectedIndex >= 0) {
      const step = direction === 'next' ? 1 : -1;
      for (let index = selectedIndex + step; rows[index]; index += step) {
        if (!canOpenReviewDecision(rows[index])) continue;
        openListedDecision({
          row: rows[index],
          ordinal: pageOffset + index + 1,
        });
        return true;
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
        matchId,
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
      if (!current()) return false;
      if (entry && entry.row.id !== boardReview.origin.row?.id)
        openListedDecision(entry);
      else setReviewJump({ key: selectionKey, entry: currentJump?.entry });
    } catch (error) {
      if (!current()) return false;
      setReviewJump({ key: selectionKey, entry: currentJump?.entry, error });
    }
    return true;
  }
  // Opening Cube without a reviewed decision shows the first matching one
  // from the board position on, as Next error would. An existing selection
  // is kept, and this happens once per visit, so returning to the checker
  // move stays put.
  const cubeReview = showResults && reviewKind === 'cube' && !!boardReview;
  const awaitingCubeSelection = useRef(false);
  useEffect(() => {
    awaitingCubeSelection.current = cubeReview;
  }, [cubeReview, visit]);
  useEffect(() => {
    if (!awaitingCubeSelection.current || !cubeReview) return;
    if (cubeId) {
      awaitingCubeSelection.current = false;
      return;
    }
    // An empty page keeps waiting: the decisions of a just-finished analysis
    // arrive with a later revision, and the first page can precede it.
    if (
      !showDecisions ||
      loading ||
      pageError ||
      boardReview?.busy ||
      currentJump?.busy ||
      !decisions.result ||
      decisions.total === 0
    )
      return;
    awaitingCubeSelection.current = false;
    const { intent, currentIntent } = boardReview;
    void navigateReview('next').then((finished) => {
      // A revision that arrives during the search, such as the first loaded
      // configuration, ends it. Unless the board moved, land on the decisions
      // of that revision instead.
      if (!finished && currentIntent() === intent)
        awaitingCubeSelection.current = true;
    });
    // The loaded page and the selection decide; the callbacks are current.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    cubeReview,
    cubeId,
    showDecisions,
    loading,
    pageError,
    boardReview?.busy,
    currentJump?.busy,
    decisions.result,
    decisions.total,
  ]);
  const narrowed = !!(player || severity || filteredGame !== scopeGame);
  const emptyPage =
    showDecisions &&
    !directCheckerReview &&
    !loading &&
    !pageError &&
    decisions.total === 0
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
    if (!matchId || !emptyKey) return;
    const abort = new AbortController();
    void reviewEmptyReason(
      matchId,
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
    matchId,
    analysisId,
    scopeGame,
    errorsOnly,
    reviewKind,
    narrowed,
    thresholdKey,
  ]);

  return {
    scoped,
    analysisId,
    job,
    selected,
    thresholds,
    updateFilter,
    resetFilters: () => setSelection(reviewKind, defaultFilter),
    removeSelectionFilters: () => {
      setSelection(reviewKind, { ...defaultFilter, errors_only: errorsOnly });
      resetButton.current?.focus();
    },
    showAllDecisions: () => {
      updateFilter({ errors_only: false });
      errorsToggle.current?.focus();
    },
    errorsToggle,
    resetButton,
    selectMetric,
    onGame,
    statistics,
    statisticsLoading: !!analysisId && statisticsLoaded !== statisticsKey,
    statisticsError:
      statisticsError?.key === statisticsKey
        ? statisticsError.error
        : undefined,
    loading,
    pageError,
    retry: decisions.retry,
    emptyReason: emptyState?.reason,
    emptyError: emptyState?.error,
    emptyPending: !!emptyPage && !emptyState,
    navigation: boardReview &&
      (!directCheckerReview || (!pageError && decisions.total > 0)) && {
        kind: reviewKind,
        total: pageError || !showDecisions ? 0 : decisions.total,
        ordinal: loading ? undefined : ordinal,
        busy: loading || boardReview.busy || !!currentJump?.busy,
        error: currentJump?.error,
        // Checker without its filters keeps only the step buttons.
        showEmptyFeedback: !directCheckerReview,
        onNavigate: (direction: 'previous' | 'next') =>
          void navigateReview(direction),
      },
  };
}
