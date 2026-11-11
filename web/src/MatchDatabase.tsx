import Tooltip from './Tooltip';
import { useOptionalConfiguration } from './ConfigurationProvider';
import { Children, type HTMLAttributes, type ReactNode } from 'react';
import { useProfileValue } from './useProfileValue';
import ActionButton, { ActionLink } from './ActionButton';
import { Search } from 'lucide-react';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type SetStateAction,
} from 'react';
import { request, type Match, type Position } from './api';
import { useCursorSearch } from './useCursorSearch';
import { useLibraryMatchOpening } from './useLibraryMatchOpening';
import AnnotationSearch, { type AnnotationHit } from './AnnotationSearch';
import { useI18n } from './i18n';
import { usePlayModeText } from './play-mode-messages';
import { useWorkspaceText } from './workspace-messages';
import { useLibraryText, type LibraryMessageKey } from './library-messages';
import {
  emptyFilters,
  loadLibrary,
  libraryRowKey,
  isOccurrenceSort,
  type Database,
  type LibraryMatch,
  type LibrarySort,
  type ActionResponse,
  type LibraryFilters as Filters,
} from './library-api';
import DatabaseSidebar from './DatabaseSidebar';
import LibrarySortHeading from './LibrarySortHeading';
import { useLibraryColumns } from './useLibraryColumns';
import {
  createMatchSummaryFormatter,
  libraryAnalysisStatus,
  isUnfinishedCompetition,
  canShowMatchPr,
} from './library-match-summary';
import {
  LIBRARY_COLUMNS,
  OCCURRENCE_COLUMNS,
  type LibraryColumn,
} from './library-columns';
import { type LibraryDataColumn } from './library-column-preferences';
import LibraryColumnPicker from './LibraryColumnPicker';
import LibraryFilters from './LibraryFilters';
import LibraryDialogs, { type LibraryDialogState } from './LibraryDialogs';
import './library.css';

export interface PositionSearchRequest {
  positionId: string;
  token: number;
}

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
  annotationRevision,
  currentPosition,
  requestedPositionSearch,
  selectedMatchId,
  databaseId,
  onDatabase,
  onOpen,
  onPosition,
  onAnnotation,
  onImport,
  onResumePlay,
  onChanged,
  visible,
}: {
  revision: number;
  visible: boolean;
  annotationRevision: number;
  currentPosition?: Position;
  requestedPositionSearch?: PositionSearchRequest;
  selectedMatchId?: string;
  databaseId: string;
  onDatabase: (id: string) => void;
  onOpen: (match: Match, gameNo?: number) => void;
  onPosition: (p: Position) => void;
  onAnnotation: (hit: AnnotationHit) => void;
  onImport: () => void;
  onResumePlay?: (id: string) => void;
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
  const showColumn = (id: LibraryColumn) => columns.activeColumns.includes(id);
  const [databases, setDatabases] = useState<Database[]>([]),
    [localRevision, setRevision] = useState(0),
    [expanded, setExpanded] = useState(false),
    [databaseError, setDatabaseError] = useState<unknown>(),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [dialog, setDialog] = useState<LibraryDialogState>(),
    [result, setResult] = useState<ActionResponse>();
  const profile = useOptionalConfiguration();
  const [filters, setAppliedFilters] = useProfileValue<Filters>(
    (c) => c.library.filters,
    (c, filters) => ({ ...c, library: { ...c.library, filters } }),
    emptyFilters,
  );
  const columns = useLibraryColumns(visible, !!filters.position_id);
  const compactActions = !showColumn('white') && !showColumn('black');
  const [draftFilters, setDraftFilters] = useState<Filters>(filters);
  const filterDraft = useRef(draftFilters);
  filterDraft.current = draftFilters;
  // Preserve editable whitespace while canonical filters drive persisted queries.
  // Only a profile replacement rebases a user's still-editable filter inputs.
  useEffect(() => {
    if (!profile?.current) return;
    filterDraft.current = profile.current.library.filters;
    setDraftFilters(profile.current.library.filters);
  }, [profile?.generation, profile?.ready]);
  const setFilters = (next: SetStateAction<Filters>) => {
    const value = typeof next === 'function' ? next(filterDraft.current) : next;
    filterDraft.current = value;
    setDraftFilters(value);
    try {
      setAppliedFilters(value);
    } catch {
      // A partially edited invalid field remains a draft until it is valid.
    }
  };
  const [preferredSort, setSort] = useProfileValue<LibrarySort>(
    (c) => c.library.sort,
    (c, sort) => ({ ...c, library: { ...c.library, sort } }),
    'created_at',
  );
  // Keep the remembered result sort, but use a match sort while its context
  // columns are absent. The API requires a position filter for occurrence keys.
  const sort =
    !filters.position_id && isOccurrenceSort(preferredSort)
      ? 'created_at'
      : preferredSort;
  const [direction, setDirection] = useProfileValue<'asc' | 'desc'>(
    (c) => c.library.direction,
    (c, direction) => ({ ...c, library: { ...c.library, direction } }),
    'desc',
  );
  const resultNames = useRef<Record<string, string>>({});
  const [exact, setExact] = useState(''),
    [positionInputError, setPositionInputError] = useState(false);
  const positionSearchForm = useRef<HTMLDetailsElement>(null);
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
    setSelected(new Set());
    setResult(undefined);
    setExact(requestedPositionSearch.positionId);
    setPositionInputError(false);
    setFilters({
      ...emptyFilters(),
      position_id: requestedPositionSearch.positionId,
    });
    if (positionSearchForm.current) positionSearchForm.current.open = true;
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
    setSelected(new Set());
    setResult(undefined);
  }, [matchScope]);
  const loadMatches = useCallback(
    (signal: AbortSignal, after?: string) =>
      loadLibrary(databaseId, filters, sort, direction, signal, after),
    [databaseId, filters, sort, direction],
  );
  const {
    items: matches,
    resultScope: matchResultScope,
    cursor,
    loading: matchLoading,
    moreLoading,
    error: searchError,
    more,
  } = useCursorSearch({
    scope: matchScope,
    revision: JSON.stringify([
      revision,
      localRevision,
      annotationRevision,
      visible,
    ]),
    loadPage: loadMatches,
    itemKey: libraryRowKey,
    debounceMs: 250,
  });
  const loading = matchLoading || moreLoading;
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
    setSelected(new Set());
  }
  function toggle(id: string, checked: boolean) {
    setSelected((previous) => {
      const next = new Set(previous);
      if (checked) {
        if (next.size < 100) next.add(id);
      } else next.delete(id);
      return next;
    });
  }
  function selectAll(checked: boolean) {
    setSelected(
      checked ? new Set(matches.slice(0, 100).map((m) => m.id)) : new Set(),
    );
  }
  function actionResult(response: ActionResponse) {
    setResult(response);
    resultNames.current = Object.fromEntries(
      matches.map((m) => [m.id, m.display_name]),
    );
    const deleted = response.results
      .filter((r) => r.status === 'deleted')
      .map((r) => r.match_id);
    setSelected(
      new Set(
        response.results
          .filter(
            (r) =>
              ![
                'copied',
                'moved',
                'deleted',
                'tagged',
                'same_database',
                'duplicate',
              ].includes(r.status),
          )
          .map((r) => r.match_id),
      ),
    );
    setRevision((n) => n + 1);
    onChanged(deleted);
  }
  function changed(id?: string, validateSelection = false) {
    if (id !== undefined) changeDatabase(id);
    setRevision((n) => n + 1);
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
    if (isUnfinishedCompetition(match)) {
      if (match.play_session_id) onResumePlay?.(match.play_session_id);
      return;
    }
    void matchOpening.open(match);
  }
  function isCurrentRow(match: LibraryMatch) {
    return match.occurrence
      ? match.occurrence.id === currentPosition?.id
      : match.id === selectedMatchId;
  }
  function matchName(match: LibraryMatch, player: 'white' | 'black') {
    return (
      <button
        className={`match-card ${isCurrentRow(match) ? 'active' : ''}`}
        disabled={
          matchResultScope !== matchScope ||
          matchOpening.openingId === libraryRowKey(match)
        }
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
  const pr = (value: number | null) =>
    value == null
      ? '—'
      : formatNumber(value, {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        });
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
        <div>
          <small>
            {l('importTarget', {
              name:
                active?.name ??
                databases.find((d) => d.is_default)?.name ??
                '…',
            })}
          </small>
          <ActionButton
            icon="import"
            label={t('app.import')}
            className="primary"
            onClick={onImport}
          />
        </div>
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
              <Search size={16} aria-hidden="true" />
              <input
                aria-label={t('app.searchMatches')}
                placeholder={t('app.searchPlaceholder')}
                value={draftFilters.q}
                disabled={!!profile && !profile.ready}
                onChange={(e) =>
                  setFilters((f) => ({ ...f, q: e.target.value }))
                }
              />
            </label>
            <ActionButton
              icon="filter"
              label={l('filters')}
              compact
              onClick={() => setExpanded((v) => !v)}
              aria-expanded={expanded}
            />
          </div>
          {expanded && (
            <LibraryFilters
              filters={draftFilters}
              onChange={setFilters}
              onClear={() => setFilters(emptyFilters())}
              currentPosition={currentPosition}
            />
          )}
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
          <div className="database-searches">
            <AnnotationSearch
              revision={annotationRevision}
              databaseId={databaseId}
              onOpen={onAnnotation}
            />
            <details ref={positionSearchForm} className="position-search">
              <summary>
                <Search size={16} aria-hidden="true" /> {t('app.findPosition')}{' '}
                <span>{t('app.positionId')}</span>
              </summary>
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
                  compact
                  disabled={loading || !exact.trim()}
                />
                {currentPosition && (
                  <ActionButton
                    icon="view"
                    label={t('app.usePosition')}
                    compact
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
            </details>
          </div>
          <div className="library-table-toolbar">
            <div className="database-count">
              {loading
                ? t('app.searching')
                : `${formatNumber(matches.length)}${cursor ? '+' : ''}`}{' '}
              {filters.position_id ? l('occurrences') : t('app.matches')} ·{' '}
              {active?.name ?? l('allDatabases')}
            </div>
            {filters.position_id.trim() && (
              <span className="library-position-filter">
                <span>
                  {t('app.positionId')}:{' '}
                  <code>{filters.position_id.trim()}</code>
                </span>
                <ActionButton
                  icon="clear"
                  label={l('clearPositionFilter')}
                  compact
                  type="button"
                  onClick={() =>
                    setFilters((previous) => ({ ...previous, position_id: '' }))
                  }
                />
              </span>
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
          {selected.size > 0 && <p className="fineprint">{l('batchLimit')}</p>}
          {selected.size > 0 && (
            <div className="library-batch-bar">
              <strong>{l('selected', { count: selected.size })}</strong>
              {(['copy', 'move', 'tags', 'export', 'delete'] as const).map(
                (action) => (
                  <ActionButton
                    icon={action === 'tags' ? 'edit' : action}
                    label={l(action)}
                    compact
                    key={action}
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
                onClick={() => setSelected(new Set())}
                aria-label={l('clearSelection')}
              />
            </div>
          )}
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
                          className={column.className}
                          style={{
                            width:
                              columns.widths?.[column.id] ??
                              column.defaultWidth,
                          }}
                        />
                      ))}
                    {columns.activeColumns
                      .map((id) =>
                        LIBRARY_COLUMNS.find((column) => column.id === id)!,
                      )
                      .map((column) => (
                        <col
                          key={column.id}
                          className={column.className}
                          style={
                            columns.widths
                              ? { width: columns.widths[column.id] }
                              : undefined
                          }
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
                          checked={
                            matches.length > 0 &&
                            matches
                              .slice(0, 100)
                              .every((m) => selected.has(m.id))
                          }
                          disabled={!matches.length}
                          onChange={(e) => selectAll(e.target.checked)}
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
                      <th scope="col" data-column="actions">
                        {l('actions')}
                        {columns.handle('actions', l('actions'))}
                      </th>
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
                                {m.occurrence
                                  ? formatNumber(m.occurrence.move_no)
                                  : '—'}
                              </td>
                            </>
                          )
                        }
                      >
                        <td onClick={(e) => e.stopPropagation()}>
                          <input
                            type="checkbox"
                            aria-label={l('select', {
                              name: m.occurrence
                                ? `${m.display_name} · ${l('occurrenceGame')} ${m.occurrence.game_no} · ${l('occurrenceMove')} ${m.occurrence.move_no}`
                                : m.display_name,
                            })}
                            checked={selected.has(m.id)}
                            onChange={(e) => toggle(m.id, e.target.checked)}
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
                          </td>
                        )}
                        {showColumn('black') && (
                          <td>
                            {showColumn('white')
                              ? m.black
                              : matchName(m, 'black')}
                          </td>
                        )}
                        {showColumn('black_pr') && (
                          <td className="library-pr-cell">
                            {pr(canShowMatchPr(m) ? m.black_pr : null)}
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
                        <td>
                          <div className="library-row-actions">
                            {m.play_session_id &&
                              m.play_phase !== 'match_over' &&
                              onResumePlay && (
                                <ActionButton
                                  icon="resume"
                                  label={l('resumePlay')}
                                  className="library-row-resume"
                                  aria-label={`${l('resumePlay')}: ${m.display_name}`}
                                  disabled={matchResultScope !== matchScope}
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    onResumePlay(m.play_session_id!);
                                  }}
                                />
                              )}
                            {compactActions && (
                              <ActionButton
                                icon="open"
                                label={w('openMatch')}
                                compact
                                className="library-row-open"
                                aria-label={`${w('openMatch')}: ${m.display_name}`}
                                disabled={
                                  matchResultScope !== matchScope ||
                                  matchOpening.openingId === libraryRowKey(m)
                                }
                                onClick={(event) => {
                                  event.stopPropagation();
                                  openMatchRow(m);
                                }}
                              />
                            )}
                            <ActionButton
                              icon="edit"
                              label={l('editMatch')}
                              compact
                              className="library-row-edit"
                              aria-label={`${l('editMatch')}: ${m.display_name}`}
                              onClick={(e) => {
                                e.stopPropagation();
                                setDialog({ kind: 'metadata', match: m });
                              }}
                            />
                            <ActionLink
                              icon="export"
                              label={`${l('export')} SGF`}
                              compact
                              href={`/api/matches/${m.id}/source`}
                              aria-label={`${l('export')}: SGF`}
                              onClick={(e) => e.stopPropagation()}
                            />
                          </div>
                        </td>
                      </OrderedLibraryRow>
                    ))}
                  </tbody>
                </table>
              </div>
              {!matches.length && !loading && (
                <p className="library-empty">{l('noMatches')}</p>
              )}
              {cursor && (
                <ActionButton
                  icon="more"
                  label={
                    filters.position_id
                      ? l('moreOccurrences')
                      : t('app.moreMatches')
                  }
                  className="load-more"
                  disabled={loading}
                  onClick={() => void more()}
                />
              )}
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
