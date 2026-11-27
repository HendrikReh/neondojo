import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import {
  sessionFields,
  useWorkspaceSession,
} from '../../shared/hooks/useWorkspaceSession';
import ActionButton from '../../shared/ui/ActionButton';
import ActionIcon from '../../shared/ui/action-icons';
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type SetStateAction,
} from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import type { Match, Position } from '../../shared/http/api';
import SearchPagination from '../../shared/ui/SearchPagination';
import { useSearchPages } from '../../shared/hooks/useSearchPages';
import { useLibraryMatchOpening } from './useLibraryMatchOpening';
import { useLibraryJobRefresh } from './useLibraryJobRefresh';
import AnnotationSearch, {
  type AnnotationHit,
} from '../analysis/notes/AnnotationSearch';
import { useI18n } from '../../shared/i18n/i18n';
import { usePlayModeText } from '../play/play-mode-messages';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { databaseDisplayName, useLibraryText } from './library-messages';
import {
  emptyFilters,
  loadLibrary,
  libraryRowKey,
  isOccurrenceSort,
  librarySorts,
  normalizeLibraryFilters,
  type LibraryMatch,
  type LibrarySort,
  type ActionResponse,
  type LibraryFilters as Filters,
} from './library-api';
import { useDatabases } from './useDatabases';
import DatabaseSidebar from './DatabaseSidebar';
import { useLibraryColumns } from './useLibraryColumns';
import { isUnfinishedCompetition } from './library-match-summary';
import type { LibraryColumn } from './library-columns';
import { type LibraryDataColumn } from './library-column-preferences';
import LibraryColumnPicker from '../../shared/ui/LibraryColumnPicker';
import LibraryImport from './import/LibraryImport';
import LibraryFilters, {
  LibraryFilterChips,
  type LibraryFilterKey,
} from './LibraryFilters';
import LibraryDialogs, { type LibraryDialogState } from './LibraryDialogs';
import LibraryActionResult from './LibraryActionResult';
import LibraryBatchBar from './LibraryBatchBar';
import LibraryTable from './LibraryTable';
import DuplicateMatchFinder from './DuplicateMatchFinder';
import './library.css';

export interface PositionSearchRequest {
  positionId: string;
  token: number;
}

type SearchPanel = 'filters' | 'notes' | 'position' | 'duplicates';
const defaultLibraryFilters = emptyFilters();

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
  const { t, formatNumber, formatError } = useI18n(),
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
  const [localRevision, setRevision] = useState(0),
    [duplicateRefresh, setDuplicateRefresh] = useState(0),
    [searchPanel, setSearchPanel] = useState<SearchPanel>(),
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
  const filterDraft = useLatest(draftFilters);
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
  const [resultNames, setResultNames] = useState<Record<string, string>>({});
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
  const { databases, error: databaseError } = useDatabases(
    `${revision}:${localRevision}`,
    {
      onLoad: (list) => {
        if (databaseId && !list.some((d) => d.id === databaseId))
          onDatabase('');
      },
    },
  );
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
    setResultNames(
      Object.fromEntries(selectedRows.map((m) => [m.id, m.display_name])),
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
  const reloadPage = useCallback(() => setRevision((n) => n + 1), []);
  useLibraryJobRefresh(matches, visible, reloadPage);
  const active = databases.find((d) => d.id === databaseId);
  const hasFilters = Object.entries(filters).some(
    ([key, value]) => key !== 'date_field' && value.trim(),
  );
  const visibleRows = new Map(
    matches.map((match) => [libraryRowKey(match), match]),
  );
  const selection = selectedRows.map(
    (match) => visibleRows.get(libraryRowKey(match)) ?? match,
  );
  const selectionIsCurrent = matchResultScope === matchScope;
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
          <LibraryBatchBar
            selection={selection}
            current={selectionIsCurrent}
            openingDisabled={rowOpeningDisabled}
            onOpen={openMatchRow}
            onResume={onOpenPlay && ((id) => onOpenPlay(id, true))}
            onEdit={(match) => setDialog({ kind: 'metadata', match })}
            onAnalyze={onAnalyzeMatches}
            onBatch={(action) =>
              setDialog({
                kind: 'batch',
                action,
                ids: [...selected],
                source: databaseId,
              })
            }
            onClear={() => setSelectedRows([])}
          />
          {result && (
            <LibraryActionResult result={result} names={resultNames} />
          )}
          <div className="library-content">
            <div>
              <LibraryTable
                matches={matches}
                databases={databases}
                occurrences={!!filters.position_id}
                columns={columns}
                sort={sort}
                direction={direction}
                onSort={sortBy}
                loading={loading}
                selected={selected}
                onSelect={changeSelection}
                selectionHintId={selectionHintId}
                isCurrent={isCurrentRow}
                openingId={matchOpening.openingId}
                openingDisabled={rowOpeningDisabled}
                onOpen={openMatchRow}
              />
              {!matches.length && !loading && !searchError && (
                <div className="library-empty">
                  <p role="status">{l('noMatches')}</p>
                  {hasFilters && (
                    <ActionButton
                      type="button"
                      icon="clear"
                      label={l('clearFilters')}
                      onClick={() => {
                        filterToggle.current?.focus();
                        setFilters(emptyFilters());
                      }}
                    />
                  )}
                  {databaseId && (
                    <ActionButton
                      type="button"
                      icon="search"
                      label={l('searchAllDatabases')}
                      onClick={() => {
                        filterToggle.current?.focus();
                        changeDatabase('');
                      }}
                    />
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
