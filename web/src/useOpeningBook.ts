import { useEffect, useState } from 'react';
import { bookLookup, type BookLookup } from './source-analysis-api';
export function useOpeningBook(positionId?: string, revision?: number) {
  const [state, setState] = useState<{
    positionId: string;
    book?: BookLookup;
    error?: unknown;
  }>();
  useEffect(() => {
    if (!positionId) return;
    const abort = new AbortController();
    void bookLookup(positionId, abort.signal).then(
      (book) => {
        if (!abort.signal.aborted) setState({ positionId, book });
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setState({ positionId, error });
      },
    );
    return () => abort.abort();
  }, [positionId, revision]);
  return state?.positionId === positionId
    ? { ...state, loading: false }
    : { loading: !!positionId };
}
