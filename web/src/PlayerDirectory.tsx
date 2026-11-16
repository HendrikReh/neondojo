import { useEffect, useRef, useState } from 'react';
import { request, type Match } from './api';
import { NumberedSearchPagination } from './SearchPagination';
import { SearchResultList, SearchResultTable } from './SearchResultList';
import { SearchResultPreview } from './SearchResultPreview';
import { useI18n } from './i18n';
import { cursorSearchParams } from './cursor-search';
import type { Database, LibraryMatch } from './library-api';
import { canAnalyzeMatch } from './library-match-summary';
import { usePlayerText } from './player-directory-messages';
import { useLibraryText } from './library-messages';
import { usePositionText } from './position-browser-messages';
import {
  aliasLabel,
  PlayerResults,
  usePlayerPage,
  type PlayerOrder,
} from './PlayerResults';
import {
  PlayerEditDialog,
  PlayerMergeDialog,
  usePlayerDetails,
} from './PlayerProfileDialogs';
import './player-directory.css';
import { prDigits } from './pr-format';
import PlayerRatingTrend from './PlayerRatingTrend';

// The most active player leads the list, usually the owner of the library.
const DIRECTORY_ORDER: PlayerOrder = { sort: 'matches', direction: 'desc' };

/** Analysis depth of a profile such as "GNU Backgammon 1.08; 2-ply cubeful". */
function profileDepth(profile: string) {
  return profile.slice(profile.lastIndexOf(';') + 1).trim() || profile;
}

type AnalyzeMatches = (
  matches: LibraryMatch[],
  unavailableCount: number,
) => void;
const ANALYSIS_BATCH_LIMIT = 2000;

/** Every match of a profile that has neither a PR for it nor a pending or finished analysis. */
async function unratedMatches(id: string) {
  const matches: LibraryMatch[] = [];
  let after: string | undefined;
  do {
    const page = await request<{
      items: LibraryMatch[];
      next_cursor: string | null;
    }>(
      `/library/matches?${cursorSearchParams(200, { player_id: id, sort: 'played_at' }, after)}`,
    );
    matches.push(...page.items);
    after = page.next_cursor ?? undefined;
  } while (after && matches.length < ANALYSIS_BATCH_LIMIT);
  return matches.filter((match) => {
    const sides = match.player_sides ?? [0, 1];
    const rated = sides.every(
      (side) => (side === 1 ? match.black_pr : match.white_pr) != null,
    );
    return (
      !rated &&
      !['queued', 'running', 'completed'].includes(match.analysis_status ?? '')
    );
  });
}

function PlayerMatches({
  id,
  databases,
  onOpen,
  onProfile,
  revision,
}: {
  id: string;
  databases: Database[];
  onOpen: (match: Match) => void;
  onProfile: (id: string) => void;
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
              <th scope="col">{l('result')}</th>
              <th scope="col">{t('prColumn')}</th>
              <th scope="col">{t('matchFormat')}</th>
              <th scope="col">{l('tournament')}</th>
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
              // Own points first when the profile played one side.
              const black = sides?.length === 1 && sides[0] === 1;
              const scores = black
                ? [row.black_score, row.white_score]
                : [row.white_score, row.black_score];
              const [own, other] = scores;
              const rating =
                sides?.length === 1
                  ? black
                    ? row.black_pr
                    : row.white_pr
                  : null;
              const opponentId = row.opponent_player_id;
              return (
                <tr
                  key={row.id}
                  tabIndex={0}
                  title={`${t('openMatch')}: ${names}`}
                  onClick={() => onOpen(row)}
                  onKeyDown={(event) => {
                    if (
                      event.target !== event.currentTarget ||
                      event.key !== 'Enter'
                    )
                      return;
                    event.preventDefault();
                    onOpen(row);
                  }}
                >
                  <td>
                    {opponentId ? (
                      <button
                        type="button"
                        className="position-preview-link player-match-link"
                        title={t('openProfile')}
                        onClick={(event) => {
                          event.stopPropagation();
                          onProfile(opponentId);
                        }}
                      >
                        {opponent}
                      </button>
                    ) : (
                      <span className="player-match-opponent">{opponent}</span>
                    )}
                  </td>
                  <td>
                    {own == null || other == null
                      ? '—'
                      : `${formatNumber(own)}–${formatNumber(other)}`}
                  </td>
                  <td
                    title={
                      rating == null ? undefined : (row.pr_profile ?? undefined)
                    }
                  >
                    {rating == null ? '—' : formatNumber(rating, prDigits)}
                  </td>
                  <td>
                    {row.match_length > 0
                      ? ui('app.matchLength', { count: row.match_length })
                      : ui('common.moneyGame')}
                  </td>
                  <td title={event || undefined}>{event || '—'}</td>
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
  onProfile,
  onAnalyzeMatches,
}: {
  id: string;
  databases: Database[];
  revision: number;
  onChanged: () => void;
  onOpen: (match: Match) => void;
  onProfile: (id: string) => void;
  onAnalyzeMatches?: AnalyzeMatches;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const { details, setDetails, error, retry } = usePlayerDetails(id, revision);
  const [dialog, setDialog] = useState<'edit' | 'merge'>();
  const [matchesRevision, setMatchesRevision] = useState(0);
  const [merged, setMerged] = useState(false);
  const [collecting, setCollecting] = useState(false);
  const [collected, setCollected] = useState<{ error?: unknown }>();
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  async function analyzeRest() {
    setCollecting(true);
    setCollected(undefined);
    try {
      const unrated = await unratedMatches(id);
      if (!mounted.current) return;
      const matches = unrated.filter(canAnalyzeMatch);
      if (matches.length)
        onAnalyzeMatches?.(matches, unrated.length - matches.length);
      else setCollected({});
    } catch (error) {
      if (mounted.current) setCollected({ error });
    } finally {
      if (mounted.current) setCollecting(false);
    }
  }

  if (!details)
    return error ? (
      <p role="alert">
        {formatError(error)} <button onClick={retry}>{t('retry')}</button>
      </p>
    ) : (
      <p role="status">{t('loading')}</p>
    );

  const trends = details.ratings
    .map((rating) => ({
      profile: rating.profile,
      points: (details.rating_history ?? []).filter(
        (point) => point.profile === rating.profile,
      ),
    }))
    .filter((trend) => trend.points.length > 1);

  const rated = Math.max(
    0,
    ...details.ratings.map((rating) => rating.match_count),
  );
  const aliasNames = [
    ...new Set(details.aliases.map((alias) => alias.name)),
  ].filter((name) => name !== details.display_name);

  return (
    <section className="player-card" aria-label={details.display_name}>
      <header className="player-card-header">
        <div>
          <h2>{details.display_name}</h2>
          <p>
            {t('counts', {
              matches: details.match_count,
              games: details.game_count,
            })}
          </p>
          {aliasNames.length > 0 && (
            <p
              className="player-card-aliases"
              title={details.aliases
                .map((alias) => aliasLabel(alias))
                .join('\n')}
            >
              {t('aliases')}: {aliasNames.join(', ')}
            </p>
          )}
        </div>
        <div className="player-profile-actions">
          <button type="button" onClick={() => setDialog('edit')}>
            {t('editShort')}
          </button>
          <button
            type="button"
            onClick={() => {
              setMerged(false);
              setDialog('merge');
            }}
          >
            {t('mergeShort')}
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
      <section className="player-ratings" aria-label={t('pr')}>
        <h3>{t('pr')}</h3>
        {details.ratings.length ? (
          <ul>
            {details.ratings.map((rating) => (
              <li key={rating.profile}>
                <strong>{formatNumber(rating.value, prDigits)}</strong>
                <span>
                  {t('coverage', {
                    rated: rating.match_count,
                    total: details.match_count,
                  })}
                </span>
                <span className="player-rating-profile" title={rating.profile}>
                  {profileDepth(rating.profile)}
                </span>
              </li>
            ))}
          </ul>
        ) : (
          <p>{t('noPr')}</p>
        )}
        {onAnalyzeMatches && rated < details.match_count && (
          <button
            type="button"
            aria-haspopup="dialog"
            disabled={collecting}
            onClick={() => void analyzeRest()}
          >
            {collecting ? t('loading') : t('analyzeRest')}
          </button>
        )}
        {collected &&
          (collected.error === undefined ? (
            <p role="status">{t('nothingToAnalyze')}</p>
          ) : (
            <p role="alert">{formatError(collected.error)}</p>
          ))}
      </section>
      <PlayerMatches
        id={id}
        databases={databases}
        onOpen={onOpen}
        onProfile={onProfile}
        revision={revision + matchesRevision}
      />
      {trends.length > 0 && (
        <section className="player-rating-trends" aria-label={t('prTrend')}>
          {trends.map((trend) => (
            <div key={trend.profile}>
              {trends.length > 1 && (
                <span className="player-rating-profile">{trend.profile}</span>
              )}
              <PlayerRatingTrend points={trend.points} />
            </div>
          ))}
        </section>
      )}
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
  onAnalyzeMatches,
}: {
  revision: number;
  onOpen: (match: Match) => void;
  onAnalyzeMatches?: AnalyzeMatches;
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
        <input
          type="search"
          aria-label={t('search')}
          maxLength={240}
          placeholder={t('searchHint')}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </label>
      {error !== undefined && <p role="alert">{formatError(error)}</p>}
      <SearchResultList
        listClassName="player-list"
        title={t('player')}
        count={
          <span aria-label={t('playerCount', { count: total })}>
            {formatNumber(total)}
          </span>
        }
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
                onProfile={setId}
                onAnalyzeMatches={onAnalyzeMatches}
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
          initialOrder={DIRECTORY_ORDER}
          selectFirst
        />
      </SearchResultList>
    </section>
  );
}
