import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import { useEffect, useMemo, useRef, useState } from 'react';
import { request } from './api';
import { removePositionBookmark } from './position-bookmark-api';
import { useI18n } from './i18n';
import type { Database } from './library-api';
import type { Theme } from './themes';
import {
  readPositionPlayed,
  savePositionPlayed,
} from './position-table-columns';
import PositionBrowserControls from './PositionBrowserControls';
import PositionBrowserDecks from './PositionBrowserDecks';
import {
  PositionBrowserPreview,
  PositionBrowserResults,
} from './PositionBrowserResults';
import {
  emptyPositionFilters,
  loadPositionPage,
  mergePositionPages,
  positionDefinition,
  positionDraft,
  togglePositionSelection,
  type PositionBrowserHit,
  type PositionSort,
  type PositionFilterDefinition,
  type PositionFilterDraft,
  type SavedPositionFilter,
} from './position-browser-api';
import { usePositionText } from './position-browser-messages';
import './position-browser.css';

export default function PositionBrowser({
  visible,
  revision,
  annotationRevision,
  bookmarkRevision,
  onBookmarkChanged,
  deckRevision,
  theme,
  onOpenPosition,
  onDeckChanged,
  onStudy,
}: {
  visible: boolean;
  revision: number;
  annotationRevision: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  deckRevision: number;
  theme: Theme;
  onOpenPosition: (id: string) => void;
  onDeckChanged: () => void;
  onStudy: (deckId: string) => void;
}) {
  const profile = useOptionalConfiguration();
  const p = usePositionText(),
    { formatError } = useI18n();
  const [definition, setDefinition] = useProfileValue<PositionFilterDefinition>(
    (c) => c.positions.definition,
    (c, definition) => ({ ...c, positions: { ...c.positions, definition } }),
    () => positionDefinition(emptyPositionFilters()),
  );
  const [draft, setDraft] = useState<PositionFilterDraft>(() =>
    positionDraft(definition),
  );
  useEffect(() => {
    if (profile?.current) {
      setDraft(positionDraft(profile.current.positions.definition));
      setInvalid(false);
    }
  }, [profile?.generation, profile?.ready]);
  const [databases, setDatabases] = useState<Database[]>([]),
    [items, setItems] = useState<PositionBrowserHit[]>([]),
    [cursor, setCursor] = useState<string | null>(null),
    [pageIndex, setPageIndex] = useState(0),
    [previewId, setPreviewId] = useState<string>(),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [loading, setLoading] = useState(false),
    [moreLoading, setMoreLoading] = useState(false),
    [transferBusy, setTransferBusy] = useState(false),
    [removing, setRemoving] = useState(false),
    [removeError, setRemoveError] = useState<unknown>(),
    [error, setError] = useState<unknown>(),
    [databaseError, setDatabaseError] = useState<unknown>(),
    [invalid, setInvalid] = useState(false),
    [refresh, setRefresh] = useState(0),
    [showPlayed, setShowPlayed] = useProfileValue<boolean>(
      (c) => c.positions.show_played,
      (c, show_played) => ({
        ...c,
        positions: { ...c.positions, show_played },
      }),
      readPositionPlayed,
    );
  const identity = useRef(0),
    scopeRef = useRef(''),
    loadedPages = useRef(1),
    moreRequest = useRef<AbortController | undefined>(undefined);
  const scope = JSON.stringify(definition);
  useEffect(() => {
    const abort = new AbortController();
    void request<Database[]>('/databases', { signal: abort.signal })
      .then((data) => {
        if (!abort.signal.aborted) {
          setDatabases(data);
          setDatabaseError(undefined);
        }
      })
      .catch((error) => {
        if (!abort.signal.aborted) setDatabaseError(error);
      });
    return () => abort.abort();
  }, [revision]);
  useEffect(() => {
    const version = ++identity.current;
    moreRequest.current?.abort();
    setMoreLoading(false);
    if (!visible) {
      setLoading(false);
      return;
    }
    const abort = new AbortController(),
      changed = scopeRef.current !== scope;
    const pageGoal = changed ? 1 : loadedPages.current;
    scopeRef.current = scope;
    if (changed) {
      loadedPages.current = 1;
      setItems([]);
      setCursor(null);
      setPageIndex(0);
      setPreviewId(undefined);
      setSelected(new Set());
    }
    setLoading(true);
    setError(undefined);
    async function loadPrefix() {
      let all: PositionBrowserHit[] = [],
        next: string | null = null,
        pages = 0;
      do {
        const page = await loadPositionPage(
          definition,
          abort.signal,
          next ?? undefined,
        );
        if (abort.signal.aborted || version !== identity.current) return;
        all = mergePositionPages(all, page.items);
        next = page.next_cursor;
        pages++;
      } while (next && pages < pageGoal);
      if (abort.signal.aborted || version !== identity.current) return;
      loadedPages.current = pages;
      setItems(all);
      setCursor(next);
      const ids = new Set(all.map((hit) => hit.position.id));
      setPreviewId((previous) =>
        previous && ids.has(previous) ? previous : all[0]?.position.id,
      );
      setSelected(
        (previous) => new Set([...previous].filter((id) => ids.has(id))),
      );
      setPageIndex((previous) =>
        Math.min(previous, Math.max(0, Math.ceil(all.length / 25) - 1)),
      );
    }
    void loadPrefix()
      .catch((error) => {
        if (!abort.signal.aborted && version === identity.current)
          setError(error);
      })
      .finally(() => {
        if (!abort.signal.aborted && version === identity.current)
          setLoading(false);
      });
    return () => {
      abort.abort();
      moreRequest.current?.abort();
    };
  }, [
    visible,
    scope,
    revision,
    annotationRevision,
    bookmarkRevision,
    deckRevision,
    refresh,
  ]);
  function apply(next: PositionFilterDraft) {
    try {
      const value = positionDefinition(next);
      setInvalid(false);
      setDefinition(value);
      setRefresh((value) => value + 1);
    } catch {
      setInvalid(true);
    }
  }
  function loadSaved(saved: SavedPositionFilter) {
    const next = positionDraft(saved.definition);
    setDraft(next);
    apply(next);
  }
  async function more() {
    if (!cursor || loading || moreLoading) return;
    const abort = new AbortController(),
      version = identity.current,
      next = cursor;
    moreRequest.current?.abort();
    moreRequest.current = abort;
    setMoreLoading(true);
    setError(undefined);
    try {
      const page = await loadPositionPage(definition, abort.signal, next);
      if (
        abort.signal.aborted ||
        identity.current !== version ||
        scopeRef.current !== scope
      )
        return;
      setItems((previous) => mergePositionPages(previous, page.items));
      setCursor(page.next_cursor);
      setPageIndex(loadedPages.current);
      loadedPages.current++;
    } catch (error) {
      if (!abort.signal.aborted && identity.current === version)
        setError(error);
    } finally {
      if (!abort.signal.aborted && identity.current === version)
        setMoreLoading(false);
    }
  }
  function sortResults(sort: PositionSort) {
    if (transferBusy || removing) return;
    const currentSort = definition.sort ?? 'match';
    const direction =
      definition.direction ?? (currentSort === 'loss' ? 'desc' : 'asc');
    const nextDirection =
      currentSort === sort && direction === 'asc' ? 'desc' : 'asc';
    setDraft((previous) => ({ ...previous, sort, direction: nextDirection }));
    apply({ ...positionDraft(definition), sort, direction: nextDirection });
  }
  async function removeSaved(ids: string[]) {
    if (loading || moreLoading || transferBusy || removing || !ids.length)
      return;
    setRemoving(true);
    setRemoveError(undefined);
    const removed = new Set<string>();
    let failure: unknown;
    try {
      // Bound concurrent writes while preserving per-position success/failure.
      for (let start = 0; start < ids.length; start += 8) {
        const batch = ids.slice(start, start + 8);
        const results = await Promise.allSettled(
          batch.map((id) => removePositionBookmark(id)),
        );
        results.forEach((result, index) => {
          if (result.status === 'fulfilled') removed.add(batch[index]);
          else failure ??= result.reason;
        });
      }
      if (removed.size) {
        moreRequest.current?.abort();
        // Reload the complete prefix so failed selections on later pages remain retryable.
        setPageIndex(0);
        setCursor(null);
        setItems((previous) =>
          previous.filter((hit) => !removed.has(hit.position.id)),
        );
        setSelected(
          (previous) => new Set([...previous].filter((id) => !removed.has(id))),
        );
        setPreviewId((previous) =>
          previous && removed.has(previous) ? undefined : previous,
        );
        onBookmarkChanged();
        setRefresh((value) => value + 1);
      }
      if (failure) setRemoveError(failure);
    } finally {
      setRemoving(false);
    }
  }
  const pageItems = useMemo(
    () => items.slice(pageIndex * 25, (pageIndex + 1) * 25),
    [items, pageIndex],
  );
  const preview = items.find((hit) => hit.position.id === previewId),
    pageCount = Math.max(1, Math.ceil(items.length / 25));
  const busy = loading || moreLoading || transferBusy || removing;
  return (
    <section className="position-browser" aria-label={p('title')}>
      <div className="position-browser-heading">
        <div>
          <h1>{p('title')}</h1>
          <p>{p('description')}</p>
        </div>
      </div>
      <PositionBrowserControls
        filters={draft}
        databases={databases}
        onChange={setDraft}
        onSearch={() => apply(draft)}
        onClear={() => {
          const next = emptyPositionFilters();
          setDraft(next);
          apply(next);
        }}
        onLoad={loadSaved}
        busy={busy}
        revision={revision}
      />
      {invalid && (
        <p role="alert" className="error">
          {p('invalidFilter')}
        </p>
      )}
      {!!databaseError && (
        <p role="alert" className="error">
          {formatError(databaseError)}
        </p>
      )}
      {!!removeError && (
        <p role="alert" className="error">
          {p('removalFailed')} {formatError(removeError)}
        </p>
      )}
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      <div className="position-browser-layout">
        <div className="position-browser-main">
          <div className="position-result-heading">
            <h2>{p('results')}</h2>
            <span>{p('loaded', { count: items.length })}</span>
          </div>
          <div className="position-selection-summary">
            <strong>{p('selected', { count: selected.size })}</strong>
            <button
              disabled={!selected.size || busy}
              onClick={() => setSelected(new Set())}
            >
              {p('clearSelection')}
            </button>
            <button
              disabled={!selected.size || busy}
              onClick={() => void removeSaved([...selected])}
            >
              {p(removing ? 'removing' : 'removeSelected')}
            </button>
            <small>{p('selectionLimit')}</small>
          </div>
          {loading && <p role="status">{p('loading')}</p>}
          {!items.length && !loading && !error ? (
            <p className="position-empty">{p('empty')}</p>
          ) : (
            <PositionBrowserResults
              items={pageItems}
              selected={selected}
              previewId={previewId}
              onPreview={setPreviewId}
              onToggle={(ids, checked) =>
                setSelected((previous) =>
                  togglePositionSelection(previous, ids, checked),
                )
              }
              onOpen={onOpenPosition}
              onRemove={(ids) => void removeSaved(ids)}
              busy={busy}
              visible={visible}
              showPlayed={showPlayed}
              onShowPlayed={(value) => {
                setShowPlayed(value);
                if (!profile) savePositionPlayed(value);
              }}
              sort={definition.sort ?? 'match'}
              direction={
                definition.direction ??
                (definition.sort === 'loss' ? 'desc' : 'asc')
              }
              onSort={sortResults}
              sortDisabled={transferBusy || removing}
            />
          )}
          <nav className="position-pagination" aria-label={p('results')}>
            <button
              disabled={pageIndex === 0 || busy}
              onClick={() => setPageIndex((value) => value - 1)}
            >
              {p('previousPage')}
            </button>
            <span>{p('page', { number: pageIndex + 1 })}</span>
            {pageIndex + 1 < pageCount ? (
              <button
                disabled={busy}
                onClick={() => setPageIndex((value) => value + 1)}
              >
                {p('nextPage')}
              </button>
            ) : (
              <button disabled={!cursor || busy} onClick={() => void more()}>
                {p(moreLoading ? 'loading' : 'more')}
              </button>
            )}
          </nav>
          <PositionBrowserDecks
            revision={deckRevision}
            selected={selected}
            hits={items}
            onChanged={onDeckChanged}
            onStudy={onStudy}
            onOpen={onOpenPosition}
            onBusyChange={setTransferBusy}
          />
        </div>
        <PositionBrowserPreview
          hit={preview}
          theme={theme}
          onOpen={onOpenPosition}
          showPlayed={showPlayed}
        />
      </div>
    </section>
  );
}
