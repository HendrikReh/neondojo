import { useCallback, useEffect, useState } from 'react';
import { useEpoch, useLatest } from '../../../shared/hooks/useLatest';
import {
  gamePlanBoardKey,
  gamePlanCache,
  loadGamePlans,
  type GamePlans,
} from './game-plan-api';
import { AppError } from '../../../shared/i18n/i18n-core';

interface PlanState {
  epoch: number;
  plans?: Readonly<GamePlans>;
  loading: boolean;
  error?: unknown;
}

/** Classification follows the visible board, independent of engine jobs. */
export function useGamePlans(board?: readonly number[], active = true) {
  const key = board ? gamePlanBoardKey(board) : undefined;
  const scope = active && board ? (key ?? 'invalid') : '';
  const [attempt, setAttempt] = useState(0);
  // A -> B -> A may happen before effect cleanup. Every transition
  // invalidates preceding promises, even when the final board key is equal.
  const epoch = useEpoch(JSON.stringify([scope, attempt]));
  const latestEpoch = useLatest(epoch);
  const [state, setState] = useState<PlanState>({ epoch: -1, loading: false });
  const retry = useCallback(() => setAttempt((value) => value + 1), []);

  useEffect(() => {
    if (!scope || key === undefined || latestEpoch.current !== epoch) return;
    // Parse the immutable board identity rather than close over a changing array.
    const snapshot = JSON.parse(key) as number[];
    const cached = gamePlanCache.get(snapshot);
    if (cached) {
      setState({ epoch, plans: cached, loading: false });
      return;
    }
    const abort = new AbortController();
    const isCurrent = () =>
      !abort.signal.aborted && latestEpoch.current === epoch;
    setState({ epoch, loading: true });
    void loadGamePlans(snapshot, abort.signal)
      .then((plans) => {
        if (!isCurrent()) return;
        gamePlanCache.set(snapshot, plans);
        setState({ epoch, plans, loading: false });
      })
      .catch((error: unknown) => {
        if (isCurrent()) setState({ epoch, loading: false, error });
      });
    return () => abort.abort();
  }, [scope, key, attempt, epoch]);

  if (!scope)
    return { plans: undefined, loading: false, error: undefined, retry };
  if (key === undefined)
    return {
      plans: undefined,
      loading: false,
      error: new AppError('error.invalidResponse'),
      retry,
    };
  const cached = board && gamePlanCache.get(board);
  if (cached) return { plans: cached, loading: false, error: undefined, retry };
  if (state.epoch === epoch)
    return {
      plans: state.plans,
      loading: state.loading,
      error: state.error,
      retry,
    };
  return { plans: undefined, loading: true, error: undefined, retry };
}
