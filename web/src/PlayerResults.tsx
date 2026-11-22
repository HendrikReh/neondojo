import { useCallback, useEffect, useId, useState } from 'react';
import { request } from './shared/api/api';
import { cursorSearchParams } from './shared/api/cursor-search';
import type { components } from './generated-api';
import { useI18n } from './shared/i18n/i18n';
import LibrarySortHeading from './shared/ui/LibrarySortHeading';
import { usePlayerText } from './player-directory-messages';
import { useVirtualPlayerText } from './virtual-player-messages';
import { NumberedSearchPagination } from './shared/ui/SearchPagination';
import {
  SearchResultRow,
  SearchResultTable,
} from './shared/ui/SearchResultList';
import { useSearchPages, type SearchPage } from './shared/hooks/useSearchPages';

type Schema = components['schemas'];
type PlayerSort = Schema['PlayerSort'];
export type PlayerOrder = {
  sort: PlayerSort;
  direction: Schema['PlayerSortDirection'];
};
const NAME_ORDER: PlayerOrder = { sort: 'name', direction: 'asc' };

/** Names the platform of an alias; other sources need no label. */
export function aliasLabel(alias: Schema['PlayerAlias']) {
  return alias.platform ? `${alias.name} · ${alias.platform}` : alias.name;
}

export function usePlayerPage<T>(
  path: string,
  params: Record<string, string>,
  revision: number,
) {
  const scope = `${path}?${cursorSearchParams(50, params)}`;
  const loadPage = useCallback(
    (signal: AbortSignal, page: number) =>
      request<SearchPage<T>>(`${scope}&page=${page}`, { signal }),
    [scope],
  );
  return useSearchPages({ scope, revision, pageSize: 50, loadPage });
}

export function PlayerResults({
  search,
  excludeId = '',
  revision,
  selected,
  onSelect,
  reportTotal,
  quietWhenEmpty = false,
  initialOrder = NAME_ORDER,
  selectFirst = false,
}: {
  search: string;
  excludeId?: string;
  revision: number;
  selected?: string;
  onSelect: (id: string) => void;
  reportTotal?: (total: number) => void;
  /** Suggestions stay silent when nothing matches. */
  quietWhenEmpty?: boolean;
  initialOrder?: PlayerOrder;
  /** Shows the first result while no profile is selected. */
  selectFirst?: boolean;
}) {
  const t = usePlayerText();
  const v = useVirtualPlayerText();
  const { formatError, formatNumber } = useI18n();
  const [order, setOrder] = useState(initialOrder);
  const results = usePlayerPage<Schema['PlayerSearchHit']>(
    '/players',
    { q: search.trim(), exclude_id: excludeId, ...order },
    revision,
  );
  // Names start ascending; counts start with the most active players.
  const sortBy = (sort: PlayerSort) =>
    setOrder((current) =>
      current.sort === sort
        ? { sort, direction: current.direction === 'asc' ? 'desc' : 'asc' }
        : { sort, direction: sort === 'name' ? 'asc' : 'desc' },
    );
  const heading = (sort: PlayerSort, label: string) => (
    <LibrarySortHeading<PlayerSort>
      fields={[{ key: sort, label }]}
      sort={order.sort}
      direction={order.direction}
      onSort={sortBy}
    />
  );
  const prefix = useId();
  const ids = results.items.map((row) => row.id);
  useEffect(() => reportTotal?.(results.total), [results.total, reportTotal]);
  const first = selectFirst && !selected ? ids[0] : undefined;
  useEffect(() => {
    if (first) onSelect(first);
  }, [first, onSelect]);
  return (
    <>
      {results.error !== undefined && (
        <p role="alert">
          {formatError(results.error)}{' '}
          <button onClick={results.retry}>{t('retry')}</button>
        </p>
      )}
      {results.loading && <p role="status">{t('loading')}</p>}
      {!quietWhenEmpty && !results.loading && !results.error && !ids.length && (
        <p className="position-empty">{t('empty')}</p>
      )}
      {!!ids.length && (
        <SearchResultTable
          scrollClassName="player-results"
          className="position-table player-result-table"
          aria-label={t('title')}
          aria-busy={results.loading}
        >
          <thead>
            <tr>
              {heading('name', t('player'))}
              {heading('matches', t('matches'))}
              {heading('games', t('games'))}
            </tr>
          </thead>
          <tbody>
            {results.items.map((row, index) => (
              <SearchResultRow
                key={row.id}
                ids={ids}
                index={index}
                idPrefix={prefix}
                active={row.id === selected}
                onPreview={onSelect}
                jumpToEdges
              >
                <td>
                  <button
                    type="button"
                    aria-pressed={row.id === selected}
                    title={row.display_name}
                    onClick={(event) => {
                      event.stopPropagation();
                      onSelect(row.id);
                    }}
                  >
                    {row.display_name}
                  </button>
                  {row.name_platforms?.length ? (
                    <span
                      className="player-alias-hit player-name-platforms"
                      title={row.name_platforms.join(', ')}
                    >
                      <span className="visually-hidden">
                        {t('namePlatforms')}{' '}
                      </span>
                      {row.name_platforms.join(', ')}
                    </span>
                  ) : null}
                  {row.matching_alias && (
                    <span
                      className="player-alias-hit"
                      title={aliasLabel(row.matching_alias)}
                    >
                      <span className="visually-hidden">
                        {t('matchedAlias')}{' '}
                      </span>
                      {aliasLabel(row.matching_alias)}
                    </span>
                  )}
                  {row.virtual_profile && (
                    <span
                      className="player-group-hit"
                      title={row.virtual_profile.name}
                    >
                      <span className="visually-hidden">{v('partOf')} </span>
                      {row.virtual_profile.name}
                    </span>
                  )}
                </td>
                <td>{formatNumber(row.match_count)}</td>
                <td>{formatNumber(row.game_count)}</td>
              </SearchResultRow>
            ))}
          </tbody>
        </SearchResultTable>
      )}
      <NumberedSearchPagination
        page={results.page}
        lastPage={results.lastPage}
        busy={results.loading}
        onPageChange={results.setPage}
        label={t('title')}
      />
    </>
  );
}
