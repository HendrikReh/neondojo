import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import SearchPagination from './SearchPagination';
import { SearchResultList } from './SearchResultList';
import { useSearchPages } from './useSearchPages';
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
    [preview, setPreview] = useState<PositionBrowserHit>(),
    [selectedHits, setSelectedHits] = useState<PositionBrowserHit[]>([]),
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
    setPreview(undefined);
    setSelectedHits([]);
  }, [scope]);
  const loadPage = useCallback(
    (signal: AbortSignal, page: number) =>
      loadPositionPage(definition, signal, page),
    [definition],
  );
  const results = useSearchPages({
    scope,
    pageSize: POSITION_PAGE_SIZE,
    revision: JSON.stringify([
      revision,
      annotationRevision,
      bookmarkRevision,
      deckRevision,
      refresh,
    ]),
    enabled: visible,
    loadPage,
  });
  const { items, loading, error } = results;
  const activePreview = preview
    ? (items.find((item) => item.position.id === preview.position.id) ??
      preview)
    : items[0];

  const selected = new Set(selectedHits.map((hit) => hit.position.id));
  function changeSelection(ids: string[], checked: boolean) {
    const next = togglePositionSelection(selected, ids, checked);
    setSelectedHits([
      ...selectedHits.filter(
        (hit) =>
          next.has(hit.position.id) &&
          !items.some((item) => item.position.id === hit.position.id),
      ),
      ...items.filter((hit) => next.has(hit.position.id)),
    ]);
  }
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
    if (loading || transferBusy || removing || !ids.length) return;
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
        results.invalidateResults();
        setSelectedHits((previous) =>
          previous.filter((hit) => !removed.has(hit.position.id)),
        );
        setPreview((previous) =>
          previous && removed.has(previous.position.id) ? undefined : previous,
        );
        onBookmarkChanged();
        setRefresh((value) => value + 1);
      }
      if (failure) setRemoveError(failure);
    } finally {
      setRemoving(false);
    }
  }
  const busy = loading || transferBusy || removing;
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
          {formatError(error)}{' '}
          <button type="button" onClick={results.retry}>
            {p('retrySimilar')}
          </button>
        </p>
      )}
      <SearchResultList
        count={<span>{p('resultCount', { count: results.total })}</span>}
        preview={
          <PositionBrowserPreview
            hit={activePreview}
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
            onClick={() => changeSelection([...selected], false)}
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
            items={items}
            selected={selected}
            previewId={activePreview?.position.id}
            onPreview={(id) =>
              setPreview(items.find((hit) => hit.position.id === id))
            }
            onToggle={changeSelection}
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
          page={results.page}
          lastPage={results.lastPage}
          busy={loading}
          disabled={transferBusy || removing}
          onPageChange={results.setPage}
        />
        <PositionBrowserDecks
          revision={deckRevision}
          selected={selected}
          hits={selectedHits}
          onChanged={onDeckChanged}
          onStudy={onStudy}
          onOpen={onOpenPosition}
          onBusyChange={setTransferBusy}
        />
      </SearchResultList>
    </section>
  );
}
