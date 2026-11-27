import ActionButton from '../../shared/ui/ActionButton';
import LibraryColumnPicker from '../../shared/ui/LibraryColumnPicker';
import { useI18n } from '../../shared/i18n/i18n';
import { usePlayModeText } from '../play/play-mode-messages';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { databaseDisplayName, useLibraryText } from './library-messages';
import type { Database, LibrarySort } from './library-api';
import type { LibraryColumn } from './library-columns';
import type { LibraryDataColumn } from './library-column-preferences';
import type { useLibraryColumns } from './useLibraryColumns';

export default function LibraryToolbar({
  loading,
  total,
  hasPositionFilter,
  activeDatabase,
  bulkAnalysisProgress,
  sort,
  direction,
  columns,
}: {
  loading: boolean;
  total: number;
  hasPositionFilter: boolean;
  activeDatabase?: Database;
  bulkAnalysisProgress?: {
    total: number;
    completed: number;
    failed: number;
    missing: number;
    loading: boolean;
  };
  sort: LibrarySort;
  direction: 'asc' | 'desc';
  columns: ReturnType<typeof useLibraryColumns>;
}) {
  const { t, formatNumber } = useI18n();
  const w = useWorkspaceText();
  const l = useLibraryText();
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

  return (
    <div className="library-table-toolbar">
      <div className="database-count">
        {loading ? t('app.searching') : `${formatNumber(total)}`}{' '}
        {hasPositionFilter ? l('occurrences') : t('app.matches')} ·{' '}
        {activeDatabase ? databaseDisplayName(activeDatabase, l) : l('allDatabases')}
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
            direction: l(direction === 'asc' ? 'ascending' : 'descending'),
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
  );
}
