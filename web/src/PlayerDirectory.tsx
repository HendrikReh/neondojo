import { useEffect, useState } from 'react';
import { request, type Match } from './api';
import { NumberedSearchPagination } from './SearchPagination';
import { SearchResultList, SearchResultTable } from './SearchResultList';
import { SearchResultPreview } from './SearchResultPreview';
import { useI18n } from './i18n';
import type { Database, LibraryMatch } from './library-api';
import { usePlayerText } from './player-directory-messages';
import { useLibraryText } from './library-messages';
import { usePositionText } from './position-browser-messages';
import { PlayerResults, usePlayerPage } from './PlayerResults';
import {
  PlayerEditDialog,
  PlayerMergeDialog,
  usePlayerDetails,
} from './PlayerProfileDialogs';
import './player-directory.css';

function PlayerMatches({
  id,
  databases,
  onOpen,
  revision,
}: {
  id: string;
  databases: Database[];
  onOpen: (match: Match) => void;
  revision: number;
}) {
  const t = usePlayerText();
  const p = usePositionText();
  const l = useLibraryText();
  const { formatError, formatNumber, locale, t: ui } = useI18n();
  const [database, setDatabase] = useState('');
  const databaseNames = new Map(databases.map((row) => [row.id, row.name]));
  const databaseId = databaseNames.has(database) ? database : '';
  const { items, page, lastPage, setPage, loading, error, retry, total } =
    usePlayerPage<LibraryMatch>(
      '/library/matches',
      {
        player_id: id,
        database_id: databaseId,
        sort: 'played_at',
      },
      revision,
    );
  const dateFormat = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeZone: 'UTC',
  });

  return (
    <section className="player-matches" aria-label={t('matches')}>
      <div className="player-matches-header">
        <h3>
          {t('matches')}
          {!loading && error === undefined && (
            <span>{formatNumber(total)}</span>
          )}
        </h3>
        <label>
          {t('database')}
          <select
            value={databaseId}
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
      </div>
      {error !== undefined && (
        <p role="alert">
          {formatError(error)} <button onClick={retry}>{t('retry')}</button>
        </p>
      )}
      {loading && !items.length && <p role="status">{t('loading')}</p>}
      {!loading && error === undefined && !items.length && (
        <p>{t('noMatches')}</p>
      )}
      {!!items.length && (
        <SearchResultTable
          scrollClassName="player-match-scroll"
          className="position-table player-match-table"
          aria-label={t('matches')}
          aria-busy={loading}
        >
          <thead>
            <tr>
              <th scope="col">{p('opponent')}</th>
              <th scope="col">{t('matchFormat')}</th>
              <th scope="col">{t('database')}</th>
              <th scope="col">{t('date')}</th>
              <th scope="col">{t('games')}</th>
            </tr>
          </thead>
          <tbody>
            {items.map((row) => {
              const sides = row.player_sides;
              const names = `${row.white} – ${row.black}`;
              let opponent = names;
              if (sides?.length === 1)
                opponent = sides[0] === 0 ? row.black : row.white;
              if (sides?.length === 2) opponent = t('bothSides');
              const event = [
                row.tournament,
                row.round && `${l('round')} ${row.round}`,
              ]
                .filter(Boolean)
                .join(' · ');
              const databaseName = databaseNames.get(row.database_id);
              return (
                <tr key={row.id}>
                  <td>
                    <button
                      type="button"
                      className="position-preview-link player-match-link"
                      onClick={() => onOpen(row)}
                      title={names}
                    >
                      {opponent}
                    </button>
                    {event && (
                      <span className="player-match-event" title={event}>
                        {event}
                      </span>
                    )}
                  </td>
                  <td>
                    {row.match_length > 0
                      ? ui('app.matchLength', { count: row.match_length })
                      : ui('common.moneyGame')}
                  </td>
                  <td title={databaseName}>{databaseName ?? '—'}</td>
                  <td>
                    {row.played_on
                      ? dateFormat.format(new Date(row.played_on))
                      : '—'}
                  </td>
                  <td>{formatNumber(row.game_count)}</td>
                </tr>
              );
            })}
          </tbody>
        </SearchResultTable>
      )}
      <NumberedSearchPagination
        page={page}
        lastPage={lastPage}
        busy={loading}
        onPageChange={setPage}
        label={t('matches')}
      />
    </section>
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
  const { details, setDetails, error, retry } = usePlayerDetails(id, revision);
  const [dialog, setDialog] = useState<'edit' | 'merge'>();
  const [matchesRevision, setMatchesRevision] = useState(0);
  const [merged, setMerged] = useState(false);

  if (!details)
    return error ? (
      <p role="alert">
        {formatError(error)} <button onClick={retry}>{t('retry')}</button>
      </p>
    ) : (
      <p role="status">{t('loading')}</p>
    );

  return (
    <section className="player-card" aria-label={details.display_name}>
      <header className="player-card-header">
        <div>
          <h2>{details.display_name}</h2>
          <p>
            {t('counts', {
              matches: formatNumber(details.match_count),
              games: formatNumber(details.game_count),
            })}
          </p>
        </div>
        <div className="player-profile-actions">
          <button type="button" onClick={() => setDialog('edit')}>
            {t('edit')}
          </button>
          <button
            type="button"
            onClick={() => {
              setMerged(false);
              setDialog('merge');
            }}
          >
            {t('merge')}
          </button>
        </div>
      </header>
      {merged && (
        <p className="player-merge-status" role="status">
          {t('merged')}
        </p>
      )}
      {error !== undefined && (
        <p role="alert">
          {formatError(error)} <button onClick={retry}>{t('retry')}</button>
        </p>
      )}
      <PlayerMatches
        id={id}
        databases={databases}
        onOpen={onOpen}
        revision={revision + matchesRevision}
      />
      <section className="player-ratings" aria-label={t('pr')}>
        <h3>{t('pr')}</h3>
        {details.ratings.length ? (
          <ul>
            {details.ratings.map((rating) => (
              <li key={rating.profile}>
                <strong>
                  {formatNumber(rating.value, { maximumFractionDigits: 2 })}
                </strong>
                {' · '}
                {t('coverage', {
                  rated: formatNumber(rating.match_count),
                  total: formatNumber(details.match_count),
                })}
                <span className="player-rating-profile">{rating.profile}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p>{t('noPr')}</p>
        )}
      </section>
      {dialog === 'edit' && (
        <PlayerEditDialog
          profile={details}
          onClose={() => setDialog(undefined)}
          onChanged={(profile) => {
            setDetails(profile);
            onChanged();
          }}
        />
      )}
      {dialog === 'merge' && (
        <PlayerMergeDialog
          profile={details}
          revision={revision}
          onClose={() => setDialog(undefined)}
          onRefreshed={setDetails}
          onMerged={(profile) => {
            setDetails(profile);
            onChanged();
            setMatchesRevision((n) => n + 1);
            setMerged(true);
            setDialog(undefined);
          }}
        />
      )}
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
  const { formatError, formatNumber } = useI18n();
  const [total, setTotal] = useState(0);
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
    <section
      className="player-directory position-browser"
      aria-label={t('title')}
    >
      <div className="position-browser-heading">
        <h1>{t('title')}</h1>
      </div>
      <label className="player-search">
        <span>{t('search')}</span>
        <input
          type="search"
          maxLength={240}
          placeholder={t('searchHint')}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </label>
      {error !== undefined && <p role="alert">{formatError(error)}</p>}
      <SearchResultList
        listClassName="player-list"
        count={<span>{t('playerCount', { count: formatNumber(total) })}</span>}
        preview={
          <SearchResultPreview
            className="player-preview"
            title={t('profile')}
            hint={t('choose')}
          >
            {id ? (
              <PlayerCard
                key={id}
                id={id}
                databases={databases}
                revision={revision}
                onOpen={onOpen}
                onChanged={() => setListRevision((n) => n + 1)}
              />
            ) : null}
          </SearchResultPreview>
        }
      >
        <PlayerResults
          search={search}
          revision={revision + listRevision}
          selected={id}
          onSelect={setId}
          reportTotal={setTotal}
        />
      </SearchResultList>
    </section>
  );
}
