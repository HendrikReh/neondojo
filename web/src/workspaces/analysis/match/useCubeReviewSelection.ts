import { useEffect, useRef, useState } from 'react';
import {
  reviewEmptyReason,
  type ReviewEmptyReason,
} from '../review-decisions-api';
import type { DecisionKind } from './match-statistics-api';
import type { BoardReview } from './useMatchReview';

/**
 * Opening Cube without a reviewed decision shows the first matching one from
 * the board position on, as Next error would. An existing selection is kept,
 * and this happens once per visit, so returning to the checker move stays put.
 */
export function useCubeAutoSelection({
  active,
  visit,
  cubeId,
  ready,
  boardReview,
  navigateNext,
}: {
  active: boolean;
  visit: number;
  cubeId?: string;
  ready: boolean;
  boardReview?: BoardReview;
  navigateNext: () => Promise<boolean>;
}) {
  const awaitingCubeSelection = useRef(false);
  useEffect(() => {
    awaitingCubeSelection.current = active;
  }, [active, visit]);

  useEffect(() => {
    if (!awaitingCubeSelection.current || !active || !boardReview) return;
    if (cubeId) {
      awaitingCubeSelection.current = false;
      return;
    }
    if (!ready) return;
    awaitingCubeSelection.current = false;
    const { intent, currentIntent } = boardReview;
    void navigateNext().then((finished) => {
      // A revision that arrives during the search, such as the first loaded
      // configuration, ends it. Unless the board moved, land on the decisions
      // of that revision instead.
      if (!finished && currentIntent() === intent)
        awaitingCubeSelection.current = true;
    });
    // The loaded page and the selection decide; the callbacks are current.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, cubeId, ready, boardReview]);
}

export function useEmptyReviewReason({
  matchId,
  analysisId,
  emptyKey,
  scopeGame,
  errorsOnly,
  reviewKind,
  narrowed,
  thresholdKey,
}: {
  matchId?: string;
  analysisId?: string;
  emptyKey?: string;
  scopeGame?: number;
  errorsOnly: boolean;
  reviewKind: DecisionKind;
  narrowed: boolean;
  thresholdKey: string;
}) {
  const [emptyReview, setEmptyReview] = useState<{
    key: string;
    reason?: ReviewEmptyReason;
    error?: unknown;
  }>();
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
    emptyReason: emptyState?.reason,
    emptyError: emptyState?.error,
    emptyPending: !!emptyKey && !emptyState,
  };
}
