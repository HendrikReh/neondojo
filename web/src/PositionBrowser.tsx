import {
  useConfirmedConfiguration,
  useWorkspaceDefaults,
} from './ConfigurationProvider';
import { useWorkspaceLayout, useWorkspaceSession } from './useWorkspaceSession';
import ActionButton from './ActionButton';
import SearchPagination from './SearchPagination';
import { SearchResultList } from './SearchResultList';
import { useSearchPages } from './useSearchPages';
import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { request } from './api';
import {
  removePositionBookmark,
  savePositionBookmark,
} from './position-bookmark-api';
import { useI18n } from './i18n';
import type { Database } from './library-api';
import type { Theme } from './themes';
import { readPositionPlayed } from './position-table-columns';
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
  positionFilterKeys,
  togglePositionSelection,
  type PositionBrowserHit,
  type PositionSort,
  type PositionFilterDefinition,
  type PositionFilterDraft,
  type PositionFilterKey,
  type SavedPositionFilter,
} from './position-browser-api';
import { usePositionText } from './position-browser-messages';
import './position-browser.css';

const defaultPositionDefinition: PositionFilterDefinition = {
  schema_version: 1,
  sort: 'match',
};

export default function PositionBrowser({
  visible,
  revision,
  annotationRevision,
  bookmarkRevision,
  onBookmarkChanged,
  deckRevision,
  theme,
  onOpenPosition,
  onFindSimilar,
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
  onFindSimilar: (
    position: PositionBrowserHit['position'],
    matchName: string,
  ) => void;
  onDeckChanged: () => void;
  onStudy: (deckId: string) => void;
}) {
  const confirmed = useConfirmedConfiguration();
  const legacy = useWorkspaceDefaults();
  const p = usePositionText(),
    { formatError } = useI18n();
  const [definition, setDefinition] =
    useWorkspaceSession<PositionFilterDefinition>(
      'positions.filters',
      legacy?.positions.definition,
      defaultPositionDefinition,
      (value) => {
        if (
          !value ||
          typeof value !== 'object' ||
          !('schema_version' in value) ||
          value.schema_version !== 1
        )
          return undefined;
        return positionDefinition(
          positionDraft(value as PositionFilterDefinition),
        );
      },
    );
  const [editedDraft, setEditedDraft] = useState<PositionFilterDraft>();
  const draft = editedDraft ?? positionDraft(definition);
  function setDraft(next: React.SetStateAction<PositionFilterDraft>) {
    setEditedDraft((previous) =>
      typeof next === 'function' ? next(previous ?? draft) : next,
    );
  }
  const [databases, setDatabases] = useState<Database[]>([]),
    [preview, setPreview] = useState<{ page: number; id: string }>(),
    [selectedHits, setSelectedHits] = useState<PositionBrowserHit[]>([]),
    [transferBusy, setTransferBusy] = useState(false),
    [transferOpen, setTransferOpen] = useState(false),
    [removing, setRemoving] = useState(false),
    [removeError, setRemoveError] = useState<unknown>(),
    [removedIds, setRemovedIds] = useState<string[]>([]),
    [restoring, setRestoring] = useState(false),
    [restoreError, setRestoreError] = useState<unknown>(),
    [databaseError, setDatabaseError] = useState<unknown>(),
    [invalid, setInvalid] = useState(false),
    [refresh, setRefresh] = useState(0),
    [showPlayed, setShowPlayed] = useWorkspaceLayout(
      'positions.showPlayed',
      legacy?.positions.show_played,
      readPositionPlayed(),
      (value) => (typeof value === 'boolean' ? value : undefined),
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
    setTransferOpen(false);
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
      confirmed?.analysis.error_thresholds,
      refresh,
    ]),
    enabled: visible,
    loadPage,
  });
  const { items, loading, error } = results;
  // The previous results stay visible while the next page loads.
  const previewOnPage = preview?.page === results.page ? preview : undefined;
  if (preview && !previewOnPage) setPreview(undefined);
  const activePreview =
    items.find((item) => item.position.id === previewOnPage?.id) ?? items[0];

  const selected = useMemo(
    () => new Set(selectedHits.map((hit) => hit.position.id)),
    [selectedHits],
  );
  const selectionMenuId = useId();
  const selectionMenuButton = useRef<HTMLButtonElement>(null);
  const selectionMenu = useRef<HTMLDivElement>(null);
  const [selectionMenuOpen, setSelectionMenuOpen] = useState(false);
  useLayoutEffect(() => {
    const menu = selectionMenu.current;
    const trigger = selectionMenuButton.current;
    if (!selectionMenuOpen || !menu || !trigger) return;
    const gap = 4;
    const edge = 8;
    const positionMenu = () => {
      const bounds = trigger.getBoundingClientRect();
      const below = bounds.bottom + gap;
      const top =
        below + menu.offsetHeight <= window.innerHeight - edge
          ? below
          : Math.max(edge, bounds.top - menu.offsetHeight - gap);
      const left = Math.max(
        edge,
        Math.min(bounds.left, window.innerWidth - menu.offsetWidth - edge),
      );
      menu.style.top = `${top}px`;
      menu.style.left = `${left}px`;
      menu.style.visibility = 'visible';
    };
    positionMenu();
    const observer = new ResizeObserver(positionMenu);
    observer.observe(menu);
    if (trigger.parentElement) observer.observe(trigger.parentElement);
    window.addEventListener('resize', positionMenu);
    window.addEventListener('scroll', positionMenu, true);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', positionMenu);
      window.removeEventListener('scroll', positionMenu, true);
      menu.style.removeProperty('visibility');
    };
  }, [selectionMenuOpen]);
  const selectPage = useRef<HTMLInputElement>(null);
  function changeSelection(ids: string[], checked: boolean) {
    setSelectedHits((previous) => {
      const byId = new Map(previous.map((hit) => [hit.position.id, hit]));
      const next = togglePositionSelection(new Set(byId.keys()), ids, checked);
      for (const hit of items) byId.set(hit.position.id, hit);
      return [...next].flatMap((id) => {
        const hit = byId.get(id);
        return hit ? [hit] : [];
      });
    });
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
  const appliedFilters = positionDraft(definition);
  const activeFilterKeys = positionFilterKeys.filter(
    (key) => appliedFilters[key] !== '',
  );
  const hasFilters = activeFilterKeys.length > 0;
  function clearFilters() {
    const next = emptyPositionFilters();
    setDraft(next);
    apply(next);
  }
  function removeFilter(key: PositionFilterKey) {
    const cleared =
      key === 'die1' || key === 'die2' ? { die1: '', die2: '' } : { [key]: '' };
    setDraft((current) => ({ ...current, ...cleared }));
    apply({ ...appliedFilters, ...cleared });
  }
  function sortResults(sort: PositionSort) {
    if (transferBusy || removing || restoring) return;
    const currentSort = definition.sort ?? 'match';
    const direction =
      definition.direction ?? (currentSort === 'loss' ? 'desc' : 'asc');
    const nextDirection =
      currentSort === sort && direction === 'asc' ? 'desc' : 'asc';
    setDraft((previous) => ({ ...previous, sort, direction: nextDirection }));
    apply({ ...positionDraft(definition), sort, direction: nextDirection });
  }
  async function removeSaved(ids: string[]) {
    if (loading || transferBusy || removing || restoring || !ids.length) return;
    setRemoving(true);
    setRemoveError(undefined);
    setRestoreError(undefined);
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
          previous && removed.has(previous.id) ? undefined : previous,
        );
        onBookmarkChanged();
        setRefresh((value) => value + 1);
        setRemovedIds([...removed]);
      }
      if (failure) setRemoveError(failure);
    } finally {
      setRemoving(false);
    }
  }
  /** Puts the positions of the last removal back into the collection. */
  async function restoreRemoved() {
    if (loading || transferBusy || removing || restoring) return;
    const ids = removedIds;
    if (!ids.length) return;
    setRestoring(true);
    setRestoreError(undefined);
    const restored = new Set<string>();
    let failure: unknown;
    try {
      for (let start = 0; start < ids.length; start += 8) {
        const batch = ids.slice(start, start + 8);
        const saved = await Promise.allSettled(
          batch.map((id) => savePositionBookmark(id)),
        );
        saved.forEach((result, index) => {
          if (result.status === 'fulfilled') restored.add(batch[index]);
          else failure ??= result.reason;
        });
      }
      if (restored.size) {
        results.invalidateResults();
        onBookmarkChanged();
        setRefresh((value) => value + 1);
      }
      // Failed positions stay on offer for another attempt.
      setRemovedIds(ids.filter((id) => !restored.has(id)));
      if (failure) setRestoreError(failure);
    } finally {
      setRestoring(false);
    }
  }
  const busy = loading || transferBusy || removing || restoring;
  useEffect(() => {
    if (!visible || busy || !selected.size)
      selectionMenu.current?.hidePopover();
  }, [visible, busy, selected.size]);
  const selectionBar = (
    <div
      className="position-selection-summary position-selection-actions"
      hidden={!selected.size}
      aria-label={p('selectionLimit')}
      role="group"
    >
      <strong aria-live="polite">
        {p('selected', { count: selected.size })}
      </strong>
      <ActionButton
        icon="add"
        label={p('addToDeck')}
        type="button"
        className="primary"
        disabled={busy}
        onClick={() => setTransferOpen(true)}
      />
      <ActionButton
        icon="clear"
        label={p('clearSelection')}
        type="button"
        disabled={busy}
        onClick={() => {
          changeSelection([...selected], false);
          selectPage.current?.focus();
        }}
      />
      <ActionButton
        ref={selectionMenuButton}
        icon="more"
        label={p('moreSelectionActions')}
        compact
        type="button"
        popoverTarget={selectionMenuId}
        disabled={busy}
      />
      <div
        id={selectionMenuId}
        ref={selectionMenu}
        popover="auto"
        className="position-selection-menu"
        onToggle={(event) => setSelectionMenuOpen(event.newState === 'open')}
        onKeyDown={(event) => {
          if (event.key !== 'Escape') return;
          event.preventDefault();
          event.stopPropagation();
          selectionMenu.current?.hidePopover();
          selectionMenuButton.current?.focus();
        }}
      >
        <ActionButton
          icon="delete"
          label={p(removing ? 'removing' : 'removeSaved')}
          type="button"
          disabled={busy}
          onClick={() => {
            selectionMenu.current?.hidePopover();
            selectionMenuButton.current?.focus();
            void removeSaved([...selected]);
          }}
        />
      </div>
    </div>
  );
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
        applied={appliedFilters}
        activeKeys={activeFilterKeys}
        databases={databases}
        onChange={setDraft}
        onSearch={() => apply(draft)}
        onClear={clearFilters}
        onRemoveFilter={removeFilter}
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
      {!!restoreError && (
        <p role="alert" className="error">
          {p('restoreFailed')} {formatError(restoreError)}
        </p>
      )}
      {removedIds.length > 0 && (
        <p role="status" className="position-undo-notice">
          {p(restoring ? 'restoring' : 'removedNotice', {
            count: removedIds.length,
          })}
          <button
            type="button"
            disabled={busy}
            onClick={() => void restoreRemoved()}
          >
            {p('undo')}
          </button>
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
            onFindSimilar={onFindSimilar}
            showPlayed={showPlayed}
            visible={visible}
          />
        }
      >
        {loading && <p role="status">{p('loading')}</p>}
        {!items.length && !loading && !error ? (
          <>
            {selectionBar}
            <div className="position-empty" role="status">
              <p>{p(hasFilters ? 'noFilterResults' : 'empty')}</p>
              {hasFilters && (
                <button type="button" onClick={clearFilters}>
                  {p('showAllSaved')}
                </button>
              )}
            </div>
          </>
        ) : (
          <PositionBrowserResults
            items={items}
            selected={selected}
            previewId={activePreview?.position.id}
            onPreview={(id) => setPreview({ page: results.page, id })}
            onToggle={changeSelection}
            selectPageRef={selectPage}
            onOpen={onOpenPosition}
            onRemove={(ids) => void removeSaved(ids)}
            busy={busy}
            visible={visible}
            showPlayed={showPlayed}
            onShowPlayed={setShowPlayed}
            sort={definition.sort ?? 'match'}
            direction={
              definition.direction ??
              (definition.sort === 'loss' ? 'desc' : 'asc')
            }
            onSort={sortResults}
            sortDisabled={transferBusy || removing || restoring}
            toolbarStart={selectionBar}
          />
        )}
        <SearchPagination
          page={results.page}
          lastPage={results.lastPage}
          busy={loading}
          disabled={transferBusy || removing || restoring}
          onPageChange={results.setPage}
        />
        <PositionBrowserDecks
          revision={deckRevision}
          selected={selected}
          hits={selectedHits}
          transferOpen={visible && transferOpen}
          onCloseTransfer={() => setTransferOpen(false)}
          onChanged={onDeckChanged}
          onStudy={onStudy}
          onOpen={onOpenPosition}
          onBusyChange={setTransferBusy}
        />
      </SearchResultList>
    </section>
  );
}
