import { useEffect, useState } from 'react';
import { loadCubeContext, type CubeContext } from './cube-context-api';

/** Load the score-dependent cube context; stale responses are dropped. */
export function useCubeContext(matchId?: string, doubler?: number) {
  const key = matchId && doubler != null ? `${matchId}:${doubler}` : '';
  const [state, setState] = useState<{
    key: string;
    context?: Readonly<CubeContext>;
    error?: unknown;
  }>({ key: '' });
  useEffect(() => {
    if (!key || !matchId || doubler == null) return;
    const abort = new AbortController();
    void loadCubeContext(matchId, doubler, abort.signal)
      .then((context) => {
        if (!abort.signal.aborted) setState({ key, context });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setState({ key, error });
      });
    return () => abort.abort();
  }, [key, matchId, doubler]);
  return state.key === key && key
    ? { context: state.context, error: state.error }
    : { context: undefined, error: undefined };
}
