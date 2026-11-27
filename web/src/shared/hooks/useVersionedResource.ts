import { useEffect, useState } from 'react';
import { useLatest } from './useLatest';

export interface VersionedResource {
  id: string;
  version: number;
}

/**
 * Loads a versioned entity by ID, ignoring stale responses whose version is
 * older than what is already stored for the same ID.
 */
export function useVersionedResource<T extends VersionedResource>(
  id: string | undefined,
  load: (id: string, signal: AbortSignal) => Promise<T>,
  revision = 0,
) {
  const [details, setDetails] = useState<T>();
  const [failure, setFailure] = useState<{ id: string; error: unknown }>();
  const [attempt, setAttempt] = useState(0);
  const latestLoad = useLatest(load);

  useEffect(() => {
    if (!id) return;
    const abort = new AbortController();
    setFailure(undefined);
    latestLoad.current(id, abort.signal).then(
      (row) => {
        if (!abort.signal.aborted) {
          setDetails((previous) =>
            previous?.id === id && previous.version > row.version
              ? previous
              : row,
          );
        }
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setFailure({ id, error });
      },
    );
    return () => abort.abort();
  }, [id, revision, attempt, latestLoad]);

  return {
    details: id && details?.id === id ? details : undefined,
    setDetails,
    error: id && failure?.id === id ? failure.error : undefined,
    retry: () => setAttempt((n) => n + 1),
  };
}
