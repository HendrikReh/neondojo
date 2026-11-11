import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import SearchPagination, {
  lastSearchPage,
  useSearchResultPage,
} from './SearchPagination';
import { SearchResultList } from './SearchResultList';
import { useCursorSearch } from './useCursorSearch';
import { useCallback, useEffect, useState } from 'react';
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
  POSITION_PAGE_SIZE,
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

const positionKey = (hit: PositionBrowserHit) => hit.position.id;

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
    [pageIndex, setPageIndex] = useState(0),
    [previewId, setPreviewId] = useState<string>(),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [transferBusy, setTransferBusy] = useState(false),
    [removing, setRemoving] = useState(false),
    [removeError, setRemoveError] = useState<unknown>(),
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
    setPageIndex(0);
    setPreviewId(undefined);
    setSelected(new Set());
  }, [scope]);
  const loadPage = useCallback(
    (signal: AbortSignal, after?: string) =>
      loadPositionPage(definition, signal, after),
    [definition],
  );
  const onReplace = useCallback((items: PositionBrowserHit[]) => {
    const ids = new Set(items.map(positionKey));
    setPreviewId((previous) =>
      previous && ids.has(previous) ? previous : items[0]?.position.id,
    );
    setSelected(
      (previous) => new Set([...previous].filter((id) => ids.has(id))),
    );
    setPageIndex((previous) =>
      Math.min(previous, lastSearchPage(items.length, POSITION_PAGE_SIZE)),
    );
  }, []);
  const {
    items,
    cursor,
    loading,
    moreLoading,
    error,
    more,
    invalidateResults,
  } = useCursorSearch({
    scope,
    revision: JSON.stringify([
      revision,
      annotationRevision,
      bookmarkRevision,
      deckRevision,
      refresh,
    ]),
    enabled: visible,
    keepLoadedPages: true,
    loadPage,
    itemKey: positionKey,
    onReplace,
  });
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
        // Reload the complete prefix so failed selections on later pages remain retryable.
        setPageIndex(0);
        invalidateResults(removed);
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
  const { page, lastPage, rows } = useSearchResultPage(
    items,
    POSITION_PAGE_SIZE,
    pageIndex,
  );
  const preview = items.find((hit) => hit.position.id === previewId);
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
      <SearchResultList
        count={<span>{p('loaded', { count: items.length })}</span>}
        preview={
          <PositionBrowserPreview
            hit={preview}
            theme={theme}
            onOpen={onOpenPosition}
            showPlayed={showPlayed}
            visible={visible}
          />
        }
      >
        <div className="position-selection-summary">
          <strong>{p('selected', { count: selected.size })}</strong>
          <ActionButton
            icon="clear"
            label={p('clearSelection')}
            compact
            disabled={!selected.size || busy}
            onClick={() => setSelected(new Set())}
          />
          <ActionButton
            icon="delete"
            label={p(removing ? 'removing' : 'removeSelected')}
            compact
            disabled={!selected.size || busy}
            onClick={() => void removeSaved([...selected])}
          />
          <small>{p('selectionLimit')}</small>
        </div>
        {loading && <p role="status">{p('loading')}</p>}
        {!items.length && !loading && !error ? (
          <p className="position-empty">{p('empty')}</p>
        ) : (
          <PositionBrowserResults
            items={rows}
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
        <SearchPagination
          page={page}
          lastPage={lastPage}
          busy={busy}
          onPageChange={setPageIndex}
          onMore={cursor ? more : undefined}
          moreLabel={p(moreLoading ? 'loading' : 'more')}
        />
        <PositionBrowserDecks
          revision={deckRevision}
          selected={selected}
          hits={items}
          onChanged={onDeckChanged}
          onStudy={onStudy}
          onOpen={onOpenPosition}
          onBusyChange={setTransferBusy}
        />
      </SearchResultList>
    </section>
  );
}
