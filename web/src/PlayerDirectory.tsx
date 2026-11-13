import { useEffect, useState } from 'react';
import { request, type Match, type Page } from './api';
import { cursorSearchParams, mergeSearchItems } from './cursor-search';
import type { components } from './generated-api';
import { useI18n } from './i18n';
import type { Database, LibraryMatch } from './library-api';
import { usePlayerText } from './player-directory-messages';
import './player-directory.css';

type Schema = components['schemas'];
type Profile = Schema['PlayerProfile'];
type Details = Schema['PlayerDetails'];

function usePage<T extends { id: string }>(
  path: string,
  params: Record<string, string>,
  revision: number,
) {
  const search = cursorSearchParams(50, params).toString();
  const [after, setAfter] = useState<string>();
  const [page, setPage] = useState<Page<T>>();
  const [error, setError] = useState<unknown>();
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    const abort = new AbortController();
    const query = new URLSearchParams(search);
    if (after) query.set('after', after);
    setError(undefined);
    request<Page<T>>(`${path}?${query}`, { signal: abort.signal }).then(
      (result) => {
        if (!abort.signal.aborted)
          setPage((prev) => ({
            items: after
              ? mergeSearchItems(
                  prev?.items ?? [],
                  result.items,
                  (row) => row.id,
                )
              : result.items,
            next_cursor: result.next_cursor,
          }));
      },
      (err: unknown) => {
        if (!abort.signal.aborted) setError(err);
      },
    );
    return () => abort.abort();
  }, [path, search, after, revision, retry]);

  return {
    page,
    error,
    retry: () => setRetry((n) => n + 1),
    more: () => setAfter(page?.next_cursor ?? undefined),
  };
}

function PlayerResults({
  search,
  revision,
  selected,
  onSelect,
}: {
  search: string;
  revision: number;
  selected?: string;
  onSelect: (id: string) => void;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const { page, error, retry, more } = usePage<Profile>(
    '/players',
    { q: search },
    revision,
  );
  if (error)
    return (
      <p role="alert">
        {formatError(error)} <button onClick={retry}>{t('retry')}</button>
      </p>
    );
  if (!page) return <p role="status">{t('loading')}</p>;
  return (
    <>
      {page.items.length === 0 && <p>{t('empty')}</p>}
      <ul className="player-results">
        {page.items.map((row) => (
          <li key={row.id}>
            <button
              type="button"
              aria-pressed={row.id === selected}
              onClick={() => onSelect(row.id)}
            >
              {row.display_name}
              <span>
                {formatNumber(row.match_count)} {t('matches')}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {page.next_cursor && <button onClick={more}>{t('more')}</button>}
    </>
  );
}

function PlayerMatches({
  id,
  database,
  databases,
  onOpen,
  revision,
}: {
  id: string;
  database: string;
  databases: Database[];
  onOpen: (match: Match) => void;
  revision: number;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber, locale } = useI18n();
  const { page, error, retry, more } = usePage<LibraryMatch>(
    '/library/matches',
    { player_id: id, database_id: database, sort: 'played_at' },
    revision,
  );
  if (error)
    return (
      <p role="alert">
        {formatError(error)} <button onClick={retry}>{t('retry')}</button>
      </p>
    );
  if (!page) return <p role="status">{t('loading')}</p>;
  if (!page.items.length) return <p>{t('noMatches')}</p>;
  return (
    <div className="player-match-table">
      <table className="database-table">
        <thead>
          <tr>
            <th scope="col">{t('matches')}</th>
            <th scope="col">{t('database')}</th>
            <th scope="col">{t('date')}</th>
            <th scope="col">{t('games')}</th>
          </tr>
        </thead>
        <tbody>
          {page.items.map((row) => (
            <tr key={row.id}>
              <td>
                <button type="button" onClick={() => onOpen(row)}>
                  {row.white} – {row.black}
                </button>
              </td>
              <td>
                {databases.find((item) => item.id === row.database_id)?.name ??
                  '—'}
              </td>
              <td>
                {row.played_on
                  ? new Intl.DateTimeFormat(locale, {
                      dateStyle: 'medium',
                      timeZone: 'UTC',
                    }).format(new Date(row.played_on))
                  : '—'}
              </td>
              <td>{formatNumber(row.game_count)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {page.next_cursor && <button onClick={more}>{t('more')}</button>}
    </div>
  );
}

function PlayerCard({
  id,
  databases,
  revision,
  onChanged,
  onOpen,
}: {
  id: string;
  databases: Database[];
  revision: number;
  onChanged: () => void;
  onOpen: (match: Match) => void;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const [details, setDetails] = useState<Details>();
  const [first, setFirst] = useState('');
  const [last, setLast] = useState('');
  const [alias, setAlias] = useState('');
  const [platform, setPlatform] = useState('');
  const [database, setDatabase] = useState('');
  const [mergeSearch, setMergeSearch] = useState('');
  const [candidate, setCandidate] = useState<Details>();
  const [sourceId, setSourceId] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>();
  const [retry, setRetry] = useState(0);
  const [matchesRevision, setMatchesRevision] = useState(0);

  useEffect(() => {
    const abort = new AbortController();
    setError(undefined);
    request<Details>(`/players/${id}`, { signal: abort.signal }).then(
      (row) => {
        if (!abort.signal.aborted) {
          setDetails(row);
          setFirst(row.first_name);
          setLast(row.last_name);
        }
      },
      (err: unknown) => {
        if (!abort.signal.aborted) setError(err);
      },
    );
    return () => abort.abort();
  }, [id, retry]);

  useEffect(() => {
    if (!sourceId) return;
    const abort = new AbortController();
    request<Details>(`/players/${sourceId}`, { signal: abort.signal }).then(
      (row) => {
        if (!abort.signal.aborted) setCandidate(row);
      },
      (err: unknown) => {
        if (!abort.signal.aborted) setError(err);
      },
    );
    return () => abort.abort();
  }, [sourceId]);

  async function write(path: string, method: string, body: object) {
    if (!details || busy) return false;
    setBusy(true);
    setError(undefined);
    setSaved(false);
    try {
      setDetails(
        await request<Details>(`/players/${id}${path}`, {
          method,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ...body, version: details.version }),
        }),
      );
      onChanged();
      return true;
    } catch (err) {
      setError(err);
      return false;
    } finally {
      setBusy(false);
    }
  }

  if (!details)
    return error ? (
      <p role="alert">
        {formatError(error)}{' '}
        <button onClick={() => setRetry((n) => n + 1)}>{t('retry')}</button>
      </p>
    ) : (
      <p role="status">{t('loading')}</p>
    );

  return (
    <section className="player-card" aria-label={details.display_name}>
      <h2>{details.display_name}</h2>
      <p>
        {formatNumber(details.match_count)} {t('matches')} ·{' '}
        {formatNumber(details.game_count)} {t('games')}
      </p>
      <form
        className="player-form"
        onSubmit={async (event) => {
          event.preventDefault();
          setSaved(
            await write('', 'PUT', {
              first_name: first,
              last_name: last,
            } satisfies Omit<Schema['PlayerNameWrite'], 'version'>),
          );
        }}
      >
        <label>
          {t('first')}
          <input
            value={first}
            maxLength={120}
            onChange={(event) => setFirst(event.target.value)}
          />
        </label>
        <label>
          {t('last')}
          <input
            value={last}
            maxLength={120}
            onChange={(event) => setLast(event.target.value)}
          />
        </label>
        <button
          disabled={
            busy || (first === details.first_name && last === details.last_name)
          }
        >
          {t('save')}
        </button>
      </form>
      {saved && <p role="status">{t('saved')}</p>}
      {error !== undefined && (
        <p role="alert">
          {formatError(error)}{' '}
          <button onClick={() => setRetry((n) => n + 1)}>{t('retry')}</button>
        </p>
      )}
      <h3>{t('pr')}</h3>
      {details.ratings.length ? (
        <ul>
          {details.ratings.map((rating) => (
            <li key={rating.profile}>
              <strong>
                {formatNumber(rating.value, { maximumFractionDigits: 2 })}
              </strong>{' '}
              · {rating.profile} · {formatNumber(rating.match_count)}{' '}
              {t('matches')}
            </li>
          ))}
        </ul>
      ) : (
        <p>{t('noPr')}</p>
      )}
      <details>
        <summary>{t('aliases')}</summary>
        <ul>
          {details.aliases.map((item) => (
            <li key={`${item.platform}:${item.name}`}>
              {item.name} · {item.platform || t('local')}
            </li>
          ))}
        </ul>
        <form
          className="player-form"
          onSubmit={async (event) => {
            event.preventDefault();
            if (
              await write('/aliases', 'POST', {
                name: alias,
                platform,
              } satisfies Omit<Schema['PlayerAliasWrite'], 'version'>)
            )
              setAlias('');
          }}
        >
          <label>
            {t('source')}
            <select
              value={platform}
              onChange={(event) => setPlatform(event.target.value)}
            >
              <option value="">{t('local')}</option>
              <option>backgammongalaxy.com</option>
              <option>opengammon.com</option>
            </select>
          </label>
          <label>
            {t('alias')}
            <input
              value={alias}
              maxLength={1024}
              onChange={(event) => setAlias(event.target.value)}
              required
            />
          </label>
          <button disabled={busy || !alias.trim()}>{t('add')}</button>
        </form>
      </details>
      <details>
        <summary>{t('merge')}</summary>
        <label>
          {t('mergeWith')}
          <input
            type="search"
            value={mergeSearch}
            onChange={(event) => {
              setMergeSearch(event.target.value);
              setCandidate(undefined);
              setSourceId(undefined);
            }}
          />
        </label>
        {mergeSearch.trim() && (
          <PlayerResults
            key={`${mergeSearch}:${revision}`}
            search={mergeSearch}
            revision={revision}
            selected={sourceId}
            onSelect={(id) => {
              setCandidate(undefined);
              setSourceId(id);
            }}
          />
        )}
        {candidate && candidate.id !== id && (
          <>
            <p>
              <strong>{candidate.display_name}</strong> ·{' '}
              {candidate.aliases
                .map(
                  (alias) => `${alias.name} (${alias.platform || t('local')})`,
                )
                .join(', ')}
            </p>
            <p>{t('mergeHint')}</p>
            <button
              disabled={busy}
              onClick={async () => {
                if (
                  await write('/merge', 'POST', {
                    source_id: candidate.id,
                    source_version: candidate.version,
                  } satisfies Omit<Schema['PlayerMerge'], 'version'>)
                ) {
                  setCandidate(undefined);
                  setSourceId(undefined);
                  setMergeSearch('');
                  setMatchesRevision((n) => n + 1);
                }
              }}
            >
              {t('merge')}
            </button>
          </>
        )}
      </details>
      <h3>{t('matches')}</h3>
      <label>
        {t('database')}
        <select
          value={database}
          onChange={(event) => setDatabase(event.target.value)}
        >
          <option value="">{t('all')}</option>
          {databases.map((row) => (
            <option key={row.id} value={row.id}>
              {row.name}
            </option>
          ))}
        </select>
      </label>
      <PlayerMatches
        key={`${database}:${matchesRevision}:${revision}`}
        id={id}
        database={database}
        databases={databases}
        onOpen={onOpen}
        revision={revision}
      />
    </section>
  );
}

export default function PlayerDirectory({
  revision,
  onOpen,
}: {
  revision: number;
  onOpen: (match: Match) => void;
}) {
  const t = usePlayerText();
  const { formatError } = useI18n();
  const [search, setSearch] = useState('');
  const [id, setId] = useState<string>();
  const [databases, setDatabases] = useState<Database[]>([]);
  const [error, setError] = useState<unknown>();
  const [listRevision, setListRevision] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    setError(undefined);
    request<Database[]>('/databases', { signal: abort.signal }).then(
      (items) => {
        if (!abort.signal.aborted) setDatabases(items);
      },
      (err: unknown) => {
        if (!abort.signal.aborted) setError(err);
      },
    );
    return () => abort.abort();
  }, [revision]);
  return (
    <div className="player-directory">
      <section className="player-list" aria-label={t('title')}>
        <h2>{t('title')}</h2>
        <label>
          {t('search')}
          <input
            type="search"
            maxLength={240}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </label>
        <PlayerResults
          key={`${search}:${revision}:${listRevision}`}
          search={search}
          revision={revision}
          selected={id}
          onSelect={setId}
        />
      </section>
      {error !== undefined && <p role="alert">{formatError(error)}</p>}
      {id && (
        <PlayerCard
          key={id}
          id={id}
          databases={databases}
          revision={revision}
          onOpen={onOpen}
          onChanged={() => setListRevision((n) => n + 1)}
        />
      )}
    </div>
  );
}
