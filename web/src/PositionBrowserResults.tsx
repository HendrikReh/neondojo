import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import { useMemo } from 'react';
import {
  SearchResultNotes,
  SearchResultPlayers,
  SearchResultRow,
  SearchResultTable,
  SearchResultTags,
} from './SearchResultList';
import {
  SearchPreviewBoard,
  SearchPreviewDetails,
  SearchResultPreview,
} from './SearchResultPreview';
import LibrarySortHeading from './LibrarySortHeading';
import { useResizableColumns } from './useResizableColumns';
import { useLibraryText } from './library-messages';
import {
  POSITION_COLUMNS,
  POSITION_COLUMN_LAYOUT,
  type PositionColumn,
} from './position-table-columns';
import {
  positionPlayer,
  positionOpponent,
  createPositionSummaryFormatter,
} from './position-row-summary';
export { positionPlayer } from './position-row-summary';
import { useI18n } from './i18n';
import type {
  PositionBrowserHit,
  PositionSort,
  PositionDirection,
} from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';
import type { Theme } from './themes';
import { useAnalysisMatchHeaderText } from './analysis-match-header-messages';
export interface PositionResultsProps {
  items: PositionBrowserHit[];
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
    { formatNumber } = useI18n();
  return hit.native_error ? (
    <span className="position-native-error">
      <strong>
        {formatNumber(hit.native_error.equity_loss, {
          minimumFractionDigits: 3,
          maximumFractionDigits: 3,
        })}
      </strong>
      <small
        title={p('provenance', {
          engine: hit.native_error.engine,
          plies: hit.native_error.plies,
          job: hit.native_error.job_id,
        })}
      >
        {hit.native_error.engine} ·{' '}
        {p('depth', { plies: hit.native_error.plies })}
      </small>
    </span>
  ) : (
    <span className="position-unknown">{p('nativeUnknown')}</span>
  );
}
export function PositionBrowserResults({
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
}: PositionResultsProps) {
  const profile = useOptionalConfiguration();
  const p = usePositionText(),
    l = useLibraryText(),
    { formatNumber, formatDecision, locale } = useI18n();
  const activeColumns = useMemo(
    () =>
      POSITION_COLUMNS.filter(
        (column) => showPlayed || column.id !== 'played_move',
      ).map((column) => column.id),
    [showPlayed],
  );
  const columns = useResizableColumns(POSITION_COLUMN_LAYOUT, {
    visible,
    activeColumns,
    ...(profile?.current
      ? {
          widths: profile.current.positions.column_widths,
          configurationGeneration: profile.generation,
          onWidthsChange: (column_widths: Record<string, number> | undefined) =>
            profile.updateCurrent((c) => ({
              ...c,
              positions: {
                ...c.positions,
                column_widths: column_widths ?? null,
              },
            })),
        }
      : {}),
    resizeLabel: (column) => l('resizeColumn', { column }),
  });
  const summaries = useMemo(() => {
    const summary = createPositionSummaryFormatter(locale, showPlayed);
    return new Map(items.map((hit) => [hit.position.id, summary(hit)]));
  }, [items, locale, showPlayed]);
  const ids = items.map((hit) => hit.position.id),
    all = ids.length > 0 && ids.every((id) => selected.has(id));
  const heading = (
    column: PositionColumn,
    fields: { key: PositionSort; label: string; name?: string }[],
  ) => (
    <LibrarySortHeading<PositionSort>
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
  return (
    <div className="position-browser-results" aria-busy={busy}>
      <div className="position-page-select">
        <label>
          <input
            type="checkbox"
            checked={all}
            disabled={!ids.length || busy || (!all && selected.size >= 200)}
            onChange={(event) => onToggle(ids, event.target.checked)}
          />
          {p('selectPage')}
        </label>
        <label className="position-played-toggle">
          <input
            type="checkbox"
            checked={showPlayed}
            onChange={(event) => onShowPlayed?.(event.target.checked)}
          />
          {p('showPlayed')}
        </label>
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
        className={`position-table${showPlayed ? ' position-table-played' : ''}${columns.dragging ? ' resizing-table' : ''}`}
        style={columns.tableStyle}
      >
        <colgroup>
          {POSITION_COLUMNS.filter((column) =>
            activeColumns.includes(column.id),
          ).map((column) => (
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
          <tr>
            <th scope="col" data-column="select">
              <span className="visually-hidden">
                {p('selected', { count: selected.size })}
              </span>
              {columns.handle('select', l('selection'))}
            </th>
            {heading('origin', [
              { key: 'match_id', label: 'ID', name: p('matchId') },
              { key: 'match', label: l('name') },
            ])}
            {heading('database', [{ key: 'database', label: p('database') }])}
            {heading('game', [{ key: 'game', label: p('gameNumber') }])}
            {heading('ply', [{ key: 'ply', label: p('moveNumber') }])}
            {heading('player', [{ key: 'player', label: p('player') }])}
            {heading('opponent', [{ key: 'opponent', label: p('opponent') }])}
            {heading('dice', [{ key: 'dice', label: p('dice') }])}
            {heading('score', [{ key: 'score', label: p('score') }])}
            {heading('cube', [
              { key: 'cube', label: p('cubeShort'), name: p('cube') },
            ])}
            {showPlayed &&
              heading('played_move', [
                { key: 'played_move', label: p('playedMove') },
              ])}
            {heading('loss', [
              { key: 'loss', label: p('lossShort'), name: p('nativeError') },
            ])}
            {heading('decks', [{ key: 'decks', label: p('decks') }])}
            {heading('notes', [
              { key: 'tags', label: l('tags') },
              { key: 'comment', label: l('comment') },
            ])}
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
              <td>{hit.database_name}</td>
              <td>{formatNumber(hit.position.game_no)}</td>
              <td>{formatNumber(hit.position.ply)}</td>
              <SearchResultPlayers
                player={positionPlayer(hit)}
                opponent={positionOpponent(hit)}
              />
              <td>
                {hit.position.dice.map((die) => formatNumber(die)).join('–')}
              </td>
              <td>
                {hit.white_score == null || hit.black_score == null
                  ? 'n/a'
                  : `${formatNumber(hit.white_score)}–${formatNumber(hit.black_score)}`}
              </td>
              <td>
                {hit.cube_value == null ? 'n/a' : formatNumber(hit.cube_value)}
              </td>
              {showPlayed && (
                <td className="position-played-move">
                  {formatDecision(hit.position.notation)}
                </td>
              )}
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
                  'n/a'
                )}
              </td>
              <td className="position-row-decks">
                {hit.decks.length
                  ? hit.decks.map((deck) => deck.name).join(', ')
                  : '—'}
              </td>
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
  showPlayed = false,
  visible = true,
}: {
  hit?: PositionBrowserHit;
  visible?: boolean;
  showPlayed?: boolean;
  theme: Theme;
  onOpen: (id: string) => void;
}) {
  const p = usePositionText(),
    { formatNumber, formatDecision } = useI18n(),
    headerText = useAnalysisMatchHeaderText();
  return (
    <SearchResultPreview title={p('preview')} hint={p('previewHint')}>
      {hit && (
        <>
          <h3>{hit.display_name}</h3>
          <p className="position-preview-context">
            {hit.database_name} · {p('game', { number: hit.position.game_no })}{' '}
            · {p('ply', { number: hit.position.ply })}
          </p>
          {'source_available' in hit && hit.source_available === false && (
            <p className="position-source-removed">
              {headerText('retainedPosition')}
            </p>
          )}
          <SearchPreviewBoard
            position={hit.position}
            white={hit.match_info.white}
            black={hit.match_info.black}
            theme={theme}
            label={p('beforeMove')}
            visible={visible}
          />
          <p className="position-before-label">{p('beforeMove')}</p>
          {showPlayed && (
            <p className="position-preview-played">
              <strong>{p('playedMove')}:</strong>{' '}
              {formatDecision(hit.position.notation)}
            </p>
          )}
          <SearchPreviewDetails
            positionId={hit.position.position_id}
            decks={hit.decks.map((deck) => deck.name).join(', ')}
          >
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
          <HitNotes hit={hit} />
          <p>
            {p('nativeError')}: <NativeError hit={hit} />
          </p>
          {!hit.card_ready && (
            <p className="position-analysis-needed">{p('needed')}</p>
          )}
          <ActionButton
            icon="analysis"
            label={p('openAnalysis')}
            className="primary"
            onClick={() => onOpen(hit.position.id)}
          />
        </>
      )}
    </SearchResultPreview>
  );
}
