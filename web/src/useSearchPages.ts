import { useEffect, useState } from 'react';

export type SearchPage<T> = { items: T[]; total?: number | null };

export function lastSearchPage(total: number, pageSize: number) {
  return Math.max(1, Math.ceil(total / pageSize));
}

const emptyItems: never[] = [];

export function useSearchPages<T extends SearchPage<unknown>>({
  scope,
  revision,
  pageSize,
  loadPage,
  enabled = true,
  debounceMs = 0,
}: {
  scope: string;
  revision?: string | number;
  pageSize: number;
  loadPage: (signal: AbortSignal, page: number) => Promise<T>;
  enabled?: boolean;
  debounceMs?: number;
}) {
  const [target, setTarget] = useState({ scope, page: 1 });
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<
    T & { scope: string; key: string; page: number }
  >();
  const [failure, setFailure] = useState<{ key: string; error: unknown }>();
  if (target.scope !== scope) {
    setTarget({ scope, page: 1 });
    setResult(undefined);
    setFailure(undefined);
  }
  const page = target.scope === scope ? target.page : 1;
  const key = JSON.stringify([scope, revision, page, attempt]);
  const current = result?.scope === scope ? result : undefined;

  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    async function load() {
      try {
        const response = await loadPage(abort.signal, page);
        if (abort.signal.aborted) return;
        const total = response.total ?? response.items.length;
        const last = lastSearchPage(total, pageSize);
        if (page > last) {
          setTarget({ scope, page: last });
          return;
        }
        setResult({ ...response, total, scope, key, page });
        setFailure(undefined);
      } catch (error) {
        if (!abort.signal.aborted) setFailure({ key, error });
      }
    }
    const timer =
      debounceMs > 0 ? setTimeout(() => void load(), debounceMs) : undefined;
    if (timer === undefined) void load();
    return () => {
      clearTimeout(timer);
      abort.abort();
    };
  }, [enabled, scope, key, loadPage, page, pageSize, debounceMs]);

  const error = failure?.key === key ? failure.error : undefined;
  const loading = enabled && current?.key !== key && failure?.key !== key;
  return {
    items: current ? (current.items as T['items']) : emptyItems,
    result: current,
    total: current?.total ?? 0,
    page: current?.page ?? page,
    lastPage: lastSearchPage(current?.total ?? 0, pageSize),
    resultScope: current?.scope,
    loading,
    error,
    setPage: (next: number) => {
      if (next === page && failure?.key === key)
        setAttempt((previous) => previous + 1);
      setTarget({ scope, page: next });
    },
    retry: () => setAttempt((previous) => previous + 1),
    invalidateResults: () => {
      setResult(undefined);
      setAttempt((previous) => previous + 1);
    },
  };
}
