import { useCallback, useEffect, useState, type RefObject, type SetStateAction } from 'react';
import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import {
  sessionFields,
  useWorkspaceSession,
} from '../../shared/hooks/useWorkspaceSession';
import { useLatest } from '../../shared/hooks/useLatest';
import { useSearchPages } from '../../shared/hooks/useSearchPages';
import {
  emptyFilters,
  isOccurrenceSort,
  librarySorts,
  loadLibrary,
  normalizeLibraryFilters,
  type LibraryFilters as Filters,
  type LibrarySort,
} from './library-api';
import { useDatabases } from './useDatabases';
import type { LibraryFilterKey } from './LibraryFilters';
import type { PositionSearchRequest } from './MatchDatabase';

const defaultLibraryFilters = emptyFilters();

export function useLibraryQuery({
  visible,
  databaseId,
  revision,
  localRevision,
  analysisRevision,
  annotationRevision,
  requestedPositionSearch,
  onDatabase,
  filterToggleRef,
  positionToggleRef,
}: {
  visible: boolean;
  databaseId: string;
  revision: number;
  localRevision: number;
  analysisRevision?: string;
  annotationRevision: number;
  requestedPositionSearch?: PositionSearchRequest;
  onDatabase: (id: string) => void;
  filterToggleRef: RefObject<HTMLButtonElement | null>;
  positionToggleRef: RefObject<HTMLButtonElement | null>;
}) {
  const legacy = useWorkspaceDefaults();
  const [filters, setAppliedFilters] = useWorkspaceSession(
    'library.filters',
    legacy?.library.filters,
    defaultLibraryFilters,
    (value) => sessionFields(value, defaultLibraryFilters),
  );
  const [filterInput, setFilterInput] = useState<Filters>();
  const draftFilters = filterInput ?? filters;
  const filterDraft = useLatest(draftFilters);

  const setFilters = (next: SetStateAction<Filters>) => {
    const value = typeof next === 'function' ? next(filterDraft.current) : next;
    filterDraft.current = value;
    setFilterInput(value);
    setAppliedFilters(normalizeLibraryFilters(value));
  };

  function removeFilter(key: LibraryFilterKey) {
    (key === 'position_id' ? positionToggleRef : filterToggleRef).current?.focus();
    setAppliedFilters((previous) => ({ ...previous, [key]: '' }));
    const draft = { ...filterDraft.current, [key]: '' };
    filterDraft.current = draft;
    setFilterInput(draft);
  }

  const [preferredSort, setSort] = useWorkspaceSession<LibrarySort>(
    'library.sort',
    legacy?.library.sort,
    'created_at',
    (value) => librarySorts.find((sort) => sort === value),
  );
  const sort =
    !filters.position_id && isOccurrenceSort(preferredSort)
      ? 'created_at'
      : preferredSort;
  const [direction, setDirection] = useWorkspaceSession<'asc' | 'desc'>(
    'library.direction',
    legacy?.library.direction,
    'desc',
    (value) => (value === 'asc' || value === 'desc' ? value : undefined),
  );

  const [exact, setExact] = useState('');
  const [positionInputError, setPositionInputError] = useState(false);

  useEffect(() => {
    setExact(filters.position_id);
    setPositionInputError(false);
  }, [filters.position_id]);

  const matchScope = JSON.stringify([
    databaseId,
    filters,
    sort,
    direction,
    requestedPositionSearch?.token,
  ]);

  const { databases, error: databaseError } = useDatabases(
    `${revision}:${localRevision}`,
    {
      onLoad: (list) => {
        if (databaseId && !list.some((d) => d.id === databaseId))
          onDatabase('');
      },
    },
  );

  const loadMatches = useCallback(
    (signal: AbortSignal, page: number) =>
      loadLibrary(databaseId, filters, sort, direction, signal, page),
    [databaseId, filters, sort, direction],
  );

  const pages = useSearchPages({
    pageSize: 50,
    scope: matchScope,
    revision: JSON.stringify([
      revision,
      localRevision,
      analysisRevision,
      annotationRevision,
    ]),
    loadPage: loadMatches,
    enabled: visible,
    debounce: {
      key: JSON.stringify([
        filters.q,
        filters.player,
        filters.match_name,
        filters.tournament,
        filters.round,
        filters.tag,
        filters.comment,
      ]),
      ms: 250,
    },
  });

  function searchPosition() {
    const positionId = exact.trim();
    if (!/^[A-Za-z0-9+/]{14}$/.test(positionId)) {
      setPositionInputError(true);
      return;
    }
    setPositionInputError(false);
    setFilters((previous) => ({ ...previous, position_id: positionId }));
  }

  function setPositionQuery(value: string) {
    setExact(value);
    setPositionInputError(false);
  }

  function sortBy(key: LibrarySort) {
    if (sort === key) {
      setSort(key);
      setDirection((d) => (d === 'asc' ? 'desc' : 'asc'));
    } else {
      setSort(key);
      setDirection('asc');
    }
  }

  const hasFilters = Object.entries(filters).some(
    ([key, value]) => key !== 'date_field' && value.trim(),
  );

  return {
    filters,
    draftFilters,
    setFilters,
    removeFilter,
    sort,
    direction,
    sortBy,
    exact,
    positionInputError,
    setPositionQuery,
    searchPosition,
    matchScope,
    databases,
    databaseError,
    hasFilters,
    ...pages,
  };
}
