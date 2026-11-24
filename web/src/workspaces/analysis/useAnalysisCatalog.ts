import { useEffect, useRef, useState } from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import { request, terminal } from '../../shared/http/api';
import { watchJobChanges } from '../../shared/jobs/job-watch';
import {
  analysisCatalogPath,
  analysisResultPath,
  mergeCatalogEntries,
  type AnalysisCatalogEntry,
  type AnalysisCatalogPage,
  type PositionAnalysisResult,
} from './analysis-catalog-api';

/** Catalog reads while pending entries report progress. */
export const CATALOG_PROGRESS_READ_MS = 3_000;

function isPending(entry: AnalysisCatalogEntry | null | undefined) {
  return entry?.status === 'running' || entry?.status === 'queued';
}

function withSelectedEntry(
  items: AnalysisCatalogEntry[],
  selected: AnalysisCatalogEntry | null | undefined,
) {
  return selected
    ? items.map((item) => (item.id === selected.id ? selected : item))
    : items;
}

export function useAnalysisCatalog(
  matchId?: string,
  positionId?: string,
  revision?: string,
) {
  const scope = JSON.stringify([matchId, positionId]);
  const current = useLatest(scope);
  const currentMatch = useLatest(matchId);
  const readVersion = useRef(0);
  const [generation, setGeneration] = useState(0);
  const [state, setState] = useState<{
    scope: string;
    page: AnalysisCatalogPage;
  }>();
  // The effective selection and source revision belong to the match; only the
  // entries depend on the position. Keep the match's last page so moving to
  // another position does not clear the selection that dependent views read.
  const [matchPage, setMatchPage] = useState<{
    matchId: string;
    page: AnalysisCatalogPage;
  }>();
  const [failure, setFailure] = useState<{ scope: string; error: unknown }>();
  const [busy, setBusy] = useState<string>();
  const [loadingMore, setLoadingMore] = useState<string>();
  const page = state?.scope === scope ? state.page : undefined;
  const currentPage = useLatest(page);
  const refresh = () => {
    readVersion.current += 1;
    setGeneration((value) => value + 1);
  };
  useEffect(() => {
    if (!matchId) return;
    const abort = new AbortController();
    const version = readVersion.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reads = 0;
    let lastRead = 0;
    let watched = '';
    let stopWatching: (() => void) | undefined;
    function reloadSoon(immediately: boolean) {
      if (immediately) clearTimeout(timer);
      else if (timer !== undefined) return;
      timer = setTimeout(
        () => {
          timer = undefined;
          void load(true);
        },
        immediately
          ? 0
          : Math.max(0, CATALOG_PROGRESS_READ_MS - (Date.now() - lastRead)),
      );
    }
    // Pending entries follow their jobs over the shared job watch: a finished
    // job reloads the catalog at once, progress at most every few seconds.
    function follow(page: AnalysisCatalogPage) {
      const pending = [page.selected, ...page.items].filter(isPending);
      const jobs = [
        ...new Map(
          pending.flatMap((entry) =>
            entry?.job && !terminal(entry.job)
              ? [[entry.job.id, entry.job] as const]
              : [],
          ),
        ).values(),
      ];
      const key = jobs
        .map((job) => job.id)
        .sort()
        .join(',');
      if (key !== watched) {
        stopWatching?.();
        stopWatching = jobs.length
          ? watchJobChanges(jobs, (job) => reloadSoon(!job || terminal(job)))
          : undefined;
        watched = key;
      }
      // Pending entries without a job to follow are read again periodically.
      if (pending.some((entry) => !entry?.job || terminal(entry.job)))
        reloadSoon(false);
    }
    async function load(poll = false) {
      const read = ++reads;
      lastRead = Date.now();
      try {
        const next = await request<AnalysisCatalogPage>(
          analysisCatalogPath(matchId!, positionId),
          { signal: abort.signal },
        );
        if (
          abort.signal.aborted ||
          read !== reads ||
          current.current !== scope ||
          version !== readVersion.current ||
          next.match_id !== matchId
        )
          return;
        setState((previous) =>
          poll &&
          previous?.scope === scope &&
          previous.page.source_revision === next.source_revision
            ? {
                scope,
                page: {
                  ...next,
                  items: withSelectedEntry(
                    mergeCatalogEntries(previous.page.items, next.items),
                    next.selected,
                  ),
                  next_cursor: previous.page.next_cursor,
                },
              }
            : { scope, page: next },
        );
        setMatchPage({ matchId: matchId!, page: next });
        setFailure(undefined);
        follow(next);
      } catch (error) {
        if (
          !abort.signal.aborted &&
          current.current === scope &&
          version === readVersion.current
        )
          setFailure({ scope, error });
      }
    }
    void load();
    return () => {
      abort.abort();
      clearTimeout(timer);
      stopWatching?.();
    };
  }, [matchId, positionId, scope, revision, generation]);

  async function write(path: string, options: RequestInit) {
    if (!matchId || busy === matchId) return false;
    readVersion.current += 1;
    setBusy(matchId);
    setFailure(undefined);
    try {
      await request(`/matches/${encodeURIComponent(matchId)}${path}`, options);
      if (currentMatch.current !== matchId) return false;
      // The server resolves automatic selection. Keep dependent readers empty until
      // its authoritative response arrives, including when the option is off-page.
      setState(undefined);
      setMatchPage(undefined);
      refresh();
      return true;
    } catch (error) {
      if (currentMatch.current === matchId)
        setFailure({ scope: current.current, error });
      return false;
    } finally {
      setBusy((pending) => (pending === matchId ? undefined : pending));
    }
  }
  async function select(analysisId: string | null) {
    await write('/analysis-selection', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ analysis_id: analysisId }),
    });
  }
  function remove(analysisId: string, sourceRevision: number) {
    const query = new URLSearchParams({
      source_revision: String(sourceRevision),
    });
    return write(`/analyses/${encodeURIComponent(analysisId)}?${query}`, {
      method: 'DELETE',
    });
  }
  async function loadMore() {
    if (!matchId || !page?.next_cursor || loadingMore === scope) return;
    const version = readVersion.current;
    const { next_cursor: cursor, source_revision: source } = page;
    const isCurrentPage = () =>
      current.current === scope &&
      version === readVersion.current &&
      currentPage.current?.source_revision === source &&
      currentPage.current.next_cursor === cursor;
    setLoadingMore(scope);
    try {
      const next = await request<AnalysisCatalogPage>(
        analysisCatalogPath(matchId, positionId, cursor),
      );
      if (
        !isCurrentPage() ||
        next.match_id !== matchId ||
        next.source_revision !== source
      )
        return;
      setState((previous) => {
        if (
          previous?.scope !== scope ||
          previous.page.source_revision !== source ||
          previous.page.next_cursor !== cursor
        )
          return previous;
        // A page can arrive after a newer head response. Only the head owns
        // the displayed selection and checkpoint metadata.
        const loaded = new Set(previous.page.items.map((item) => item.id));
        return {
          scope,
          page: {
            ...previous.page,
            next_cursor: next.next_cursor,
            items: withSelectedEntry(
              [
                ...previous.page.items,
                ...next.items.filter((item) => !loaded.has(item.id)),
              ],
              previous.page.selected,
            ),
          },
        };
      });
      setFailure(undefined);
    } catch (error) {
      if (isCurrentPage()) setFailure({ scope, error });
    } finally {
      setLoadingMore((pending) => (pending === scope ? undefined : pending));
    }
  }
  const failed = failure?.scope === scope;
  const known =
    page ??
    (matchId && matchPage?.matchId === matchId && !failed
      ? matchPage.page
      : undefined);
  return {
    sourceRevision: known?.source_revision,
    entries: page?.items ?? [],
    selected: busy === matchId ? undefined : (known?.selected ?? undefined),
    selectedId: known?.selected_id ?? null,
    analysisId: busy === matchId ? undefined : known?.selected?.id,
    loading: !!matchId && !page && failure?.scope !== scope,
    saving: !!matchId && busy === matchId,
    loadingMore: loadingMore === scope,
    nextCursor: page?.next_cursor,
    error: failed ? failure.error : undefined,
    select,
    remove,
    loadMore,
    refresh,
  };
}
export type AnalysisCatalog = ReturnType<typeof useAnalysisCatalog>;

export function useAnalysisResult(
  positionId?: string,
  analysisId?: string,
  revision?: string,
) {
  const path =
    positionId && analysisId
      ? analysisResultPath(positionId, analysisId)
      : undefined;
  const scope = JSON.stringify([path, revision]);
  const [state, setState] = useState<{
    scope: string;
    result?: PositionAnalysisResult | null;
    error?: unknown;
  }>();
  useEffect(() => {
    if (!path) return;
    const abort = new AbortController();
    void request<PositionAnalysisResult | null>(path, {
      signal: abort.signal,
    }).then(
      (result) => {
        if (!abort.signal.aborted)
          setState({
            scope,
            result:
              result &&
              result.position_id === positionId &&
              result.analysis_id === analysisId
                ? result
                : null,
          });
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setState({ scope, error });
      },
    );
    return () => abort.abort();
  }, [path, scope, positionId, analysisId]);
  return path && state?.scope === scope
    ? { ...state, loading: false }
    : { loading: !!path, result: undefined, error: undefined };
}
