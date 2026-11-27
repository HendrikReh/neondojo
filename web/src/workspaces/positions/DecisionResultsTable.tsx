import { useId, type ReactNode } from 'react';
import EngineLabel from '../../shared/engine/EngineLabel';
import { useResizableColumns } from '../../shared/hooks/useResizableColumns';
import {
  layoutColumns,
  useWorkspaceLayout,
} from '../../shared/hooks/useWorkspaceSession';
import { useI18n } from '../../shared/i18n/i18n';
import ActionButton from '../../shared/ui/ActionButton';
import LibraryColumnPicker from '../../shared/ui/LibraryColumnPicker';
import LibrarySortHeading from '../../shared/ui/LibrarySortHeading';
import { NumberedSearchPagination } from '../../shared/ui/SearchPagination';
import { SearchResultRow } from '../../shared/ui/SearchResultList';
import { useLibraryText } from '../library/library-messages';
import type { DecisionHit } from '../study/mistakes/recurring-mistakes-api';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import { mistakeProvenance } from '../study/mistakes/recurring-mistakes-provenance';
import { useDecisionSearchText } from './decision-search-messages';
import {
  DECISION_COLUMN_IDS,
  DECISION_COLUMN_LAYOUT,
  DECISION_COLUMNS,
  type DecisionColumn,
} from './decision-table-columns';

export default function DecisionResultsTable({
  visible,
  items,
  keys,
  preview,
  page,
  total,
  limit,
  direction,
  loading,
  summary,
  onSelect,
  onOpen,
  onToggleSort,
  onPageChange,
}: {
  visible: boolean;
  items: readonly DecisionHit[];
  keys: readonly string[];
  preview?: DecisionHit;
  page: number;
  total: number;
  limit: number;
  direction: 'asc' | 'desc';
  loading: boolean;
  summary: ReactNode;
  onSelect: (key: string) => void;
  onOpen: (key: string) => void;
  onToggleSort: () => void;
  onPageChange: (page: number) => void;
}) {
  const t = useDecisionSearchText();
  const l = useLibraryText();
  const mistake = useMistakeText();
  const { formatDecision, formatNumber } = useI18n();
  const rowPrefix = `${useId()}-`;
  const [shownColumns, setShownColumns] = useWorkspaceLayout<DecisionColumn[]>(
    'decisions.visibleColumns',
    undefined,
    [...DECISION_COLUMN_IDS],
    (value) => layoutColumns(value, DECISION_COLUMN_IDS),
  );
  const activeColumns = DECISION_COLUMNS.filter(({ id }) =>
    shownColumns.includes(id),
  );
  const columns = useResizableColumns(DECISION_COLUMN_LAYOUT, {
    visible,
    activeColumns: activeColumns.map(({ id }) => id),
    resizeLabel: (column) => l('resizeColumn', { column }),
  });

  const score = (hit: DecisionHit) =>
    hit.away
      ? t('scoreAway', { own: hit.away[0], opponent: hit.away[1] })
      : '—';
  const cube = (hit: DecisionHit) =>
    `${formatNumber(hit.cube_value)}${
      hit.cube_owner == null ? '' : ` · ${hit.players[hit.cube_owner]}`
    }`;
  const loss = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });

  function cell(column: DecisionColumn, hit: DecisionHit) {
    switch (column) {
      case 'match':
        return `${hit.players[0]} – ${hit.players[1]}`;
      case 'move':
        return `${hit.reference.game_no}.${hit.reference.decision_no}`;
      case 'kind':
        return mistake(hit.mistake_kind ?? hit.kind);
      case 'score':
        return `${score(hit)}${hit.crawford ? ' · Crawford' : ''}`;
      case 'cube':
        return cube(hit);
      case 'played':
        return formatDecision(hit.played);
      case 'best':
        return formatDecision(hit.best);
      case 'loss':
        return loss(hit.equity_loss);
      case 'source':
        return (
          <>
            <EngineLabel
              details={hit.engine}
              tooltip={mistakeProvenance(hit, mistake)}
            />
            {hit.depth && ` · ${hit.depth}`}
          </>
        );
    }
  }

  return (
    <div className="position-browser-main" aria-busy={loading}>
      <div className="position-page-select decision-table-toolbar">
        <div className="decision-search-summary">{summary}</div>
        {!shownColumns.includes('loss') && (
          <span className="library-hidden-sort">
            {l('sortedBy', {
              column: t('loss'),
              direction: l(direction === 'asc' ? 'ascending' : 'descending'),
            })}
          </span>
        )}
        <LibraryColumnPicker
          options={DECISION_COLUMN_IDS.map((id) => ({
            id,
            label: t(id),
          }))}
          shown={shownColumns}
          onToggle={(id, checked) =>
            setShownColumns((current) => {
              const next = DECISION_COLUMN_IDS.filter((column) =>
                column === id ? checked : current.includes(column),
              );
              return next.length ? next : current;
            })
          }
          onAll={() => setShownColumns([...DECISION_COLUMN_IDS])}
          onReset={() => setShownColumns([...DECISION_COLUMN_IDS])}
        />
        <ActionButton
          icon="reset"
          label={l('resetColumnWidths')}
          compact
          type="button"
          className="position-width-reset"
          onClick={columns.reset}
          disabled={!columns.custom}
        />
      </div>
      <div className="position-table-scroll">
        <table
          ref={columns.tableRef}
          className={`position-table saved-position-table decision-search-table${columns.dragging ? ' resizing-table' : ''}`}
          style={
            columns.tableStyle ?? {
              minWidth: activeColumns.reduce(
                (width, column) => width + column.defaultWidth,
                0,
              ),
            }
          }
          aria-label={t('title')}
        >
          <colgroup>
            {activeColumns.map((column) => (
              <col
                key={column.id}
                data-column={column.id}
                className={column.className}
                style={{
                  width: columns.widths?.[column.id] ?? column.defaultWidth,
                }}
              />
            ))}
          </colgroup>
          <thead>
            <tr>
              {activeColumns.map(({ id }) =>
                id === 'loss' ? (
                  <LibrarySortHeading<'loss'>
                    key={id}
                    fields={[{ key: 'loss', label: t('loss') }]}
                    sort="loss"
                    direction={direction}
                    disabled={loading}
                    column={id}
                    resizeHandle={columns.handle(id, t(id))}
                    onSort={onToggleSort}
                  />
                ) : (
                  <th scope="col" key={id} data-column={id}>
                    {t(id)}
                    {columns.handle(id, t(id))}
                  </th>
                ),
              )}
            </tr>
          </thead>
          <tbody>
            {items.map((hit, index) => {
              const key = keys[index];
              return (
                <SearchResultRow
                  key={key}
                  ids={keys}
                  index={index}
                  idPrefix={rowPrefix}
                  active={hit === preview}
                  selectedClassName="similar-position-selected"
                  aria-selected={hit === preview}
                  onPreview={onSelect}
                  onOpen={onOpen}
                  jumpToEdges
                >
                  {activeColumns.map(({ id }) => (
                    <td
                      key={id}
                      className={
                        id === 'loss' ? 'decision-search-loss' : undefined
                      }
                    >
                      {cell(id, hit)}
                    </td>
                  ))}
                </SearchResultRow>
              );
            })}
          </tbody>
        </table>
      </div>
      <NumberedSearchPagination
        page={page}
        lastPage={Math.max(1, Math.ceil(total / limit))}
        onPageChange={onPageChange}
        label={t('title')}
      />
    </div>
  );
}
