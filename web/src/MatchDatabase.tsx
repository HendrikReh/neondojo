import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { request, type Match, type Page, type Position } from './api';
import AnnotationSearch, { type AnnotationHit } from './AnnotationSearch';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
import { useWorkspaceText } from './workspace-messages';
import { useLibraryText, type LibraryMessageKey } from './library-messages';
import {
  emptyFilters,
  loadLibrary,
  defaultDatabaseId,
  type Database,
  type LibraryMatch,
  type ActionResponse,
  type LibraryFilters as Filters,
} from './library-api';
import DatabaseSidebar from './DatabaseSidebar';
import LibraryFilters from './LibraryFilters';
import LibraryDialogs, { type LibraryDialogState } from './LibraryDialogs';
import MatchPreview from './MatchPreview';
import type { MoveNotesTarget } from './MoveNotesDialog';
import type { Theme } from './themes';
import './library.css';

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
  selectedMatchId,
  databaseId,
  onDatabase,
  onOpen,
  onPosition,
  onAnnotation,
  onImport,
  onChanged,
  onMoveNotes,
  theme,
  visible,
}: {
  revision: number;
  visible: boolean;
  annotationRevision: number;
  currentPosition?: Position;
  selectedMatchId?: string;
  databaseId: string;
  onDatabase: (id: string) => void;
  onOpen: (match: Match, gameNo?: number) => void;
  onPosition: (p: Position) => void;
  onAnnotation: (hit: AnnotationHit) => void;
  onImport: () => void;
  onChanged: (deletedIds?: string[], validateSelection?: boolean) => void;
  onMoveNotes: (target: MoveNotesTarget) => void;
  theme: Theme;
}) {
  const { t, locale, formatNumber, formatError } = useI18n(),
    w = useWorkspaceText(),
    l = useLibraryText();
  const sourceT = useSourceText();
  const [databases, setDatabases] = useState<Database[]>([]),
    [localRevision, setRevision] = useState(0),
    [filters, setFilters] = useState<Filters>(emptyFilters),
    [expanded, setExpanded] = useState(false),
    [matches, setMatches] = useState<LibraryMatch[]>([]),
    [cursor, setCursor] = useState<string | null>(null),
    [loading, setLoading] = useState(false),
    [error, setError] = useState<unknown>(),
    [sort, setSort] = useState('created_at'),
    [direction, setDirection] = useState('desc'),
    [previewId, setPreviewId] = useState<string>(),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [dialog, setDialog] = useState<LibraryDialogState>(),
    [result, setResult] = useState<ActionResponse>();
  const queryScope = useRef('');
  const queryVersion = useRef(0),
    moreRequest = useRef<AbortController | undefined>(undefined);
  const [previewSnapshot, setPreviewSnapshot] = useState<LibraryMatch>();
  const resultNames = useRef<Record<string, string>>({});
  const [exact, setExact] = useState(''),
    [found, setFound] = useState<Page<Position>>(),
    [positionLoading, setPositionLoading] = useState(false),
    [positionError, setPositionError] = useState<unknown>();
  const positionRequest = useRef<AbortController | undefined>(undefined);
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
        if (
          databaseCurrent.current &&
          !value.some((d) => d.id === databaseCurrent.current)
        )
          databaseCallback.current(defaultDatabaseId);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => abort.abort();
  }, [revision, localRevision]);
  useEffect(() => {
    setSelected(new Set());
    setPreviewId(undefined);
    setPreviewSnapshot(undefined);
    setFound(undefined);
    setResult(undefined);
    positionRequest.current?.abort();
    setPositionLoading(false);
  }, [databaseId, filters, sort, direction]);
  useEffect(() => {
    const abort = new AbortController(),
      version = ++queryVersion.current;
    moreRequest.current?.abort();
    setLoading(true);
    setError(undefined);
    const scope = JSON.stringify([databaseId, filters, sort, direction]);
    if (queryScope.current !== scope) {
      setCursor(null);
      setMatches([]);
      queryScope.current = scope;
    }
    const timer = setTimeout(() => {
      void loadLibrary(databaseId, filters, sort, direction, abort.signal)
        .then((page) => {
          if (version !== queryVersion.current || abort.signal.aborted) return;
          setMatches(page.items);
          setPreviewSnapshot(
            (previous) =>
              page.items.find((m) => m.id === previous?.id) ?? previous,
          );
          setCursor(page.next_cursor);
        })
        .catch((e) => {
          if (!abort.signal.aborted) setError(e);
        })
        .finally(() => {
          if (!abort.signal.aborted) setLoading(false);
        });
    }, 250);
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [
    databaseId,
    filters,
    sort,
    direction,
    revision,
    localRevision,
    annotationRevision,
    visible,
  ]);
  useEffect(
    () => () => {
      positionRequest.current?.abort();
      moreRequest.current?.abort();
    },
    [],
  );
  async function more() {
    if (!cursor || loading) return;
    moreRequest.current?.abort();
    const abort = new AbortController();
    moreRequest.current = abort;
    const version = queryVersion.current;
    setLoading(true);
    try {
      const page = await loadLibrary(
        databaseId,
        filters,
        sort,
        direction,
        abort.signal,
        cursor,
      );
      if (version === queryVersion.current && !abort.signal.aborted) {
        setMatches((previous) => {
          const ids = new Set(previous.map((m) => m.id));
          return [...previous, ...page.items.filter((m) => !ids.has(m.id))];
        });
        setCursor(page.next_cursor);
      }
    } catch (e) {
      if (version === queryVersion.current && !abort.signal.aborted)
        setError(e);
    } finally {
      if (version === queryVersion.current && !abort.signal.aborted)
        setLoading(false);
    }
  }
  async function searchPosition(more = false) {
    positionRequest.current?.abort();
    const abort = new AbortController();
    positionRequest.current = abort;
    setPositionLoading(true);
    setPositionError(undefined);
    try {
      const params = new URLSearchParams({
        position_id: exact.trim(),
        limit: '20',
      });
      if (databaseId) params.set('database_id', databaseId);
      if (more && found?.next_cursor) params.set('after', found.next_cursor);
      const page = await request<Page<Position>>(`/positions?${params}`, {
        signal: abort.signal,
      });
      if (!abort.signal.aborted)
        setFound((previous) =>
          more && previous
            ? { ...page, items: [...previous.items, ...page.items] }
            : page,
        );
    } catch (e) {
      if (!abort.signal.aborted) setPositionError(e);
    } finally {
      if (!abort.signal.aborted) setPositionLoading(false);
    }
  }
  function setPositionQuery(value: string) {
    positionRequest.current?.abort();
    setExact(value);
    setFound(undefined);
    setPositionLoading(false);
  }
  function changeDatabase(id: string) {
    onDatabase(id);
    setSelected(new Set());
    setMatches([]);
    setCursor(null);
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
    if (
      response.results.some(
        (r) =>
          r.match_id === previewId &&
          (r.status === 'deleted' || (r.status === 'moved' && databaseId)),
      )
    ) {
      setPreviewId(undefined);
      setPreviewSnapshot(undefined);
    }
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
  function changed(id?: string, databaseDeleted = false) {
    if (id !== undefined) changeDatabase(id || defaultDatabaseId);
    setRevision((n) => n + 1);
    onChanged([], databaseDeleted);
  }
  function sortBy(key: string) {
    if (sort === key) setDirection((d) => (d === 'asc' ? 'desc' : 'asc'));
    else {
      setSort(key);
      setDirection('asc');
    }
  }
  const heading = (key: string, label: string) => (
    <th
      aria-sort={
        sort === key
          ? direction === 'asc'
            ? 'ascending'
            : 'descending'
          : 'none'
      }
    >
      <button onClick={() => sortBy(key)}>
        {label}
        {sort === key ? (direction === 'asc' ? ' ↑' : ' ↓') : ''}
      </button>
    </th>
  );
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
      setPreviewId(matches[next].id);
      setPreviewSnapshot(matches[next]);
      document.getElementById(`library-row-${matches[next].id}`)?.focus();
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
  const preview = matches.find((m) => m.id === previewId) ?? previewSnapshot,
    active = databases.find((d) => d.id === databaseId);
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
    m.analysis_status === 'imported'
      ? sourceT('imported')
      : m.analysis_status
        ? l(
            (['completed', 'queued', 'running', 'failed', 'partial'].includes(
              m.analysis_status,
            )
              ? m.analysis_status
              : 'noAnalysis') as LibraryMessageKey,
          )
        : l('noAnalysis');
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
          <button className="primary" onClick={onImport}>
            {t('app.import')}
          </button>
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
              <span aria-hidden="true">⌕</span>
              <input
                aria-label={t('app.searchMatches')}
                placeholder={t('app.searchPlaceholder')}
                value={filters.q}
                onChange={(e) =>
                  setFilters((f) => ({ ...f, q: e.target.value }))
                }
              />
            </label>
            <button
              onClick={() => setExpanded((v) => !v)}
              aria-expanded={expanded}
            >
              {l('filters')}
            </button>
          </div>
          {expanded && (
            <LibraryFilters
              filters={filters}
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
          <div className="database-searches">
            <AnnotationSearch
              revision={annotationRevision}
              databaseId={databaseId}
              onOpen={onAnnotation}
            />
            <details className="position-search">
              <summary>
                ⌕ {t('app.findPosition')} <span>{t('app.positionId')}</span>
              </summary>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void searchPosition();
                }}
              >
                <input
                  aria-label={t('app.positionId')}
                  placeholder={t('app.positionPlaceholder')}
                  value={exact}
                  onChange={(e) => setPositionQuery(e.target.value)}
                />
                <button disabled={positionLoading || !exact.trim()}>
                  {t('app.exactSearch')}
                </button>
                {currentPosition && (
                  <button
                    type="button"
                    onClick={() =>
                      setPositionQuery(currentPosition.position_id)
                    }
                  >
                    {t('app.usePosition')}
                  </button>
                )}
              </form>
              {!!positionError && (
                <p role="alert">{formatError(positionError)}</p>
              )}
              {found && (
                <div className="position-results">
                  {!found.items.length ? (
                    <p>{t('app.noPositionMatches')}</p>
                  ) : (
                    found.items.map((p) => (
                      <button key={p.id} onClick={() => onPosition(p)}>
                        {t('app.positionResult', {
                          game: p.game_no,
                          move: p.ply,
                          dice: p.dice.join('–'),
                        })}{' '}
                        <strong>{p.notation}</strong>
                        <small>{p.match_uuid.slice(0, 8)}…</small>
                      </button>
                    ))
                  )}
                  {found.next_cursor && (
                    <button
                      disabled={positionLoading}
                      onClick={() => void searchPosition(true)}
                    >
                      {t('app.moreResults')}
                    </button>
                  )}
                </div>
              )}
            </details>
          </div>
          <div className="database-count">
            {loading
              ? t('app.searching')
              : `${formatNumber(matches.length)}${cursor ? '+' : ''}`}{' '}
            {t('app.matches')} · {active?.name ?? l('allDatabases')}
          </div>
          {selected.size > 0 && <p className="fineprint">{l('batchLimit')}</p>}
          {selected.size > 0 && (
            <div className="library-batch-bar">
              <strong>{l('selected', { count: selected.size })}</strong>
              {(['copy', 'move', 'tags', 'export', 'delete'] as const).map(
                (action) => (
                  <button
                    key={action}
                    onClick={() =>
                      setDialog({
                        kind: 'batch',
                        action,
                        ids: [...selected],
                        source: databaseId,
                      })
                    }
                  >
                    {l(action)}
                  </button>
                ),
              )}
              <button
                onClick={() => setSelected(new Set())}
                aria-label={l('clearSelection')}
              >
                ×
              </button>
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
                <table className="database-table">
                  <thead>
                    <tr>
                      <th>
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
                      </th>
                      {heading('name', t('app.matches'))}
                      {heading('white', l('white'))}
                      {heading('white_pr', 'PR (GNUbg)')}
                      {heading('black', l('black'))}
                      {heading('black_pr', 'PR (GNUbg)')}
                      {heading('match_length', l('length'))}
                      {heading('result', l('result'))}
                      {heading('played_at', l('played'))}
                      {heading('created_at', l('imported'))}
                      <th>{l('tournament')}</th>
                      <th>{l('analysis')}</th>
                      <th>
                        {l('tags')} / {l('comment')}
                      </th>
                      <th>{w('details')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {matches.map((m, index) => (
                      <tr
                        key={m.id}
                        id={`library-row-${m.id}`}
                        tabIndex={0}
                        className={`${m.id === selectedMatchId ? 'selected ' : ''}${previewId === m.id ? 'previewed' : ''}`}
                        onClick={() => {
                          setPreviewId(m.id);
                          setPreviewSnapshot(m);
                        }}
                        onDoubleClick={() => onOpen(m)}
                        onKeyDown={(e) => rowKey(e, m, index)}
                      >
                        <td onClick={(e) => e.stopPropagation()}>
                          <input
                            type="checkbox"
                            aria-label={l('select', { name: m.display_name })}
                            checked={selected.has(m.id)}
                            onChange={(e) => toggle(m.id, e.target.checked)}
                            disabled={
                              selected.size >= 100 && !selected.has(m.id)
                            }
                          />
                        </td>
                        <td>
                          <button
                            title={m.filename}
                            className={`match-card ${m.id === selectedMatchId ? 'active' : ''}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              onOpen(m);
                            }}
                          >
                            <span className="library-match-title">
                              {m.display_name}
                            </span>
                            <span className="sr-only">
                              {m.white} {t('common.versus')} {m.black}
                            </span>
                            <small>
                              {m.format.toUpperCase()} ·{' '}
                              {formatNumber(m.position_count)} {w('positions')}
                            </small>
                          </button>
                        </td>
                        <td>{m.white}</td>
                        <td title={m.pr_profile ?? l('prHelp')}>
                          {pr(m.white_pr)}
                        </td>
                        <td>{m.black}</td>
                        <td title={m.pr_profile ?? l('prHelp')}>
                          {pr(m.black_pr)}
                        </td>
                        <td>{m.match_length || l('money')}</td>
                        <td
                          title={l(
                            m.result_state as
                              'unknown' | 'ongoing' | 'completed',
                          )}
                        >
                          {m.white_score != null && m.black_score != null
                            ? `${m.white_score}–${m.black_score}`
                            : l('unknown')}
                        </td>
                        <td>{date(m.played_on)}</td>
                        <td>{date(m.created_at)}</td>
                        <td>{m.tournament ?? '—'}</td>
                        <td>{state(m)}</td>
                        <td className="library-notes-cell">
                          <div>
                            {m.tags.map((tag) => (
                              <span className="tag-chip" key={tag}>
                                {tag}
                              </span>
                            ))}
                          </div>
                          <small title={m.comment}>
                            {m.comment.slice(0, 150)}
                          </small>
                        </td>
                        <td>
                          <button
                            aria-label={`${w('details')}: ${m.filename}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              setPreviewId(m.id);
                              setPreviewSnapshot(m);
                            }}
                          >
                            {w('details')}
                          </button>
                          <a
                            href={`/api/matches/${m.id}/source`}
                            onClick={(e) => e.stopPropagation()}
                          >
                            SGF ↓
                          </a>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {!matches.length && !loading && (
                <p className="library-empty">{l('noMatches')}</p>
              )}
              {cursor && (
                <button
                  className="load-more"
                  disabled={loading}
                  onClick={() => void more()}
                >
                  {t('app.moreMatches')}
                </button>
              )}
              <p className="fineprint library-pr-help">{l('prHelp')}</p>
            </div>
            <MatchPreview
              match={preview}
              theme={theme}
              onOpen={onOpen}
              onEdit={(match) => setDialog({ kind: 'metadata', match })}
              onNotes={(move, match) =>
                onMoveNotes({ match, gameNo: move.game_no, moveId: move.id })
              }
            />
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
