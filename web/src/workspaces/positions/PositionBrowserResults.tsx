import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import {
  layoutColumns,
  useWorkspaceLayout,
} from '../../shared/hooks/useWorkspaceSession';
import ActionButton from '../../shared/ui/ActionButton';
import EngineLabel from '../../shared/engine/EngineLabel';
import { useMemo, type ReactNode, type Ref } from 'react';
import {
  SearchResultNotes,
  SearchResultRow,
  SearchResultTable,
  SearchResultTags,
} from '../../shared/ui/SearchResultList';
import {
  SearchPreviewBoard,
  SearchPreviewDecks,
  SearchPreviewDetails,
  SearchResultPreview,
} from '../../shared/ui/SearchResultPreview';
import LibrarySortHeading from '../../shared/ui/LibrarySortHeading';
import LibraryColumnPicker from '../../shared/ui/LibraryColumnPicker';
import { useResizableColumns } from '../../shared/hooks/useResizableColumns';
import { useLibraryText } from '../library/library-messages';
import {
  POSITION_COLUMNS,
  POSITION_COLUMN_LAYOUT,
  DEFAULT_POSITION_COLUMNS,
  POSITION_DATA_COLUMNS,
  type PositionColumn,
  type PositionDataColumn,
} from './position-table-columns';
import {
  positionPlayer,
  positionOpponent,
  createPositionSummaryFormatter,
  positionAnalysisProvenance,
} from './position-row-summary';
export { positionPlayer } from './position-row-summary';
import { useI18n } from '../../shared/i18n/i18n';
import type {
  PositionBrowserHit,
  PositionSort,
  PositionDirection,
} from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';
import type { Theme } from '../../shared/board/themes';
import { useAnalysisMatchHeaderText } from '../analysis/analysis-match-header-messages';
export interface PositionResultsProps {
  selectPageRef?: Ref<HTMLInputElement>;
  items: readonly PositionBrowserHit[];
  selected: ReadonlySet<string>;
  previewId?: string;
  onPreview: (id: string) => void;
  onToggle: (ids: string[], checked: boolean) => void;
  onOpen: (id: string) => void;
  onRemove?: (ids: string[]) => void;
  busy: boolean;
  visible?: boolean;
  showPlayed?: boolean;
  onShowPlayed?: (value: boolean) => void;
  sort?: PositionSort;
  direction?: PositionDirection;
  onSort?: (sort: PositionSort) => void;
  sortDisabled?: boolean;
  /** Actions for the current selection, shown at the start of the toolbar. */
  toolbarStart?: ReactNode;
}
const scopeKeys: Record<
  PositionBrowserHit['notes'][number]['scope'],
  PositionMessageKey
> = {
  match: 'scopeMatch',
  game: 'scopeGame',
  position: 'scopePosition',
  move: 'scopeMove',
};
function HitNotes({ hit }: { hit: PositionBrowserHit }) {
  const p = usePositionText();
  return (
    <div className="position-hit-notes">
      {!!hit.tags.length && (
        <div className="annotation-tags">
          <SearchResultTags tags={hit.tags} />
        </div>
      )}
      {hit.notes
        .filter((note) => note.comment_excerpt)
        .map((note, index) => (
          <p key={`${note.scope}-${index}`}>
            <small>{p(scopeKeys[note.scope])}</small> {note.comment_excerpt}
          </p>
        ))}
    </div>
  );
}
function NativeError({ hit }: { hit: PositionBrowserHit }) {
  const p = usePositionText(),
    { formatNumber, locale } = useI18n();
  return hit.native_error ? (
    <span className="position-native-error">
      <strong>
        {formatNumber(hit.native_error.equity_loss, {
          minimumFractionDigits: 3,
          maximumFractionDigits: 3,
        })}
      </strong>
      <small>
        <EngineLabel
          details={positionAnalysisProvenance(hit.native_error, locale)}
          tooltip={positionAnalysisProvenance(hit.native_error, locale)}
        />
      </small>
    </span>
  ) : (
    <span className="position-unknown">{p('nativeUnknown')}</span>
  );
}
export function PositionBrowserResults({
  selectPageRef,
  items,
  selected,
  previewId,
  onPreview,
  onToggle,
  onOpen,
  onRemove,
  busy,
  visible = true,
  showPlayed = false,
  onShowPlayed,
  sort = 'match',
  direction = sort === 'loss' ? 'desc' : 'asc',
  onSort,
  sortDisabled = false,
  toolbarStart,
}: PositionResultsProps) {
  const legacy = useWorkspaceDefaults();
  const p = usePositionText(),
    l = useLibraryText(),
    { formatNumber, formatDecision, locale } = useI18n();
  const [shown, setShown] = useWorkspaceLayout<PositionDataColumn[]>(
    'positions.visibleColumns',
    legacy?.positions.visible_columns,
    [...DEFAULT_POSITION_COLUMNS],
    (value) => layoutColumns(value, POSITION_DATA_COLUMNS),
  );
  const activeColumns = useMemo(
    () =>
      POSITION_COLUMNS.filter(
        ({ id }) =>
          id === 'select' ||
          id === 'actions' ||
          (id === 'played_move' ? showPlayed : shown.includes(id)),
      ),
    [showPlayed, shown],
  );
  const columns = useResizableColumns(POSITION_COLUMN_LAYOUT, {
    visible,
    activeColumns: activeColumns.map(({ id }) => id),
    legacyWidths: legacy?.positions.column_widths,
    resizeLabel: (column) => l('resizeColumn', { column }),
  });
  const summaries = useMemo(() => {
    const summary = createPositionSummaryFormatter(locale, showPlayed);
    return new Map(items.map((hit) => [hit.position.id, summary(hit)]));
  }, [items, locale, showPlayed]);
  const ids = items.map((hit) => hit.position.id),
    all = ids.length > 0 && ids.every((id) => selected.has(id));
  const fieldsByColumn: Record<
    Exclude<PositionColumn, 'select' | 'actions'>,
    readonly { key: PositionSort; label: string; name?: string }[]
  > = {
    origin: [
      { key: 'match_id', label: 'ID', name: p('matchId') },
      { key: 'match', label: l('match') },
    ],
    database: [{ key: 'database', label: p('database') }],
    game: [{ key: 'game', label: p('gameNumber') }],
    ply: [{ key: 'ply', label: p('moveNumber') }],
    player: [{ key: 'player', label: p('player') }],
    opponent: [{ key: 'opponent', label: p('opponent') }],
    dice: [{ key: 'dice', label: p('dice') }],
    score: [{ key: 'score', label: p('score') }],
    cube: [{ key: 'cube', label: p('cubeShort'), name: p('cube') }],
    played_move: [{ key: 'played_move', label: p('playedMove') }],
    loss: [{ key: 'loss', label: p('equityLoss'), name: p('nativeError') }],
    decks: [{ key: 'decks', label: p('decks') }],
    notes: [
      { key: 'tags', label: l('tags') },
      { key: 'comment', label: l('comment') },
    ],
  };
  const heading = (column: keyof typeof fieldsByColumn) => {
    const fields = fieldsByColumn[column];
    return (
      <LibrarySortHeading<PositionSort>
        key={column}
        fields={fields}
        sort={sort}
        direction={direction}
        onSort={(value) => onSort?.(value)}
        disabled={sortDisabled || !onSort}
        column={column}
        resizeHandle={columns.handle(
          column,
          fields.map((field) => field.name ?? field.label).join(' / '),
        )}
      />
    );
  };
  const sortColumn = Object.values(fieldsByColumn)
    .flat()
    .find((field) => field.key === sort);
  const sortVisible = activeColumns.some(
    ({ id }) =>
      id !== 'select' &&
      id !== 'actions' &&
      fieldsByColumn[id].some((field) => field.key === sort),
  );
  return (
    <div className="position-browser-results" aria-busy={busy}>
      <div className="position-page-select">
        {toolbarStart}
        <label className="position-played-toggle">
          <input
            type="checkbox"
            checked={showPlayed}
            onChange={(event) => onShowPlayed?.(event.target.checked)}
          />
          {p('showPlayed')}
        </label>
        {!sortVisible && sortColumn && (
          <span className="library-hidden-sort">
            {l('sortedBy', {
              column: sortColumn.name ?? sortColumn.label,
              direction: l(direction === 'asc' ? 'ascending' : 'descending'),
            })}
          </span>
        )}
        <LibraryColumnPicker
          options={POSITION_DATA_COLUMNS.map((id) => ({
            id,
            label:
              id === 'origin'
                ? l('match')
                : fieldsByColumn[id].map((field) => field.label).join(' / '),
          }))}
          shown={shown}
          onToggle={(id, checked) =>
            setShown((current) => {
              const next = POSITION_DATA_COLUMNS.filter((column) =>
                column === id ? checked : current.includes(column),
              );
              return next.length ? next : current;
            })
          }
          onAll={() => setShown([...POSITION_DATA_COLUMNS])}
          onReset={() => setShown([...DEFAULT_POSITION_COLUMNS])}
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
      <SearchResultTable
        ref={columns.tableRef}
        className={`position-table saved-position-table${columns.dragging ? ' resizing-table' : ''}`}
        style={
          columns.tableStyle ?? {
            minWidth: activeColumns.reduce(
              (width, column) => width + column.defaultWidth,
              0,
            ),
          }
        }
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
            <th scope="col" data-column="select">
              <input
                ref={selectPageRef}
                type="checkbox"
                aria-label={p('selectPage')}
                title={p('selectPage')}
                checked={all}
                disabled={!ids.length || busy || (!all && selected.size >= 200)}
                onChange={(event) => onToggle(ids, event.target.checked)}
              />
              {columns.handle('select', l('selection'))}
            </th>
            {activeColumns.map(
              ({ id }) => id !== 'select' && id !== 'actions' && heading(id),
            )}
            <th scope="col" data-column="actions">
              <span className="visually-hidden">{l('actions')}</span>
              {columns.handle('actions', l('actions'))}
            </th>
          </tr>
        </thead>
        <tbody>
          {items.map((hit, index) => (
            <SearchResultRow
              key={hit.position.id}
              ids={ids}
              index={index}
              idPrefix="position-row-"
              text={summaries.get(hit.position.id)}
              active={previewId === hit.position.id}
              data-position-id={hit.position.id}
              onPreview={onPreview}
              onOpen={onOpen}
            >
              <td onClick={(event) => event.stopPropagation()}>
                <input
                  type="checkbox"
                  aria-label={p('selectRow', {
                    game: hit.position.game_no,
                    ply: hit.position.ply,
                    player: positionPlayer(hit),
                  })}
                  checked={selected.has(hit.position.id)}
                  disabled={
                    busy ||
                    (!selected.has(hit.position.id) && selected.size >= 200)
                  }
                  onChange={(event) =>
                    onToggle([hit.position.id], event.target.checked)
                  }
                />
              </td>
              {shown.includes('origin') && (
                <td>
                  <button
                    className="position-preview-link"
                    aria-pressed={previewId === hit.position.id}
                    onClick={(event) => {
                      event.stopPropagation();
                      onPreview(hit.position.id);
                    }}
                  >
                    <strong className="position-match-number">
                      #{formatNumber(hit.match_no)}
                    </strong>
                    <span className="position-match-name">
                      {hit.display_name}
                    </span>
                  </button>
                </td>
              )}
              {shown.includes('database') && <td>{hit.database_name}</td>}
              {shown.includes('game') && (
                <td>{formatNumber(hit.position.game_no)}</td>
              )}
              {shown.includes('ply') && (
                <td>{formatNumber(hit.position.ply)}</td>
              )}
              {shown.includes('player') && (
                <td className="position-player">{positionPlayer(hit)}</td>
              )}
              {shown.includes('opponent') && (
                <td className="position-opponent">{positionOpponent(hit)}</td>
              )}
              {shown.includes('dice') && (
                <td>
                  {hit.position.dice.map((die) => formatNumber(die)).join('–')}
                </td>
              )}
              {shown.includes('score') && (
                <td>
                  {hit.white_score == null || hit.black_score == null
                    ? '—'
                    : `${formatNumber(hit.white_score)}–${formatNumber(hit.black_score)}`}
                </td>
              )}
              {shown.includes('cube') && (
                <td>
                  {hit.cube_value == null ? '—' : formatNumber(hit.cube_value)}
                </td>
              )}
              {showPlayed && (
                <td className="position-played-move">
                  {formatDecision(hit.position.notation)}
                </td>
              )}
              {shown.includes('loss') && (
                <td className="position-loss">
                  {hit.native_error ? (
                    <span
                      className={`position-loss-badge severity-${hit.native_error.severity}`}
                    >
                      {formatNumber(hit.native_error.equity_loss, {
                        minimumFractionDigits: 3,
                        maximumFractionDigits: 3,
                      })}
                    </span>
                  ) : (
                    '—'
                  )}
                </td>
              )}
              {shown.includes('decks') && (
                <td className="position-row-decks">
                  {hit.decks.length
                    ? hit.decks.map((deck) => deck.name).join(', ')
                    : '—'}
                </td>
              )}
              {shown.includes('notes') && (
                <td>
                  <SearchResultNotes
                    tags={hit.tags}
                    comment={hit.notes
                      .filter((note) => note.comment_excerpt)
                      .map(
                        (note) =>
                          `${p(scopeKeys[note.scope])}: ${note.comment_excerpt}`,
                      )
                      .join(' · ')}
                  />
                </td>
              )}
              <td>
                <div className="position-row-actions">
                  {onRemove && (
                    <ActionButton
                      icon="delete"
                      label={p('removeSaved')}
                      compact
                      className="position-remove-saved"
                      aria-label={p('removeSaved')}
                      disabled={busy}
                      onClick={(event) => {
                        event.stopPropagation();
                        onRemove([hit.position.id]);
                      }}
                    />
                  )}
                </div>
              </td>
            </SearchResultRow>
          ))}
        </tbody>
      </SearchResultTable>
    </div>
  );
}
export function PositionBrowserPreview({
  hit,
  theme,
  onOpen,
  onFindSimilar,
  showPlayed = false,
  visible = true,
}: {
  hit?: PositionBrowserHit;
  visible?: boolean;
  showPlayed?: boolean;
  theme: Theme;
  onOpen: (id: string) => void;
  onFindSimilar?: (
    position: PositionBrowserHit['position'],
    matchName: string,
  ) => void;
}) {
  const p = usePositionText(),
    { formatNumber, formatDecision } = useI18n(),
    headerText = useAnalysisMatchHeaderText();
  return (
    <SearchResultPreview
      className="saved-position-preview"
      title={p('preview')}
      hint={p('previewHint')}
      hideTitle
    >
      {hit && (
        <>
          <div className="saved-position-preview-header">
            <div>
              <h3>{hit.display_name}</h3>
              <p className="position-preview-context">
                {p('game', { number: hit.position.game_no })} ·{' '}
                {p('ply', { number: hit.position.ply })}
              </p>
            </div>
            <div className="saved-position-preview-actions">
              {onFindSimilar && (
                <ActionButton
                  icon="similar"
                  label={p('findSimilar')}
                  compact
                  onClick={() => onFindSimilar(hit.position, hit.display_name)}
                />
              )}
              <ActionButton
                icon="analysis"
                label={p('openAnalysis')}
                className="primary"
                onClick={() => onOpen(hit.position.id)}
              />
            </div>
          </div>
          <SearchPreviewBoard
            position={hit.position}
            white={hit.match_info.white}
            black={hit.match_info.black}
            theme={theme}
            label={p('beforeMove')}
            visible={visible}
          />
          <p className="position-before-label">{p('beforeMove')}</p>
          {hit.source_available === false && (
            <p className="position-source-removed">
              {headerText('retainedPosition')}
            </p>
          )}
          {showPlayed && (
            <p className="position-preview-played">
              <strong>{p('playedMove')}:</strong>{' '}
              {formatDecision(hit.position.notation)}
            </p>
          )}
          <p className="position-preview-loss">
            <strong>{p('nativeError')}:</strong> <NativeError hit={hit} />
          </p>
          <SearchPreviewDecks
            decks={hit.decks.map((deck) => deck.name).join(', ')}
          />
          <HitNotes hit={hit} />
          <details className="saved-position-preview-details">
            <summary>{p('previewDetails')}</summary>
            <SearchPreviewDetails
              positionId={hit.position.position_id}
              hideDecks
            >
              <div>
                <dt>{p('database')}</dt>
                <dd>{hit.database_name}</dd>
              </div>
              <div>
                <dt>{p('matchPoints')}</dt>
                <dd>{formatNumber(hit.match_info.match_length)}</dd>
              </div>
              {hit.crawford != null && (
                <div>
                  <dt>{p('crawford')}</dt>
                  <dd>{p(hit.crawford ? 'yes' : 'no')}</dd>
                </div>
              )}
              {hit.tournament && (
                <div>
                  <dt>{p('tournament')}</dt>
                  <dd>{hit.tournament}</dd>
                </div>
              )}
              {hit.round && (
                <div>
                  <dt>{p('round')}</dt>
                  <dd>{hit.round}</dd>
                </div>
              )}
            </SearchPreviewDetails>
          </details>
          {!hit.card_ready && (
            <p className="position-analysis-needed">{p('needed')}</p>
          )}
        </>
      )}
    </SearchResultPreview>
  );
}
