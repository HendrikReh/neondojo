import { useCallback, useEffect, useId } from 'react';
import { request } from './api';
import { cursorSearchParams } from './cursor-search';
import type { components } from './generated-api';
import { useI18n } from './i18n';
import { usePlayerText } from './player-directory-messages';
import { NumberedSearchPagination } from './SearchPagination';
import { SearchResultRow, SearchResultTable } from './SearchResultList';
import { useSearchPages, type SearchPage } from './useSearchPages';

type Schema = components['schemas'];

export function aliasLabel(alias: Schema['PlayerAlias'], local: string) {
  return `${alias.name} · ${alias.platform || local}`;
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
}: {
  search: string;
  excludeId?: string;
  revision: number;
  selected?: string;
  onSelect: (id: string) => void;
  reportTotal?: (total: number) => void;
  /** Suggestions stay silent when nothing matches. */
  quietWhenEmpty?: boolean;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const results = usePlayerPage<Schema['PlayerSearchHit']>(
    '/players',
    { q: search.trim(), exclude_id: excludeId },
    revision,
  );
  const prefix = useId();
  const ids = results.items.map((row) => row.id);
  useEffect(() => reportTotal?.(results.total), [results.total, reportTotal]);
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
              <th scope="col">{t('player')}</th>
              <th scope="col">{t('matches')}</th>
              <th scope="col">{t('games')}</th>
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
                  {row.matching_alias && (
                    <span
                      className="player-alias-hit"
                      title={aliasLabel(row.matching_alias, t('local'))}
                    >
                      <span className="visually-hidden">
                        {t('matchedAlias')}{' '}
                      </span>
                      {aliasLabel(row.matching_alias, t('local'))}
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
