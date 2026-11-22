import { useEffect, useRef, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import { request, type Match } from './shared/api/api';
import { NumberedSearchPagination } from './shared/ui/SearchPagination';
import { SearchResultTable } from './shared/ui/SearchResultList';
import { useI18n } from './shared/i18n/i18n';
import { cursorSearchParams } from './shared/api/cursor-search';
import type { components } from './generated-api';
import type { Database, LibraryMatch } from './library-api';
import { canAnalyzeMatch } from './library-match-summary';
import { usePlayerText } from './player-directory-messages';
import { useLibraryText } from './library-messages';
import { usePositionText } from './workspaces/positions/position-browser-messages';
import { usePlayerPage } from './PlayerResults';
import { prDigits } from './shared/format/pr-format';
import PlayerRatingTrend from './PlayerRatingTrend';

type Schema = components['schemas'];

/** A real player profile or a virtual profile that groups several. */
export type PlayerScope = { player_id: string } | { virtual_player_id: string };

/** Analysis depth of a profile such as "GNU Backgammon 1.08; 2-ply cubeful". */
export function profileDepth(profile: string) {
  return profile.slice(profile.lastIndexOf(';') + 1).trim() || profile;
}

export type AnalyzeMatches = (
  matches: LibraryMatch[],
  unavailableCount: number,
) => void;
const ANALYSIS_BATCH_LIMIT = 2000;

/** A bot match played with learning aids never enters the PR. */
export function canEnterRating(
  match: Pick<LibraryMatch, 'play_mode' | 'play_assisted'>,
) {
  return (
    match.play_mode == null ||
    (match.play_mode === 'competition' && !match.play_assisted)
  );
}

/** Every match of a player that can enter the PR but has neither a PR for it nor a pending or finished analysis. */
export async function unratedMatches(scope: PlayerScope) {
  const matches: LibraryMatch[] = [];
  let after: string | undefined;
  do {
    const page = await request<{
      items: LibraryMatch[];
      next_cursor: string | null;
    }>(
      `/library/matches?${cursorSearchParams(200, { ...scope, sort: 'played_at' }, after)}`,
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
      canEnterRating(match) &&
      !['queued', 'running', 'completed'].includes(match.analysis_status ?? '')
    );
  });
}

export function PlayerMatches({
  scope,
  databases,
  onOpen,
  onProfile,
  revision,
}: {
  scope: PlayerScope;
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
        ...scope,
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
          {formatError(error)}{' '}
          <ActionButton
            icon="refresh"
            label={t('retry')}
            compact
            type="button"
            onClick={retry}
          />
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

/** The PR per analysis profile, with the action that analyzes the rest. */
export function PlayerRatings({
  scope,
  ratings,
  matchCount,
  onAnalyzeMatches,
}: {
  scope: PlayerScope;
  ratings: Schema['PlayerRating'][];
  /** The matches that can enter the PR. */
  matchCount: number;
  onAnalyzeMatches?: AnalyzeMatches;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
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
      const unrated = await unratedMatches(scope);
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

  const rated = Math.max(0, ...ratings.map((rating) => rating.match_count));
  return (
    <section className="player-ratings" aria-label={t('pr')}>
      <h3>{t('pr')}</h3>
      {ratings.length ? (
        <ul>
          {ratings.map((rating) => (
            <li key={rating.profile}>
              <strong>{formatNumber(rating.value, prDigits)}</strong>
              <span>
                {t('coverage', {
                  rated: rating.match_count,
                  total: matchCount,
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
      {onAnalyzeMatches && rated < matchCount && (
        <ActionButton
          icon="analysis"
          type="button"
          label={collecting ? t('loading') : t('analyzeRest')}
          aria-haspopup="dialog"
          disabled={collecting}
          onClick={() => void analyzeRest()}
        />
      )}
      {collected &&
        (collected.error === undefined ? (
          <p role="status">{t('nothingToAnalyze')}</p>
        ) : (
          <p role="alert">{formatError(collected.error)}</p>
        ))}
    </section>
  );
}

/** One PR trend per analysis profile with at least two rated matches. */
export function PlayerRatingTrends({
  ratings,
  history,
}: {
  ratings: Schema['PlayerRating'][];
  history: Schema['PlayerRatingPoint'][];
}) {
  const t = usePlayerText();
  const trends = ratings
    .map((rating) => ({
      profile: rating.profile,
      points: history.filter((point) => point.profile === rating.profile),
    }))
    .filter((trend) => trend.points.length > 1);
  if (!trends.length) return null;
  return (
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
  );
}
