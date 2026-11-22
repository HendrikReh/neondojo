import { useEffect, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useI18n } from '../../shared/i18n/i18n';
import {
  loadDuplicates,
  type Database,
  type DuplicateGroup,
  type LibraryMatch,
} from './library-api';
import { databaseDisplayName, useLibraryText } from './library-messages';
import { libraryAnalysisStatus } from './library-match-summary';

const SELECTION_LIMIT = 100;

interface Loaded {
  scope: string;
  groups: DuplicateGroup[];
  next?: string | null;
  totalGroups: number;
  totalMatches: number;
}

/**
 * Lists library matches with the same played record, grouped, for the
 * selected database or all databases. Selection is owned by the match table,
 * so the existing batch actions apply to the chosen copies.
 */
export default function DuplicateMatchFinder({
  open,
  panelId,
  databaseId,
  databases,
  refreshToken,
  selected,
  onSelect,
  onOpen,
  openingDisabled,
}: {
  open: boolean;
  panelId: string;
  databaseId: string;
  databases: Database[];
  refreshToken: number;
  selected: ReadonlySet<string>;
  onSelect: (rows: readonly LibraryMatch[], checked: boolean) => void;
  onOpen: (match: LibraryMatch) => void;
  openingDisabled: (match: LibraryMatch) => boolean;
}) {
  const l = useLibraryText(),
    { locale, formatError } = useI18n();
  const [searchToken, setSearchToken] = useState(0);
  const scope = JSON.stringify([databaseId, refreshToken, searchToken]);
  const [loaded, setLoaded] = useState<Loaded>();
  const [more, setMore] = useState<{ scope: string; after: string }>();
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>();
  useEffect(() => {
    if (!open) return;
    const abort = new AbortController();
    const after = more?.scope === scope ? more.after : undefined;
    setBusy(true);
    setError(undefined);
    void loadDuplicates(databaseId, after, abort.signal)
      .then((page) => {
        if (abort.signal.aborted) return;
        setLoaded((previous) => ({
          scope,
          groups:
            after && previous?.scope === scope
              ? [...previous.groups, ...page.groups]
              : page.groups,
          next: page.next_cursor,
          totalGroups: page.total_groups,
          totalMatches: page.total_matches,
        }));
      })
      .catch((e: unknown) => {
        if (!abort.signal.aborted) setError(e);
      })
      .finally(() => {
        if (!abort.signal.aborted) setBusy(false);
      });
    return () => abort.abort();
  }, [open, scope, databaseId, more]);
  const current = loaded?.scope === scope ? loaded : undefined;
  const groups = current?.groups ?? [];
  const copies = groups.flatMap((group) => group.items.slice(1));
  const databaseName = (id: string) => {
    const database = databases.find((d) => d.id === id);
    return database ? databaseDisplayName(database, l) : '—';
  };
  const date = (value: string) =>
    new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(
      new Date(value),
    );
  return (
    <div
      className="library-duplicates"
      id={panelId}
      hidden={!open}
      aria-busy={busy || undefined}
    >
      <div className="library-duplicates-toolbar">
        <h2>{l('duplicateFinder')}</h2>
        <p role="status">
          {!current
            ? busy
              ? l('loading')
              : ''
            : current.totalGroups
              ? l('duplicateSummary', {
                  groups: current.totalGroups,
                  matches: current.totalMatches,
                })
              : l('noDuplicates')}
        </p>
        <ActionButton
          icon="confirm"
          label={l('selectCopies')}
          tooltip={l('selectCopiesHelp')}
          type="button"
          disabled={
            !copies.length ||
            copies.every((match) => selected.has(match.id)) ||
            selected.size >= SELECTION_LIMIT
          }
          onClick={() => onSelect(copies, true)}
        />
        <ActionButton
          icon="refresh"
          label={l('searchAgain')}
          type="button"
          disabled={busy}
          onClick={() => setSearchToken((n) => n + 1)}
        />
      </div>
      <p className="fineprint">{l('duplicateHelp')}</p>
      {!!error && (
        <p className="error" role="alert">
          {formatError(error)}
        </p>
      )}
      {open &&
        groups.map((group, index) => (
          <section
            key={group.items[0]?.id ?? index}
            className="library-duplicate-group"
            aria-label={l('duplicateGroup', { number: index + 1 })}
          >
            <h3>
              <span className="library-duplicate-kind" data-kind={group.kind}>
                {l(
                  group.kind === 'identical_files'
                    ? 'duplicateIdenticalFiles'
                    : 'duplicateSameMoves',
                )}
              </span>{' '}
              · {l('duplicateGroupMatches', { count: group.match_count })}
              {group.match_count > group.items.length &&
                ` · ${l('duplicateListed', { count: group.items.length })}`}
            </h3>
            <div className="database-table-wrap">
              <table className="database-table library-duplicate-table">
                <thead>
                  <tr>
                    <th scope="col" data-column="select">
                      <span className="sr-only">{l('selection')}</span>
                    </th>
                    <th scope="col">ID</th>
                    <th scope="col">{l('white')}</th>
                    <th scope="col">{l('black')}</th>
                    <th scope="col">{l('name')}</th>
                    <th scope="col">{l('database')}</th>
                    <th scope="col">{l('format')}</th>
                    <th scope="col">{l('imported')}</th>
                    <th scope="col">{l('length')}</th>
                    <th scope="col">{l('analysis')}</th>
                  </tr>
                </thead>
                <tbody>
                  {group.items.map((match) => (
                    <tr
                      key={match.id}
                      aria-selected={selected.has(match.id)}
                      onDoubleClick={() => {
                        if (!openingDisabled(match)) onOpen(match);
                      }}
                    >
                      <td>
                        <input
                          type="checkbox"
                          aria-label={l('select', { name: match.display_name })}
                          checked={selected.has(match.id)}
                          disabled={
                            selected.size >= SELECTION_LIMIT &&
                            !selected.has(match.id)
                          }
                          onChange={(e) => onSelect([match], e.target.checked)}
                        />
                      </td>
                      <td className="library-id-cell">{match.match_no}</td>
                      <td>{match.white}</td>
                      <td>{match.black}</td>
                      <td className="library-duplicate-name">
                        <button
                          type="button"
                          className="match-card"
                          disabled={openingDisabled(match)}
                          onClick={() => onOpen(match)}
                        >
                          {match.display_name}
                        </button>
                      </td>
                      <td>{databaseName(match.database_id)}</td>
                      <td>{match.format.toUpperCase()}</td>
                      <td>{date(match.created_at)}</td>
                      <td>{match.match_length || l('money')}</td>
                      <td>
                        {libraryAnalysisStatus(match.analysis_status, locale)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        ))}
      {current?.next && (
        <ActionButton
          icon="expand"
          label={l('moreDuplicateGroups')}
          type="button"
          disabled={busy}
          onClick={() => {
            if (current.next) setMore({ scope, after: current.next });
          }}
        />
      )}
    </div>
  );
}
