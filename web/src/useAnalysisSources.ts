import { useEffect, useState } from 'react';
import {
  bookLookup,
  sourceAnalyses,
  type SourceAnalysis,
  type BookLookup,
} from './source-analysis-api';
export function useAnalysisSources(
  positionId: string | undefined,
  revision: number,
) {
  const [state, setState] = useState<{
    positionId?: string;
    records: SourceAnalysis[];
    book?: BookLookup;
    errors: unknown[];
    loading: boolean;
  }>({ records: [], errors: [], loading: false });
  useEffect(() => {
    setState({ positionId, records: [], errors: [], loading: !!positionId });
    if (!positionId) return;
    const abort = new AbortController();
    void Promise.allSettled([
      sourceAnalyses(positionId, abort.signal),
      bookLookup(positionId, abort.signal),
    ]).then(([source, book]) => {
      if (abort.signal.aborted) return;
      setState({
        positionId,
        records: source.status === 'fulfilled' ? source.value : [],
        book: book.status === 'fulfilled' ? book.value : undefined,
        errors: [source, book].flatMap((r) =>
          r.status === 'rejected' ? [r.reason] : [],
        ),
        loading: false,
      });
    });
    return () => abort.abort();
  }, [positionId, revision]);
  return state.positionId === positionId
    ? state
    : { positionId, records: [], errors: [], loading: !!positionId };
}
