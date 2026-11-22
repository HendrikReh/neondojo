import Tooltip from './shared/ui/Tooltip';
import { useWorkspaceDefaults } from './ConfigurationProvider';
import { Children, type HTMLAttributes, type ReactNode } from 'react';
import {
  sessionFields,
  useWorkspaceSession,
} from './shared/hooks/useWorkspaceSession';
import ActionButton from './shared/ui/ActionButton';
import ActionIcon from './shared/ui/action-icons';
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type SetStateAction,
} from 'react';
import { request, type Match, type Position } from './shared/api/api';
import SearchPagination from './shared/ui/SearchPagination';
import { useSearchPages } from './shared/hooks/useSearchPages';
import { useLibraryMatchOpening } from './useLibraryMatchOpening';
import AnnotationSearch, { type AnnotationHit } from './AnnotationSearch';
import { useI18n } from './shared/i18n/i18n';
import { usePlayModeText } from './play-mode-messages';
import { useWorkspaceText } from './shared/i18n/workspace-messages';
import {
  databaseDisplayName,
  useLibraryText,
  type LibraryMessageKey,
} from './library-messages';
import {
  emptyFilters,
  loadLibrary,
  libraryRowKey,
  isOccurrenceSort,
  librarySorts,
  normalizeLibraryFilters,
  type Database,
  type LibraryMatch,
  type LibrarySort,
  type ActionResponse,
  type LibraryFilters as Filters,
} from './library-api';
import DatabaseSidebar from './DatabaseSidebar';
import LibrarySortHeading from './shared/ui/LibrarySortHeading';
import { useLibraryColumns } from './useLibraryColumns';
import {
  createMatchSummaryFormatter,
  libraryAnalysisStatus,
  canAnalyzeMatch,
  isUnfinishedCompetition,
  canShowMatchPr,
} from './library-match-summary';
import {
  LIBRARY_COLUMNS,
  OCCURRENCE_COLUMNS,
  type LibraryColumn,
} from './library-columns';
import { type LibraryDataColumn } from './library-column-preferences';
import LibraryColumnPicker from './shared/ui/LibraryColumnPicker';
import LibraryImport from './LibraryImport';
import LibraryFilters, {
  LibraryFilterChips,
  type LibraryFilterKey,
} from './LibraryFilters';
import LibraryDialogs, { type LibraryDialogState } from './LibraryDialogs';
import DuplicateMatchFinder from './DuplicateMatchFinder';
import './library.css';
import { prDigits } from './shared/format/pr-format';

export interface PositionSearchRequest {
  positionId: string;
  token: number;
}

type SearchPanel = 'filters' | 'notes' | 'position' | 'duplicates';
const defaultLibraryFilters = emptyFilters();

const resultKey: Record<string, LibraryMessageKey> = {
  copied: 'copied',
  moved: 'moved',
  deleted: 'deleted',
  tagged: 'tagged',
  duplicate: 'duplicate',
  same_database: 'sameDatabase',
  not_found: 'notFound',
  busy: 'busy',
  wrong_database: 'notFound',
  duplicate_request: 'duplicate',
};
export default function MatchDatabase({
  revision,
  analysisRevision,
  annotationRevision,
  currentPosition,
  requestedPositionSearch,
  selectedMatchId,
  databaseId,
  onDatabase,
  onOpen,
  bulkAnalysisProgress,
  onAnalyzeMatches,
  onPosition,
  onAnnotation,
  onImport,
  onImportFolder,
  uploading,
  onOpenPlay,
  onChanged,
  visible,
}: {
  revision: number;
  analysisRevision?: string;
  visible: boolean;
  annotationRevision: number;
  currentPosition?: Position;
  requestedPositionSearch?: PositionSearchRequest;
  selectedMatchId?: string;
  databaseId: string;
  onDatabase: (id: string) => void;
  onOpen: (match: Match, gameNo?: number) => void;
  bulkAnalysisProgress?: {
    total: number;
    completed: number;
    failed: number;
    missing: number;
    loading: boolean;
  };
  onAnalyzeMatches?: (
    matches: LibraryMatch[],
    unavailableCount: number,
  ) => void;
  onPosition: (p: Position) => void;
  onAnnotation: (hit: AnnotationHit) => void;
  onImport: (databaseId: string) => void;
  onImportFolder: (databaseId: string) => void;
  uploading: boolean;
  onOpenPlay?: (id: string, resume?: boolean) => void;
  onChanged: (deletedIds?: string[], validateSelection?: boolean) => void;
}) {
  const { t, locale, formatNumber, formatError } = useI18n(),
    w = useWorkspaceText(),
    l = useLibraryText();
  const modeText = usePlayModeText();
  const columnNames: Record<LibraryDataColumn, string> = {
    id: 'ID',
    white: l('white'),
    white_pr: `${l('white')} · PR`,
    black: l('black'),
    black_pr: `${l('black')} · PR`,
    play_mode: modeText('mode'),
    match_length: l('length'),
    score: l('result'),
    played_at: l('played'),
    created_at: l('imported'),
    tournament: l('tournament'),
    position_count: w('positions'),
    analysis_status: l('analysis'),
    tags: l('tags'),
    comment: l('comment'),
  };
  const showColumn = (id: LibraryColumn) =>
    columns.activeColumns.some((column) => column === id);
  const [databases, setDatabases] = useState<Database[]>([]),
    [localRevision, setRevision] = useState(0),
    [duplicateRefresh, setDuplicateRefresh] = useState(0),
    [searchPanel, setSearchPanel] = useState<SearchPanel>(),
    [databaseError, setDatabaseError] = useState<unknown>(),
    [selectedRows, setSelectedRows] = useState<LibraryMatch[]>([]),
    [dialog, setDialog] = useState<LibraryDialogState>(),
    [result, setResult] = useState<ActionResponse>();
  const legacy = useWorkspaceDefaults();
  const [filters, setAppliedFilters] = useWorkspaceSession(
    'library.filters',
    legacy?.library.filters,
    defaultLibraryFilters,
    (value) => sessionFields(value, defaultLibraryFilters),
  );
  const columns = useLibraryColumns(visible, !!filters.position_id);
  const [filterInput, setFilterInput] = useState<Filters>();
  const draftFilters = filterInput ?? filters;
  const filterDraft = useRef(draftFilters);
  const filterToggle = useRef<HTMLButtonElement>(null);
  const notesToggle = useRef<HTMLButtonElement>(null);
  const positionToggle = useRef<HTMLButtonElement>(null);
  const duplicatesToggle = useRef<HTMLButtonElement>(null);
  const searchId = useId();
  const selectionHintId = `${searchId}-selection`;
  const searchPanels = [
    {
      id: 'filters',
      label: l('filters'),
      icon: 'filter',
      trigger: filterToggle,
    },
    { id: 'notes', label: w('notes'), icon: 'notes', trigger: notesToggle },
    {
      id: 'position',
      label: l('positionSearch'),
      icon: 'similar',
      trigger: positionToggle,
    },
    {
      id: 'duplicates',
      label: l('duplicates'),
      icon: 'duplicates',
      trigger: duplicatesToggle,
    },
  ] as const;
  filterDraft.current = draftFilters;
  const setFilters = (next: SetStateAction<Filters>) => {
    const value = typeof next === 'function' ? next(filterDraft.current) : next;
    filterDraft.current = value;
    setFilterInput(value);
    setAppliedFilters(normalizeLibraryFilters(value));
  };
  function removeFilter(key: LibraryFilterKey) {
    (key === 'position_id' ? positionToggle : filterToggle).current?.focus();
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
  // Keep the remembered result sort, but use a match sort while its context
  // columns are absent. The API requires a position filter for occurrence keys.
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
  const resultNames = useRef<Record<string, string>>({});
  const selected = new Set(selectedRows.map((row) => row.id));
  const [exact, setExact] = useState(''),
    [positionInputError, setPositionInputError] = useState(false);
  const consumedPositionSearch = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (
      !requestedPositionSearch ||
      consumedPositionSearch.current === requestedPositionSearch.token
    )
      return;
    consumedPositionSearch.current = requestedPositionSearch.token;
    // An explicit new search also resets transient match actions when its
    // filter values are identical to the previous search.
    setSelectedRows([]);
    setResult(undefined);
    setExact(requestedPositionSearch.positionId);
    setPositionInputError(false);
    setFilters({
      ...emptyFilters(),
      position_id: requestedPositionSearch.positionId,
    });
    setSearchPanel('position');
  }, [requestedPositionSearch]);
  const matchScope = JSON.stringify([
    databaseId,
    filters,
    sort,
    direction,
    requestedPositionSearch?.token,
  ]);
  const matchOpening = useLibraryMatchOpening({
    scope: matchScope,
    positionId: filters.position_id.trim(),
    visible,
    onOpen,
    onPosition,
  });
  useEffect(() => {
    setExact(filters.position_id);
    setPositionInputError(false);
  }, [filters.position_id]);
  const databaseCallback = useRef(onDatabase);
  databaseCallback.current = onDatabase;
  const databaseCurrent = useRef(databaseId);
  databaseCurrent.current = databaseId;
  useEffect(() => {
    const abort = new AbortController();
    void request<Database[]>('/databases', { signal: abort.signal })
      .then((value) => {
        if (abort.signal.aborted) return;
        setDatabases(value);
        setDatabaseError(undefined);
        if (
          databaseCurrent.current &&
          !value.some((d) => d.id === databaseCurrent.current)
        )
          databaseCallback.current('');
      })
      .catch((e) => {
        if (!abort.signal.aborted) setDatabaseError(e);
      });
    return () => abort.abort();
  }, [revision, localRevision]);
  useEffect(() => {
    setSelectedRows([]);
    setResult(undefined);
  }, [matchScope]);
  const loadMatches = useCallback(
    (signal: AbortSignal, page: number) =>
      loadLibrary(databaseId, filters, sort, direction, signal, page),
    [databaseId, filters, sort, direction],
  );
  const {
    items: matches,
    resultScope: matchResultScope,
    total,
    page,
    lastPage,
    setPage,
    loading,
    error: searchError,
  } = useSearchPages({
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
  const error = searchError || databaseError;
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
  function changeDatabase(id: string) {
    onDatabase(id);
    setSelectedRows([]);
  }
  function changeSelection(rows: readonly LibraryMatch[], checked: boolean) {
    const next = new Map(selectedRows.map((row) => [row.id, row]));
    for (const row of rows) {
      if (!checked) next.delete(row.id);
      else if (next.size < 100) next.set(row.id, row);
    }
    setSelectedRows([...next.values()]);
  }
  function actionResult(response: ActionResponse) {
    setResult(response);
    resultNames.current = Object.fromEntries(
      selectedRows.map((m) => [m.id, m.display_name]),
    );
    const deleted = response.results
      .filter((r) => r.status === 'deleted')
      .map((r) => r.match_id);
    const failed = new Set(
      response.results
        .filter(
          (row) =>
            ![
              'copied',
              'moved',
              'deleted',
              'tagged',
              'same_database',
              'duplicate',
            ].includes(row.status),
        )
        .map((row) => row.match_id),
    );
    setSelectedRows((rows) => rows.filter((row) => failed.has(row.id)));
    setRevision((n) => n + 1);
    setDuplicateRefresh((n) => n + 1);
    onChanged(deleted);
  }
  function changed(id?: string, validateSelection = false) {
    if (id !== undefined) changeDatabase(id);
    setRevision((n) => n + 1);
    setDuplicateRefresh((n) => n + 1);
    onChanged([], validateSelection);
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
  const heading = (
    key: LibrarySort & LibraryColumn,
    label: string,
    name?: string,
  ) =>
    showColumn(key) ? (
      <LibrarySortHeading
        fields={[{ key, label, name }]}
        sort={sort}
        direction={direction}
        onSort={sortBy}
        column={key}
        resizeHandle={columns.handle(key, name ?? label)}
      />
    ) : null;
  function openMatchRow(match: LibraryMatch) {
    if (matchResultScope !== matchScope) return;
    if (match.play_paused || isUnfinishedCompetition(match)) {
      if (match.play_session_id) onOpenPlay?.(match.play_session_id);
      return;
    }
    void matchOpening.open(match);
  }
  const rowOpeningDisabled = (match: LibraryMatch) =>
    matchResultScope !== matchScope ||
    matchOpening.openingId === libraryRowKey(match);
  function isCurrentRow(match: LibraryMatch) {
    return match.occurrence
      ? match.occurrence.id === currentPosition?.id
      : match.id === selectedMatchId;
  }
  function matchName(match: LibraryMatch, player: 'white' | 'black') {
    return (
      <button
        className={`match-card ${isCurrentRow(match) ? 'active' : ''}`}
        disabled={rowOpeningDisabled(match)}
        aria-busy={matchOpening.openingId === libraryRowKey(match)}
        onClick={(event) => {
          event.stopPropagation();
          openMatchRow(match);
        }}
      >
        <span className="library-match-player">{match[player]}</span>
        <span className="sr-only">
          {t('common.versus')} {match[player === 'white' ? 'black' : 'white']}
        </span>
        {match.display_name.trim() && match.display_name !== match.filename && (
          <span className="library-match-title">· {match.display_name}</span>
        )}
      </button>
    );
  }
  /** PRs from a source whose import resolved irregularities. */
  function importNotesMarker(match: LibraryMatch) {
    if (!match.import_warning_count) return null;
    const label = l('importNotesCount', {
      count: match.import_warning_count,
    });
    return (
      <span
        className="import-notes-marker"
        role="img"
        aria-label={label}
        title={label}
      >
        <ActionIcon name="warning" size={13} />
      </span>
    );
  }
  function rowKey(
    e: KeyboardEvent<HTMLTableRowElement>,
    match: LibraryMatch,
    index: number,
  ) {
    if (e.target !== e.currentTarget) return;
    if (e.key === 'Enter') {
      e.preventDefault();
      openMatchRow(match);
    }
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      const next = Math.min(
        matches.length - 1,
        Math.max(0, index + (e.key === 'ArrowDown' ? 1 : -1)),
      );
      document
        .getElementById(`library-row-${libraryRowKey(matches[next])}`)
        ?.focus();
    }
  }
  useEffect(() => {
    if (
      !visible ||
      !matches.some((m) =>
        ['queued', 'running'].includes(m.analysis_status ?? ''),
      )
    )
      return;
    const timer = setInterval(() => setRevision((n) => n + 1), 4000);
    return () => clearInterval(timer);
  }, [visible, matches]);
  const active = databases.find((d) => d.id === databaseId);
  const hasFilters = Object.entries(filters).some(
    ([key, value]) => key !== 'date_field' && value.trim(),
  );
  const allOnPageSelected =
    matches.length > 0 && matches.every((match) => selected.has(match.id));
  const someOnPageSelected = matches.some((match) => selected.has(match.id));
  const visibleRows = new Map(
    matches.map((match) => [libraryRowKey(match), match]),
  );
  const selection = selectedRows.map(
    (match) => visibleRows.get(libraryRowKey(match)) ?? match,
  );
  const hasSelection = selected.size > 0;
  const pausedSelected = selection.some((match) => match.play_paused);
  const singleSelected = selection.length === 1 ? selection[0] : undefined;
  const resumeId =
    singleSelected &&
    (singleSelected.play_paused || singleSelected.play_phase !== 'match_over')
      ? singleSelected.play_session_id
      : undefined;
  const selectionIsCurrent = matchResultScope === matchScope;
  const analyzableMatches = selection.filter(canAnalyzeMatch);
  const pr = (value: number | null) =>
    value == null ? '—' : formatNumber(value, prDigits);
  const date = (value: string | null) =>
    value
      ? new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(
          new Date(value.length === 10 ? `${value}T12:00:00` : value),
        )
      : '—';
  const state = (m: LibraryMatch) =>
    libraryAnalysisStatus(m.analysis_status, locale);
  const summaries = useMemo(() => {
    const summary = createMatchSummaryFormatter(locale);
    return new Map(matches.map((match) => [match.id, summary(match)]));
  }, [matches, locale]);
  return (
    <section className="match-database" aria-label={w('database')}>
      <div className="database-heading">
        <div>
          <h1>{w('databaseTitle')}</h1>
          <p>{w('databaseDescription')}</p>
        </div>
        <LibraryImport
          databases={databases}
          databaseId={databaseId}
          uploading={uploading}
          onImport={onImport}
          onImportFolder={onImportFolder}
        />
      </div>
      <div className="library-layout">
        <DatabaseSidebar
          databases={databases}
          selected={databaseId}
          onSelect={changeDatabase}
          onNew={() => setDialog({ kind: 'database' })}
          onEdit={(database) => setDialog({ kind: 'database', database })}
          onDelete={(database) =>
            setDialog({ kind: 'deleteDatabase', database })
          }
        />
        <div className="library-main">
          <div className="library-search-toolbar">
            <label className="search-box">
              <ActionIcon name="search" size={16} />
              <input
                aria-label={t('app.searchMatches')}
                placeholder={t('app.searchPlaceholder')}
                value={draftFilters.q}
                onChange={(e) =>
                  setFilters((f) => ({ ...f, q: e.target.value }))
                }
              />
            </label>
            {searchPanels.map(({ id, label, icon, trigger }) => (
              <ActionButton
                key={id}
                ref={trigger}
                icon={icon}
                label={label}
                type="button"
                onClick={() =>
                  setSearchPanel((current) => (current === id ? undefined : id))
                }
                aria-controls={`${searchId}-${id}`}
                aria-expanded={searchPanel === id}
              />
            ))}
          </div>
          <div
            onKeyDown={(event) => {
              if (
                event.key !== 'Escape' ||
                !searchPanel ||
                event.defaultPrevented
              )
                return;
              event.preventDefault();
              searchPanels
                .find(({ id }) => id === searchPanel)
                ?.trigger.current?.focus();
              setSearchPanel(undefined);
            }}
          >
            <LibraryFilters
              filters={draftFilters}
              onChange={setFilters}
              onClear={() => setFilters(emptyFilters())}
              panelId={`${searchId}-filters`}
              open={searchPanel === 'filters'}
            />
            <AnnotationSearch
              revision={annotationRevision}
              databaseId={databaseId}
              onOpen={onAnnotation}
              panelId={`${searchId}-notes`}
              open={searchPanel === 'notes'}
            />
            <div
              className="position-search"
              id={`${searchId}-position`}
              hidden={searchPanel !== 'position'}
            >
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  searchPosition();
                }}
              >
                <input
                  aria-label={t('app.positionId')}
                  placeholder={t('app.positionPlaceholder')}
                  value={exact}
                  aria-invalid={positionInputError || undefined}
                  onChange={(e) => setPositionQuery(e.target.value)}
                />
                <ActionButton
                  icon="search"
                  label={t('app.exactSearch')}
                  type="submit"
                  disabled={loading || !exact.trim()}
                />
                {currentPosition && (
                  <ActionButton
                    icon="view"
                    label={t('app.usePosition')}
                    type="button"
                    onClick={() =>
                      setPositionQuery(currentPosition.position_id)
                    }
                  />
                )}
              </form>
              {positionInputError && (
                <p role="alert">{l('invalidPositionId')}</p>
              )}
            </div>
            <DuplicateMatchFinder
              open={visible && searchPanel === 'duplicates'}
              panelId={`${searchId}-duplicates`}
              databaseId={databaseId}
              databases={databases}
              refreshToken={duplicateRefresh + revision}
              selected={selected}
              onSelect={changeSelection}
              onOpen={openMatchRow}
              openingDisabled={rowOpeningDisabled}
            />
          </div>
          <LibraryFilterChips filters={filters} onRemove={removeFilter} />
          {!!error && (
            <p className="error" role="alert">
              {formatError(error)}
            </p>
          )}
          {!!matchOpening.error && (
            <p className="error" role="alert">
              {formatError(matchOpening.error)}
            </p>
          )}
          <div className="library-table-toolbar">
            <div className="database-count">
              {loading ? t('app.searching') : `${formatNumber(total)}`}{' '}
              {filters.position_id ? l('occurrences') : t('app.matches')} ·{' '}
              {active ? databaseDisplayName(active, l) : l('allDatabases')}
            </div>
            {!!bulkAnalysisProgress?.total && (
              <p className="library-bulk-analysis-progress" role="status">
                {bulkAnalysisProgress.loading ? (
                  l('loading')
                ) : (
                  <>
                    {l('bulkAnalysisProgress', {
                      completed: bulkAnalysisProgress.completed,
                      total: bulkAnalysisProgress.total,
                    })}
                    {bulkAnalysisProgress.failed > 0 &&
                      ` · ${l('failed')} (${formatNumber(bulkAnalysisProgress.failed)})`}
                    {bulkAnalysisProgress.missing > 0 &&
                      ` · ${l('notFound')} (${formatNumber(bulkAnalysisProgress.missing)})`}
                  </>
                )}
              </p>
            )}
            {!showColumn(sort as LibraryColumn) && (
              <span className="library-hidden-sort">
                {l('sortedBy', {
                  column: columnNames[sort as LibraryDataColumn] ?? sort,
                  direction: l(
                    direction === 'asc' ? 'ascending' : 'descending',
                  ),
                })}
              </span>
            )}
            <LibraryColumnPicker
              options={columns.order.map((id) => ({
                id,
                label: columnNames[id],
              }))}
              shown={columns.shown}
              onToggle={columns.toggle}
              onAll={columns.showAll}
              onReset={columns.resetSelection}
              onMove={columns.move}
            />
            <ActionButton
              icon="reset"
              label={l('resetColumnWidths')}
              compact
              type="button"
              className="library-width-reset"
              onClick={columns.reset}
              disabled={!columns.custom}
            />
          </div>
          {filters.position_id && matches.length > 0 && (
            <p id={selectionHintId} className="library-occurrence-selection">
              {l('occurrenceSelection')}
            </p>
          )}
          <div
            className="library-batch-bar"
            data-has-selection={hasSelection}
            role="group"
            aria-label={l('actions')}
          >
            <strong role="status">
              {l('selectedMatches', { count: selected.size })}
            </strong>
            <ActionButton
              type="button"
              icon="open"
              label={w('openMatch')}
              compact
              disabled={
                !singleSelected ||
                !!resumeId ||
                singleSelected.play_paused ||
                rowOpeningDisabled(singleSelected)
              }
              onClick={() => {
                if (singleSelected) openMatchRow(singleSelected);
              }}
            />
            <ActionButton
              type="button"
              icon="resume"
              label={l('resumePlay')}
              compact
              disabled={!selectionIsCurrent || !resumeId || !onOpenPlay}
              onClick={() => {
                if (resumeId) onOpenPlay?.(resumeId, true);
              }}
            />
            <ActionButton
              type="button"
              icon="analysis"
              label={`${l('analyzeSelection')} …`}
              aria-haspopup="dialog"
              compact
              disabled={
                !selectionIsCurrent ||
                !onAnalyzeMatches ||
                !analyzableMatches.length
              }
              onClick={(event) => {
                event.currentTarget.focus();
                onAnalyzeMatches?.(
                  analyzableMatches,
                  selection.length - analyzableMatches.length,
                );
              }}
            />
            <ActionButton
              type="button"
              icon="edit"
              label={l('editMatch')}
              aria-haspopup="dialog"
              compact
              disabled={!selectionIsCurrent || !singleSelected}
              onClick={(event) => {
                event.currentTarget.focus();
                if (singleSelected)
                  setDialog({ kind: 'metadata', match: singleSelected });
              }}
            />
            {(['copy', 'move', 'tags', 'export', 'delete'] as const).map(
              (action) => (
                <ActionButton
                  icon={action}
                  label={l(action)}
                  compact
                  key={action}
                  disabled={
                    !hasSelection ||
                    !selectionIsCurrent ||
                    (action === 'export' && selected.size > 50) ||
                    (pausedSelected &&
                      ['copy', 'move', 'delete'].includes(action))
                  }
                  onClick={() =>
                    setDialog({
                      kind: 'batch',
                      action,
                      ids: [...selected],
                      source: databaseId,
                    })
                  }
                />
              ),
            )}
            <ActionButton
              icon="clear"
              label={l('clearSelection')}
              compact
              disabled={!hasSelection}
              onClick={() => setSelectedRows([])}
              aria-label={l('clearSelection')}
            />
          </div>
          {hasSelection && <p className="fineprint">{l('batchLimit')}</p>}
          {result && (
            <div className="library-action-result" role="status">
              {Object.entries(
                result.results.reduce<Record<string, number>>(
                  (totals, r) => ({
                    ...totals,
                    [r.status]: (totals[r.status] ?? 0) + 1,
                  }),
                  {},
                ),
              ).map(([status, count]) => (
                <span key={status}>
                  {l(resultKey[status] ?? 'error')}: {count}{' '}
                </span>
              ))}
              {result.results.length > 0 && (
                <details>
                  <summary>{w('details')}</summary>
                  <ul>
                    {result.results.map((r, i) => (
                      <li key={`${r.match_id}:${i}`}>
                        <strong>
                          {resultNames.current[r.match_id] ??
                            r.match_id.slice(0, 8)}
                        </strong>
                        : {l(resultKey[r.status] ?? 'error')}
                        {r.error ? ` · ${r.error}` : ''}
                        {(r.created_id || r.existing_id) && (
                          <small title={r.created_id ?? r.existing_id ?? ''}>
                            {' '}
                            · {l('target')}:{' '}
                            {(r.created_id ?? r.existing_id)?.slice(0, 8)}
                          </small>
                        )}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
          )}
          <div className="library-content">
            <div>
              <div className="match-list database-table-wrap">
                <table
                  ref={columns.tableRef}
                  className={`database-table${columns.dragging ? ' library-table-resizing' : ''}`}
                  style={
                    columns.tableStyle ?? { minWidth: columns.minimumWidth }
                  }
                >
                  <colgroup>
                    {filters.position_id &&
                      OCCURRENCE_COLUMNS.map((column) => (
                        <col
                          key={column.id}
                          data-column={column.id}
                          className={column.className}
                          style={{
                            width:
                              columns.widths?.[column.id] ??
                              column.defaultWidth,
                          }}
                        />
                      ))}
                    {columns.tableColumns.map(({ id, className, width }) => (
                      <col
                        key={id}
                        data-column={id}
                        className={className}
                        style={{ width }}
                      />
                    ))}
                  </colgroup>
                  <thead>
                    <OrderedLibraryRow
                      columns={columns.activeColumns}
                      leadingCells={
                        filters.position_id && (
                          <>
                            <LibrarySortHeading
                              fields={[
                                {
                                  key: 'occurrence_database',
                                  label: l('database'),
                                },
                              ]}
                              sort={sort}
                              direction={direction}
                              onSort={sortBy}
                              column="occurrence_database"
                              resizeHandle={columns.handle(
                                'occurrence_database',
                                l('database'),
                              )}
                            />
                            <LibrarySortHeading
                              fields={[
                                {
                                  key: 'occurrence_game',
                                  label: l('occurrenceGame'),
                                },
                              ]}
                              sort={sort}
                              direction={direction}
                              onSort={sortBy}
                              column="occurrence_game"
                              resizeHandle={columns.handle(
                                'occurrence_game',
                                l('occurrenceGame'),
                              )}
                            />
                            <LibrarySortHeading
                              fields={[
                                {
                                  key: 'occurrence_move',
                                  label: l('occurrenceMove'),
                                },
                              ]}
                              sort={sort}
                              direction={direction}
                              onSort={sortBy}
                              column="occurrence_move"
                              resizeHandle={columns.handle(
                                'occurrence_move',
                                l('occurrenceMove'),
                              )}
                            />
                          </>
                        )
                      }
                    >
                      <th scope="col" data-column="select">
                        <input
                          type="checkbox"
                          aria-label={l('selectAll')}
                          aria-describedby={
                            filters.position_id && matches.length
                              ? selectionHintId
                              : undefined
                          }
                          checked={allOnPageSelected}
                          ref={(checkbox) => {
                            if (checkbox)
                              checkbox.indeterminate =
                                someOnPageSelected && !allOnPageSelected;
                          }}
                          disabled={loading || !matches.length}
                          onChange={(e) =>
                            changeSelection(matches, e.target.checked)
                          }
                        />
                        {columns.handle('select', l('selection'))}
                      </th>
                      {heading('id', 'ID')}
                      {heading('white', l('white'))}
                      {heading('white_pr', 'PR', `${l('white')} · PR`)}
                      {heading('black', l('black'))}
                      {heading('black_pr', 'PR', `${l('black')} · PR`)}
                      {showColumn('play_mode') && (
                        <th scope="col" data-column="play_mode">
                          <span className="library-sort-label">
                            {modeText('mode')}
                          </span>
                          {columns.handle('play_mode', modeText('mode'))}
                        </th>
                      )}
                      {heading('match_length', l('length'))}
                      {heading('score', l('result'))}
                      {heading('played_at', l('played'))}
                      {heading('created_at', l('imported'))}
                      {heading('tournament', l('tournament'))}
                      {heading('position_count', w('positions'))}
                      {heading('analysis_status', l('analysis'))}
                      {heading('tags', l('tags'))}
                      {heading('comment', l('comment'))}
                    </OrderedLibraryRow>
                  </thead>
                  <tbody>
                    {matches.map((m, index) => (
                      <OrderedLibraryRow
                        columns={columns.activeColumns}
                        key={libraryRowKey(m)}
                        id={`library-row-${libraryRowKey(m)}`}
                        data-match-id={m.id}
                        data-position-id={m.occurrence?.id}
                        tooltip={summaries.get(m.id)}
                        tabIndex={0}
                        className={isCurrentRow(m) ? 'selected' : undefined}
                        onClick={(event) => event.currentTarget.focus()}
                        onDoubleClick={(event) => {
                          if (
                            !(event.target as HTMLElement).closest(
                              'button, a, input',
                            )
                          )
                            openMatchRow(m);
                        }}
                        onKeyDown={(e) => rowKey(e, m, index)}
                        leadingCells={
                          filters.position_id && (
                            <>
                              <td data-column="occurrence_database">
                                {m.occurrence?.database_name ??
                                  databases.find((d) => d.id === m.database_id)
                                    ?.name ??
                                  '—'}
                              </td>
                              <td data-column="occurrence_game">
                                {m.occurrence
                                  ? formatNumber(m.occurrence.game_no)
                                  : '—'}
                              </td>
                              <td data-column="occurrence_move">
                                {m.occurrence ? (
                                  <button
                                    type="button"
                                    className="library-occurrence-link"
                                    aria-label={l('openOccurrence', {
                                      game: formatNumber(m.occurrence.game_no),
                                      move: formatNumber(m.occurrence.move_no),
                                    })}
                                    disabled={rowOpeningDisabled(m)}
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      openMatchRow(m);
                                    }}
                                  >
                                    {formatNumber(m.occurrence.move_no)}
                                  </button>
                                ) : (
                                  '—'
                                )}
                              </td>
                            </>
                          )
                        }
                      >
                        <td onClick={(e) => e.stopPropagation()}>
                          <input
                            type="checkbox"
                            aria-label={l(
                              m.occurrence ? 'selectMatch' : 'select',
                              { name: m.display_name },
                            )}
                            checked={selected.has(m.id)}
                            onChange={(e) =>
                              changeSelection([m], e.target.checked)
                            }
                            disabled={
                              selected.size >= 100 && !selected.has(m.id)
                            }
                          />
                        </td>
                        {showColumn('id') && (
                          <td className="library-id-cell">
                            {formatNumber(m.match_no)}
                          </td>
                        )}
                        {showColumn('white') && (
                          <td>{matchName(m, 'white')}</td>
                        )}
                        {showColumn('white_pr') && (
                          <td className="library-pr-cell">
                            {pr(canShowMatchPr(m) ? m.white_pr : null)}
                            {importNotesMarker(m)}
                          </td>
                        )}
                        {showColumn('black') && (
                          <td>
                            {showColumn('white') ? (
                              <span className="library-player-name">
                                {m.black}
                              </span>
                            ) : (
                              matchName(m, 'black')
                            )}
                          </td>
                        )}
                        {showColumn('black_pr') && (
                          <td className="library-pr-cell">
                            {pr(canShowMatchPr(m) ? m.black_pr : null)}
                            {importNotesMarker(m)}
                          </td>
                        )}
                        {showColumn('play_mode') && (
                          <td
                            className="library-play-mode-cell"
                            data-column="play_mode"
                          >
                            {m.play_mode ? (
                              <span className="library-play-mode">
                                {modeText(m.play_mode)} ·{' '}
                                {modeText(
                                  m.play_assisted ? 'assisted' : 'independent',
                                )}
                              </span>
                            ) : (
                              '—'
                            )}
                          </td>
                        )}
                        {showColumn('match_length') && (
                          <td>{m.match_length || 'M'}</td>
                        )}
                        {showColumn('score') && (
                          <td>
                            {m.white_score != null && m.black_score != null
                              ? `${m.white_score}–${m.black_score}`
                              : 'n/a'}
                          </td>
                        )}
                        {showColumn('played_at') && (
                          <td>{date(m.played_on)}</td>
                        )}
                        {showColumn('created_at') && (
                          <td>{date(m.created_at)}</td>
                        )}
                        {showColumn('tournament') && (
                          <td>{m.tournament ?? '—'}</td>
                        )}
                        {showColumn('position_count') && (
                          <td className="library-match-positions">
                            {formatNumber(m.position_count)}
                          </td>
                        )}
                        {showColumn('analysis_status') && <td>{state(m)}</td>}
                        {showColumn('tags') && (
                          <td className="library-tags-cell library-notes-cell">
                            <div className="library-notes-summary">
                              {m.tags.length > 0 && (
                                <span className="library-note-tags">
                                  {m.tags.map((tag) => (
                                    <span className="tag-chip" key={tag}>
                                      {tag}
                                    </span>
                                  ))}
                                </span>
                              )}
                            </div>
                          </td>
                        )}
                        {showColumn('comment') && (
                          <td className="library-comment-cell">
                            <span className="library-note-comment">
                              {m.comment}
                            </span>
                          </td>
                        )}
                      </OrderedLibraryRow>
                    ))}
                  </tbody>
                </table>
              </div>
              {!matches.length && !loading && !searchError && (
                <div className="library-empty">
                  <p role="status">{l('noMatches')}</p>
                  {hasFilters && (
                    <button
                      type="button"
                      onClick={() => {
                        filterToggle.current?.focus();
                        setFilters(emptyFilters());
                      }}
                    >
                      {l('clearFilters')}
                    </button>
                  )}
                  {databaseId && (
                    <button
                      type="button"
                      onClick={() => {
                        filterToggle.current?.focus();
                        changeDatabase('');
                      }}
                    >
                      {l('searchAllDatabases')}
                    </button>
                  )}
                </div>
              )}
              <SearchPagination
                page={page}
                lastPage={lastPage}
                busy={loading}
                onPageChange={setPage}
              />
              <p className="fineprint library-pr-help">{l('prHelp')}</p>
            </div>
          </div>
        </div>
      </div>
      {dialog && (
        <LibraryDialogs
          key={JSON.stringify(dialog)}
          state={dialog}
          databases={databases}
          onClose={() => setDialog(undefined)}
          onChanged={changed}
          onAction={actionResult}
        />
      )}
    </section>
  );
}

function OrderedLibraryRow({
  columns,
  children,
  leadingCells,
  tooltip,
  ...props
}: HTMLAttributes<HTMLTableRowElement> & {
  columns: readonly LibraryColumn[];
  leadingCells?: ReactNode;
  tooltip?: string;
}) {
  const natural = LIBRARY_COLUMNS.map((c) => c.id).filter((id) =>
    columns.includes(id),
  );
  const cells = Children.toArray(children);
  return (
    <Tooltip as="tr" text={tooltip} {...props}>
      {leadingCells}
      {columns.map((id) => cells[natural.indexOf(id)])}
    </Tooltip>
  );
}
