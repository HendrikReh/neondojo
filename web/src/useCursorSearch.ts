import { useEffect, useRef, useState } from 'react';
import type { Page } from './api';
import { mergeSearchItems } from './cursor-search';

type SearchPage<T> = Page<T> & { total?: number };

type SearchRun = {
  key: string;
  initial: AbortController;
  more?: AbortController;
};

type SearchStatus = {
  key: string;
  pending?: 'initial' | 'more';
  error?: unknown;
};

const emptyItems: never[] = [];

export function useCursorSearch<T>({
  scope,
  revision,
  loadPage,
  itemKey,
  enabled = true,
  debounceMs = 0,
  keepLoadedPages = false,
  onReplace,
}: {
  scope: string;
  revision?: string | number;
  loadPage: (signal: AbortSignal, after?: string) => Promise<SearchPage<T>>;
  itemKey: (item: T) => string;
  enabled?: boolean;
  debounceMs?: number;
  keepLoadedPages?: boolean;
  onReplace?: (items: T[]) => void;
}) {
  const key = JSON.stringify([scope, revision]);
  const [result, setResult] = useState<SearchPage<T> & { scope: string }>();
  const [status, setStatus] = useState<SearchStatus>();
  const active = useRef<SearchRun | undefined>(undefined);
  const prefix = useRef({ scope, pages: 1 });

  useEffect(() => {
    if (!enabled) return;
    const initial = new AbortController();
    const run: SearchRun = { key, initial };
    active.current = run;
    const pageGoal =
      keepLoadedPages && prefix.current.scope === scope
        ? prefix.current.pages
        : 1;
    prefix.current = { scope, pages: pageGoal };
    setResult((previous) => (previous?.scope === scope ? previous : undefined));
    setStatus({ key, pending: 'initial' });

    async function load() {
      try {
        let page = await loadPage(initial.signal);
        if (initial.signal.aborted) return;
        let pages = 1;
        while (page.next_cursor && pages < pageGoal) {
          const next = await loadPage(initial.signal, page.next_cursor);
          if (initial.signal.aborted) return;
          page = {
            ...next,
            items: mergeSearchItems(page.items, next.items, itemKey),
          };
          pages++;
        }
        prefix.current = { scope, pages };
        setResult({ ...page, scope });
        onReplace?.(page.items);
        setStatus({ key });
      } catch (error) {
        if (!initial.signal.aborted) setStatus({ key, error });
      }
    }

    const timer = debounceMs
      ? setTimeout(() => void load(), debounceMs)
      : undefined;
    if (timer === undefined) void load();
    return () => {
      clearTimeout(timer);
      initial.abort();
      run.more?.abort();
      active.current = undefined;
    };
  }, [
    key,
    scope,
    enabled,
    debounceMs,
    keepLoadedPages,
    itemKey,
    loadPage,
    onReplace,
  ]);

  const current = result?.scope === scope ? result : undefined;
  const currentStatus = status?.key === key ? status : undefined;
  const loading =
    enabled && (!currentStatus || currentStatus.pending === 'initial');
  const moreLoading = enabled && currentStatus?.pending === 'more';

  async function more() {
    const run = active.current;
    if (
      !enabled ||
      !current?.next_cursor ||
      loading ||
      moreLoading ||
      run?.key !== key ||
      run.more ||
      run.initial.signal.aborted
    )
      return false;
    const request = new AbortController();
    run.more = request;
    setStatus({ key, pending: 'more' });
    try {
      const page = await loadPage(request.signal, current.next_cursor);
      if (request.signal.aborted) return false;
      setResult((previous) =>
        previous?.scope === scope
          ? {
              ...page,
              scope,
              items: mergeSearchItems(previous.items, page.items, itemKey),
            }
          : previous,
      );
      prefix.current.pages++;
      setStatus({ key });
      return true;
    } catch (error) {
      if (!request.signal.aborted) setStatus({ key, error });
      return false;
    } finally {
      run.more = undefined;
    }
  }

  // A mutation invalidates the cursor before the caller refreshes the query.
  function invalidateResults(removed?: ReadonlySet<string>) {
    active.current?.initial.abort();
    active.current?.more?.abort();
    setResult((previous) =>
      removed && previous?.scope === scope
        ? {
            ...previous,
            items: previous.items.filter((item) => !removed.has(itemKey(item))),
            next_cursor: null,
          }
        : undefined,
    );
  }

  return {
    items: current?.items ?? emptyItems,
    resultScope: current?.scope,
    cursor: current?.next_cursor ?? null,
    total: current?.total,
    loading,
    moreLoading,
    error: currentStatus?.error,
    more,
    invalidateResults,
  };
}
