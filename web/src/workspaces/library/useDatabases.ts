import { useEffect, useRef, useState } from 'react';
import { request } from '../../shared/api/api';
import type { Database } from './library-api';

export interface DatabaseList {
  databases: Database[];
  /** The last failure; cleared by the next successful load. */
  error: unknown;
}

/**
 * Loads the library databases and reloads them whenever `reloadKey` changes.
 * A disabled hook keeps its last list and does not request. A newer load
 * aborts the previous one, so a late response never replaces a newer list.
 * `onLoad` sees each successful list before React renders it.
 */
export function useDatabases(
  reloadKey: string | number,
  {
    enabled = true,
    onLoad,
  }: { enabled?: boolean; onLoad?: (databases: Database[]) => void } = {},
): DatabaseList {
  const [databases, setDatabases] = useState<Database[]>([]);
  const [error, setError] = useState<unknown>();
  const loaded = useRef(onLoad);
  loaded.current = onLoad;
  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    request<Database[]>('/databases', { signal: abort.signal }).then(
      (list) => {
        if (abort.signal.aborted) return;
        setDatabases(list);
        setError(undefined);
        loaded.current?.(list);
      },
      (failure: unknown) => {
        if (!abort.signal.aborted) setError(failure);
      },
    );
    return () => abort.abort();
  }, [enabled, reloadKey]);
  return { databases, error };
}
