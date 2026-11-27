import configurationOptions from '../../../../../shared/configuration-options.json' with { type: 'json' };
import { useCallback, useEffect, useRef, useState } from 'react';
import { useEpoch, useLatest } from '../../../shared/hooks/useLatest';
import { useSearchPages } from '../../../shared/hooks/useSearchPages';
import type { Match } from '../../../shared/http/api';
import { useConfirmedConfiguration } from '../../../settings/configuration/ConfigurationProvider';
import {
  canOpenReviewDecision,
  findReviewNeighbor,
  reviewDecisions,
  type ReviewDecision,
  type ReviewListEntry,
  type ReviewNavigationOrigin,
} from '../review-decisions-api';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import {
  decisionFilterQuery,
  type DecisionKind,
} from './match-statistics-api';
import { useMatchStatistics } from './useMatchStatistics';
import { useMatchReviewFilters } from './useMatchReviewFilters';
import {
  useCubeAutoSelection,
  useEmptyReviewReason,
} from './useCubeReviewSelection';

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
  const filterScope = `${visit}:${analysisId ?? ''}:${scoped ? (gameNo ?? 'none') : 'match'}`;

  const filters = useMatchReviewFilters({
    visit,
    filterScope,
    reviewKind,
    scoped,
    gameNo,
    onShowReview,
    onGame,
  });
  const { selected } = filters;
  const {
    player,
    severity,
    game_no: filteredGame,
    errors_only: errorsOnly,
  } = selected;

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
  const stats = useMatchStatistics(matchId, analysisId, statisticsKey);

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  useCubeAutoSelection({
    active: showResults && reviewKind === 'cube' && !!boardReview,
    visit,
    cubeId,
    ready:
      showDecisions &&
      !loading &&
      !pageError &&
      !boardReview?.busy &&
      !currentJump?.busy &&
      Boolean(decisions.result) &&
      decisions.total > 0,
    boardReview,
    navigateNext: () => navigateReview('next'),
  });

  const narrowed = !!(player || severity || filteredGame !== scopeGame);
  const emptyPage =
    showDecisions &&
    !directCheckerReview &&
    !loading &&
    !pageError &&
    decisions.total === 0
      ? decisions.result
      : undefined;
  const empty = useEmptyReviewReason({
    matchId,
    analysisId,
    emptyKey: emptyPage?.key,
    scopeGame,
    errorsOnly,
    reviewKind,
    narrowed,
    thresholdKey,
  });

  return {
    scoped,
    analysisId,
    job,
    thresholds,
    onGame,
    ...filters,
    ...stats,
    loading,
    pageError,
    retry: decisions.retry,
    ...empty,
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
