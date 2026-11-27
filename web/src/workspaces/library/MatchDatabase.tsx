import { useCallback, useEffect, useId, useRef, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import type { Match, Position } from '../../shared/http/api';
import SearchPagination from '../../shared/ui/SearchPagination';
import { useI18n } from '../../shared/i18n/i18n';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import type { AnnotationHit } from '../analysis/notes/AnnotationSearch';
import { useLibraryText } from './library-messages';
import { emptyFilters, libraryRowKey, type LibraryMatch } from './library-api';
import { useLibraryMatchOpening } from './useLibraryMatchOpening';
import { useLibraryJobRefresh } from './useLibraryJobRefresh';
import DatabaseSidebar from './DatabaseSidebar';
import { useLibraryColumns } from './useLibraryColumns';
import { isUnfinishedCompetition } from './library-match-summary';
import LibraryImport from './import/LibraryImport';
import LibraryDialogs, { type LibraryDialogState } from './LibraryDialogs';
import LibraryActionResult from './LibraryActionResult';
import LibraryBatchBar from './LibraryBatchBar';
import LibraryTable from './LibraryTable';
import LibrarySearchPanels, { type SearchPanel } from './LibrarySearchPanels';
import LibraryToolbar from './LibraryToolbar';
import { useLibrarySelection } from './useLibrarySelection';
import { useLibraryQuery } from './useLibraryQuery';
import './library.css';

export interface PositionSearchRequest {
  positionId: string;
  token: number;
}

export interface LibraryRevisions {
  library: number;
  analysis?: string;
  annotation: number;
}

export interface LibrarySelectionContext {
  databaseId: string;
  currentPosition?: Position;
  requestedPositionSearch?: PositionSearchRequest;
  selectedMatchId?: string;
}

export interface LibraryCallbacks {
  onDatabase: (id: string) => void;
  onOpen: (match: Match, gameNo?: number) => void;
  onAnalyzeMatches?: (
    matches: LibraryMatch[],
    unavailableCount: number,
  ) => void;
  onPosition: (p: Position) => void;
  onAnnotation: (hit: AnnotationHit) => void;
  onImport: (databaseId: string) => void;
  onImportFolder: (databaseId: string) => void;
  onOpenPlay?: (id: string, resume?: boolean) => void;
  onChanged: (deletedIds?: string[], validateSelection?: boolean) => void;
}

export interface MatchDatabaseProps {
  visible: boolean;
  uploading: boolean;
  revisions: LibraryRevisions;
  context: LibrarySelectionContext;
  callbacks: LibraryCallbacks;
  bulkAnalysisProgress?: {
    total: number;
    completed: number;
    failed: number;
    missing: number;
    loading: boolean;
  };
}

export default function MatchDatabase({
  visible,
  uploading,
  revisions,
  context,
  callbacks,
  bulkAnalysisProgress,
}: MatchDatabaseProps) {
  const {
    library: revision,
    analysis: analysisRevision,
    annotation: annotationRevision,
  } = revisions;
  const {
    databaseId,
    currentPosition,
    requestedPositionSearch,
    selectedMatchId,
  } = context;
  const {
    onDatabase,
    onOpen,
    onAnalyzeMatches,
    onPosition,
    onAnnotation,
    onImport,
    onImportFolder,
    onOpenPlay,
    onChanged,
  } = callbacks;
  const { formatError } = useI18n(),
    w = useWorkspaceText(),
    l = useLibraryText();
  const [localRevision, setRevision] = useState(0),
    [duplicateRefresh, setDuplicateRefresh] = useState(0),
    [searchPanel, setSearchPanel] = useState<SearchPanel>(),
    [dialog, setDialog] = useState<LibraryDialogState>();
  const filterToggle = useRef<HTMLButtonElement>(null);
  const positionToggle = useRef<HTMLButtonElement>(null);
  const searchId = useId();
  const selectionHintId = `${searchId}-selection`;

  const query = useLibraryQuery({
    visible,
    databaseId,
    revision,
    localRevision,
    analysisRevision,
    annotationRevision,
    requestedPositionSearch,
    onDatabase,
    filterToggleRef: filterToggle,
    positionToggleRef: positionToggle,
  });
  const columns = useLibraryColumns(visible, !!query.filters.position_id);
  const matchOpening = useLibraryMatchOpening({
    scope: query.matchScope,
    positionId: query.filters.position_id.trim(),
    visible,
    onOpen,
    onPosition,
  });
  const selectionState = useLibrarySelection({
    matchScope: query.matchScope,
    matches: query.items,
    onMutated: (deleted) => {
      setRevision((n) => n + 1);
      setDuplicateRefresh((n) => n + 1);
      onChanged(deleted);
    },
  });
  const consumedPositionSearch = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (
      !requestedPositionSearch ||
      consumedPositionSearch.current === requestedPositionSearch.token
    )
      return;
    consumedPositionSearch.current = requestedPositionSearch.token;
    selectionState.resetTransientState();
    query.setPositionQuery(requestedPositionSearch.positionId);
    query.setFilters({
      ...emptyFilters(),
      position_id: requestedPositionSearch.positionId,
    });
    setSearchPanel('position');
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestedPositionSearch]);

  const error = query.error || query.databaseError;
  // Before MountHidden enables the first fetch, treat an unloaded page as
  // pending so the toolbar does not flash "0 Matches" or an empty state.
  const loading = query.loading || (!query.result && !query.error);
  function changeDatabase(id: string) {
    onDatabase(id);
    selectionState.clearSelection();
  }
  function changed(id?: string, validateSelection = false) {
    if (id !== undefined) changeDatabase(id);
    setRevision((n) => n + 1);
    setDuplicateRefresh((n) => n + 1);
    onChanged([], validateSelection);
  }
  function openMatchRow(match: LibraryMatch) {
    if (query.resultScope !== query.matchScope) return;
    if (match.play_paused || isUnfinishedCompetition(match)) {
      if (match.play_session_id) onOpenPlay?.(match.play_session_id);
      return;
    }
    void matchOpening.open(match);
  }
  const rowOpeningDisabled = (match: LibraryMatch) =>
    query.resultScope !== query.matchScope ||
    matchOpening.openingId === libraryRowKey(match);
  const isCurrentRow = (match: LibraryMatch) =>
    match.occurrence
      ? match.occurrence.id === currentPosition?.id
      : match.id === selectedMatchId;
  const reloadPage = useCallback(() => setRevision((n) => n + 1), []);
  useLibraryJobRefresh(query.items, visible, reloadPage);
  const active = query.databases.find((d) => d.id === databaseId);
  const selectionIsCurrent = query.resultScope === query.matchScope;

  return (
    <section className="match-database" aria-label={w('database')}>
      <div className="database-heading">
        <div>
          <h1>{w('database')}</h1>
          <p>{w('databaseDescription')}</p>
        </div>
        <LibraryImport
          databases={query.databases}
          databaseId={databaseId}
          uploading={uploading}
          onImport={onImport}
          onImportFolder={onImportFolder}
        />
      </div>
      <div className="library-layout">
        <DatabaseSidebar
          databases={query.databases}
          selected={databaseId}
          onSelect={changeDatabase}
          onNew={() => setDialog({ kind: 'database' })}
          onEdit={(database) => setDialog({ kind: 'database', database })}
          onDelete={(database) =>
            setDialog({ kind: 'deleteDatabase', database })
          }
        />
        <div className="library-main">
          <LibrarySearchPanels
            searchId={searchId}
            visible={visible}
            loading={loading}
            databaseId={databaseId}
            databases={query.databases}
            annotationRevision={annotationRevision}
            duplicateRefreshToken={duplicateRefresh + revision}
            currentPosition={currentPosition}
            filters={query.filters}
            draftFilters={query.draftFilters}
            searchPanel={searchPanel}
            onSearchPanel={setSearchPanel}
            exactPosition={query.exact}
            positionInputError={query.positionInputError}
            onExactPositionChange={query.setPositionQuery}
            onSearchPosition={query.searchPosition}
            onSetFilters={query.setFilters}
            onRemoveFilter={query.removeFilter}
            onAnnotation={onAnnotation}
            selected={selectionState.selected}
            onSelect={selectionState.changeSelection}
            onOpen={openMatchRow}
            openingDisabled={rowOpeningDisabled}
            filterToggleRef={filterToggle}
            positionToggleRef={positionToggle}
          />
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
          <LibraryToolbar
            loading={loading}
            total={query.total}
            hasPositionFilter={Boolean(query.filters.position_id)}
            activeDatabase={active}
            bulkAnalysisProgress={bulkAnalysisProgress}
            sort={query.sort}
            direction={query.direction}
            columns={columns}
          />
          {query.filters.position_id && query.items.length > 0 && (
            <p id={selectionHintId} className="library-occurrence-selection">
              {l('occurrenceSelection')}
            </p>
          )}
          <LibraryBatchBar
            selection={selectionState.selection}
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
                ids: [...selectionState.selected],
                source: databaseId,
              })
            }
            onClear={selectionState.clearSelection}
          />
          {selectionState.result && (
            <LibraryActionResult
              result={selectionState.result}
              names={selectionState.resultNames}
            />
          )}
          <div className="library-content">
            <div>
              <LibraryTable
                matches={query.items}
                databases={query.databases}
                occurrences={!!query.filters.position_id}
                columns={columns}
                sort={query.sort}
                direction={query.direction}
                onSort={query.sortBy}
                loading={loading}
                selected={selectionState.selected}
                onSelect={selectionState.changeSelection}
                selectionHintId={selectionHintId}
                isCurrent={isCurrentRow}
                openingId={matchOpening.openingId}
                openingDisabled={rowOpeningDisabled}
                onOpen={openMatchRow}
              />
              {!query.items.length && !loading && !query.error && (
                <div className="library-empty">
                  <p role="status">{l('noMatches')}</p>
                  {query.hasFilters && (
                    <ActionButton
                      type="button"
                      icon="clear"
                      label={l('clearFilters')}
                      onClick={() => {
                        filterToggle.current?.focus();
                        query.setFilters(emptyFilters());
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
                page={query.page}
                lastPage={query.lastPage}
                busy={loading}
                onPageChange={query.setPage}
              />
              <p className="fineprint">{l('prHelp')}</p>
            </div>
          </div>
        </div>
      </div>
      {dialog && (
        <LibraryDialogs
          key={JSON.stringify(dialog)}
          state={dialog}
          databases={query.databases}
          onClose={() => setDialog(undefined)}
          onChanged={changed}
          onAction={selectionState.actionResult}
        />
      )}
    </section>
  );
}
