import { useEffect, useRef, useState } from 'react';
import { request } from './api';
import {
  analysisCatalogPath,
  analysisResultPath,
  mergeCatalogEntries,
  type AnalysisCatalogPage,
  type PositionAnalysisResult,
} from './analysis-catalog-api';

export function useAnalysisCatalog(
  matchId?: string,
  positionId?: string,
  revision?: string,
) {
  const scope = JSON.stringify([matchId, positionId]);
  const current = useRef(scope);
  current.current = scope;
  const currentMatch = useRef(matchId);
  currentMatch.current = matchId;
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
  const refresh = () => {
    readVersion.current += 1;
    setGeneration((value) => value + 1);
  };
  useEffect(() => {
    if (!matchId) return;
    const abort = new AbortController();
    const version = readVersion.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    async function load(poll = false) {
      try {
        const next = await request<AnalysisCatalogPage>(
          analysisCatalogPath(matchId!, positionId),
          { signal: abort.signal },
        );
        if (
          abort.signal.aborted ||
          current.current !== scope ||
          version !== readVersion.current ||
          next.match_id !== matchId
        )
          return;
        setState((previous) =>
          poll && previous?.scope === scope
            ? {
                scope,
                page: {
                  ...next,
                  items: mergeCatalogEntries(previous.page.items, next.items),
                  next_cursor: previous.page.next_cursor,
                },
              }
            : { scope, page: next },
        );
        setMatchPage({ matchId: matchId!, page: next });
        setFailure(undefined);
        if (
          next.items.some(
            (entry) => entry.status === 'running' || entry.status === 'queued',
          )
        )
          timer = setTimeout(() => {
            void load(true);
          }, 3000);
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
    setLoadingMore(scope);
    try {
      const next = await request<AnalysisCatalogPage>(
        analysisCatalogPath(matchId, positionId, page.next_cursor),
      );
      if (
        current.current !== scope ||
        version !== readVersion.current ||
        next.match_id !== matchId
      )
        return;
      setState((previous) =>
        previous?.scope === scope
          ? {
              scope,
              page: {
                ...next,
                items: mergeCatalogEntries(previous.page.items, next.items),
              },
            }
          : previous,
      );
      setMatchPage({ matchId, page: next });
      setFailure(undefined);
    } catch (error) {
      if (current.current === scope && version === readVersion.current)
        setFailure({ scope, error });
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
