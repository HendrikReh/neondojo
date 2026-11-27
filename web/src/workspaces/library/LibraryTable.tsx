import { Fragment, useMemo, type KeyboardEvent, type ReactNode } from 'react';
import ActionIcon from '../../shared/ui/action-icons';
import LibrarySortHeading from '../../shared/ui/LibrarySortHeading';
import { useClippedCellTooltip } from '../../shared/ui/useClippedCellTooltip';
import { prDigits } from '../../shared/format/pr-format';
import { useI18n } from '../../shared/i18n/i18n';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { usePlayModeText } from '../play/play-mode-messages';
import { OCCURRENCE_COLUMNS, type LibraryColumn } from './library-columns';
import type { LibraryDataColumn } from './library-column-preferences';
import {
  libraryRowKey,
  type LibraryMatch,
  type LibrarySort,
} from './library-api';
import {
  canShowMatchPr,
  createMatchSummaryFormatter,
  libraryAnalysisStatus,
} from './library-match-summary';
import { useLibraryText } from './library-messages';
import type { useDatabases } from './useDatabases';
import type { useLibraryColumns } from './useLibraryColumns';

type OccurrenceColumn = (typeof OCCURRENCE_COLUMNS)[number]['id'];

/** One page of matches, or of a position's occurrences, in the chosen columns. */
export default function LibraryTable({
  matches,
  databases,
  occurrences,
  columns,
  sort,
  direction,
  onSort,
  loading,
  selected,
  onSelect,
  selectionHintId,
  isCurrent,
  openingId,
  openingDisabled,
  onOpen,
}: {
  matches: readonly LibraryMatch[];
  databases: ReturnType<typeof useDatabases>['databases'];
  /** The rows are occurrences of a position. */
  occurrences: boolean;
  columns: ReturnType<typeof useLibraryColumns>;
  sort: LibrarySort;
  direction: 'asc' | 'desc';
  onSort: (key: LibrarySort) => void;
  loading: boolean;
  selected: ReadonlySet<string>;
  onSelect: (matches: readonly LibraryMatch[], checked: boolean) => void;
  selectionHintId: string;
  isCurrent: (match: LibraryMatch) => boolean;
  openingId?: string;
  openingDisabled: (match: LibraryMatch) => boolean;
  onOpen: (match: LibraryMatch) => void;
}) {
  const { t, locale, formatNumber } = useI18n(),
    w = useWorkspaceText(),
    l = useLibraryText();
  const modeText = usePlayModeText();
  const cellTooltip = useClippedCellTooltip();
  const showColumn = (id: LibraryColumn) =>
    columns.activeColumns.some((column) => column === id);
  const occurrenceNames = [
    ['occurrence_database', l('database')],
    ['occurrence_game', l('game')],
    ['occurrence_move', l('sourceMove')],
  ] as const;
  const heading = (
    key: LibrarySort & (LibraryColumn | OccurrenceColumn),
    label: string,
    name?: string,
  ) => (
    <LibrarySortHeading
      fields={[{ key, label, name }]}
      sort={sort}
      direction={direction}
      onSort={onSort}
      column={key}
      resizeHandle={columns.handle(key, name ?? label)}
    />
  );
  function matchName(match: LibraryMatch, player: 'white' | 'black') {
    return (
      <button
        className={`match-card ${isCurrent(match) ? 'active' : ''}`}
        disabled={openingDisabled(match)}
        aria-busy={openingId === libraryRowKey(match)}
        onClick={(event) => {
          event.stopPropagation();
          onOpen(match);
        }}
      >
        <span className="library-match-player" data-tooltip-overflow>
          {match[player]}
        </span>
        <span className="sr-only">
          {t('common.versus')} {match[player === 'white' ? 'black' : 'white']}
        </span>
        {match.display_name.trim() && match.display_name !== match.filename && (
          <span
            className="library-match-title"
            data-tooltip-overflow
            data-tooltip-text={match.display_name}
          >
            · {match.display_name}
          </span>
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
      onOpen(match);
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
  const allOnPageSelected =
    matches.length > 0 && matches.every((match) => selected.has(match.id));
  const someOnPageSelected = matches.some((match) => selected.has(match.id));
  const pr = (value: number | null) =>
    value == null ? '—' : formatNumber(value, prDigits);
  const date = (value: string | null) =>
    value
      ? dateTimeFormat(locale, { dateStyle: 'medium' }).format(
          new Date(value.length === 10 ? `${value}T12:00:00` : value),
        )
      : '—';
  const state = (m: LibraryMatch) =>
    libraryAnalysisStatus(m.analysis_status, locale);
  const summaries = useMemo(() => {
    const summary = createMatchSummaryFormatter(locale);
    return new Map(matches.map((match) => [match.id, summary(match)]));
  }, [matches, locale]);
  const dataColumns = columns.activeColumns.filter(
    (id): id is LibraryDataColumn => id !== 'select',
  );
  // Header and body rows render cells in the user's column order from these
  // maps, so hidden columns never build elements.
  const headerCells: Record<LibraryDataColumn, () => ReactNode> = {
    id: () => heading('id', 'ID'),
    white: () => heading('white', l('white')),
    white_pr: () => heading('white_pr', 'PR', `${l('white')} · PR`),
    black: () => heading('black', l('black')),
    black_pr: () => heading('black_pr', 'PR', `${l('black')} · PR`),
    play_mode: () => (
      <th scope="col" data-column="play_mode">
        <span className="library-sort-label">{modeText('mode')}</span>
        {columns.handle('play_mode', modeText('mode'))}
      </th>
    ),
    match_length: () => heading('match_length', l('length')),
    score: () => heading('score', l('result')),
    played_at: () => heading('played_at', l('played')),
    created_at: () => heading('created_at', l('imported'), l('imported')),
    tournament: () => heading('tournament', l('tournament')),
    position_count: () => heading('position_count', w('positions')),
    analysis_status: () => heading('analysis_status', l('analysis')),
    tags: () => heading('tags', l('tags')),
    comment: () => heading('comment', l('comment')),
  };
  const bodyCells: Record<LibraryDataColumn, (m: LibraryMatch) => ReactNode> = {
    id: (m) => <td className="library-id-cell">{formatNumber(m.match_no)}</td>,
    white: (m) => <td>{matchName(m, 'white')}</td>,
    white_pr: (m) => (
      <td className="library-pr-cell">
        {pr(canShowMatchPr(m) ? m.white_pr : null)}
        {importNotesMarker(m)}
      </td>
    ),
    black: (m) => (
      <td>
        {showColumn('white') ? (
          <span className="library-player-name" data-tooltip-overflow>
            {m.black}
          </span>
        ) : (
          matchName(m, 'black')
        )}
      </td>
    ),
    black_pr: (m) => (
      <td className="library-pr-cell">
        {pr(canShowMatchPr(m) ? m.black_pr : null)}
        {importNotesMarker(m)}
      </td>
    ),
    play_mode: (m) => (
      <td className="library-play-mode-cell" data-column="play_mode">
        {m.play_mode ? (
          <span className="library-play-mode" data-tooltip-overflow>
            {modeText(m.play_mode)} ·{' '}
            {modeText(m.play_assisted ? 'assisted' : 'independent')}
          </span>
        ) : (
          '—'
        )}
      </td>
    ),
    match_length: (m) => <td>{m.match_length || 'M'}</td>,
    score: (m) => (
      <td>
        {m.white_score != null && m.black_score != null
          ? `${m.white_score}–${m.black_score}`
          : 'n/a'}
      </td>
    ),
    played_at: (m) => <td data-tooltip-overflow>{date(m.played_on)}</td>,
    created_at: (m) => <td data-tooltip-overflow>{date(m.created_at)}</td>,
    tournament: (m) => <td data-tooltip-overflow>{m.tournament ?? '—'}</td>,
    position_count: (m) => (
      <td className="library-match-positions">
        {formatNumber(m.position_count)}
      </td>
    ),
    analysis_status: (m) => <td data-tooltip-overflow>{state(m)}</td>,
    tags: (m) => (
      <td className="library-tags-cell library-notes-cell">
        <div className="library-notes-summary">
          {m.tags.length > 0 && (
            <span
              className="library-note-tags"
              data-tooltip-overflow
              data-tooltip-text={m.tags.join(', ')}
            >
              {m.tags.map((tag) => (
                <span className="tag-chip" key={tag} data-tooltip-overflow>
                  {tag}
                </span>
              ))}
            </span>
          )}
        </div>
      </td>
    ),
    comment: (m) => (
      <td
        className="library-comment-cell"
        data-tooltip-overflow
        data-tooltip-text={m.comment}
      >
        <span className="library-note-comment">{m.comment}</span>
      </td>
    ),
  };
  return (
    <div className="match-list database-table-wrap">
      <table
        ref={columns.tableRef}
        className={`database-table${columns.dragging ? ' resizing-table library-table-resizing' : ''}`}
        style={columns.tableStyle ?? { minWidth: columns.minimumWidth }}
      >
        <colgroup>
          {occurrences &&
            OCCURRENCE_COLUMNS.map((column) => (
              <col
                key={column.id}
                data-column={column.id}
                className={column.className}
                style={{
                  width: columns.widths?.[column.id] ?? column.defaultWidth,
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
          <tr>
            {occurrences &&
              occurrenceNames.map(([key, label]) => (
                <Fragment key={key}>{heading(key, label)}</Fragment>
              ))}
            <th scope="col" data-column="select">
              <input
                type="checkbox"
                aria-label={l('selectAll')}
                aria-describedby={
                  occurrences && matches.length ? selectionHintId : undefined
                }
                checked={allOnPageSelected}
                ref={(checkbox) => {
                  if (checkbox)
                    checkbox.indeterminate =
                      someOnPageSelected && !allOnPageSelected;
                }}
                disabled={loading || !matches.length}
                onChange={(e) => onSelect(matches, e.target.checked)}
              />
              {columns.handle('select', l('selection'))}
            </th>
            {dataColumns.map((id) => (
              <Fragment key={id}>{headerCells[id]()}</Fragment>
            ))}
          </tr>
        </thead>
        <tbody
          onPointerOver={cellTooltip.onPointerOver}
          onPointerLeave={cellTooltip.onPointerLeave}
          onPointerDown={cellTooltip.onPointerDown}
        >
          {matches.map((m, index) => (
            <tr
              key={libraryRowKey(m)}
              id={`library-row-${libraryRowKey(m)}`}
              data-match-id={m.id}
              data-position-id={m.occurrence?.id}
              aria-description={summaries.get(m.id)}
              tabIndex={0}
              className={isCurrent(m) ? 'selected' : undefined}
              onClick={(event) => event.currentTarget.focus()}
              onDoubleClick={(event) => {
                if (!(event.target as HTMLElement).closest('button, a, input'))
                  onOpen(m);
              }}
              onKeyDown={(e) => rowKey(e, m, index)}
            >
              {occurrences && (
                <>
                  <td data-column="occurrence_database" data-tooltip-overflow>
                    {m.occurrence?.database_name ??
                      databases.find((d) => d.id === m.database_id)?.name ??
                      '—'}
                  </td>
                  <td data-column="occurrence_game">
                    {m.occurrence ? formatNumber(m.occurrence.game_no) : '—'}
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
                        disabled={openingDisabled(m)}
                        onClick={(event) => {
                          event.stopPropagation();
                          onOpen(m);
                        }}
                      >
                        {formatNumber(m.occurrence.move_no)}
                      </button>
                    ) : (
                      '—'
                    )}
                  </td>
                </>
              )}
              <td onClick={(e) => e.stopPropagation()}>
                <input
                  type="checkbox"
                  aria-label={l(m.occurrence ? 'selectMatch' : 'select', {
                    name: m.display_name,
                  })}
                  checked={selected.has(m.id)}
                  onChange={(e) => onSelect([m], e.target.checked)}
                  disabled={selected.size >= 100 && !selected.has(m.id)}
                />
              </td>
              {dataColumns.map((id) => (
                <Fragment key={id}>{bodyCells[id](m)}</Fragment>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {cellTooltip.tooltip}
    </div>
  );
}
