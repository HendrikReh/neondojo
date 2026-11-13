import Tooltip from './Tooltip';
import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import { useCallback, useEffect, useId, useState, type FormEvent } from 'react';
import {
  normalizeAnnotationTag,
  searchAnnotations,
  searchValidation,
  type AnnotationFilters,
  type AnnotationHit,
  type AnnotationKind,
  type SearchValidation,
} from './annotation-api';
import SearchPagination from './SearchPagination';
import { useSearchPages } from './useSearchPages';
import { useLibraryText } from './library-messages';
import { useAnnotationI18n } from './annotation-messages';

export type { AnnotationHit } from './annotation-api';

export default function AnnotationSearch({
  revision = 0,
  databaseId,
  onOpen,
  panelId,
  open,
}: {
  revision?: number;
  databaseId?: string;
  onOpen: (hit: AnnotationHit) => void;
  panelId: string;
  open: boolean;
}) {
  const profile = useOptionalConfiguration();
  const { t, formatError } = useAnnotationI18n();
  const l = useLibraryText();
  const htmlId = useId();
  const [q, setQuery] = useState('');
  const [tag, setTag] = useState('');
  const [kind, setKind] = useState<AnnotationKind | ''>('');
  const [submitted, setSubmitted] = useProfileValue<AnnotationFilters | null>(
    (c) => c.annotations.filters,
    (c, filters) => ({ ...c, annotations: { ...c.annotations, filters } }),
    null,
  );
  useEffect(() => {
    if (!profile?.current) return;
    const filters = profile.current.annotations.filters;
    setQuery(filters?.q ?? '');
    setTag(filters?.tag ?? '');
    setKind(filters?.kind ?? '');
    setValidation(null);
  }, [profile?.generation, profile?.ready]);
  const [validation, setValidation] = useState<SearchValidation | null>(null);
  const [searchRun, setSearchRun] = useState(0);
  const loadPage = useCallback(
    (signal: AbortSignal, page: number) =>
      searchAnnotations(
        {
          q: '',
          tag: '',
          kind: '',
          ...submitted,
          database_id: submitted?.database_id ?? databaseId,
        },
        page,
        signal,
      ),
    [submitted, databaseId],
  );
  const results = useSearchPages({
    scope: JSON.stringify([submitted, databaseId, searchRun]),
    revision,
    pageSize: 20,
    enabled: !!submitted,
    loadPage,
  });
  const { items, loading, error } = results;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const issue = searchValidation(q, tag);
    setValidation(issue);
    if (issue) return;
    setSearchRun((run) => run + 1);
    setSubmitted({
      q: q.trim(),
      tag: normalizeAnnotationTag(tag),
      kind,
      ...(databaseId ? { database_id: databaseId } : {}),
    });
  };

  const findTag = (value: string) => {
    setQuery('');
    setSearchRun((run) => run + 1);
    setTag(value);
    setValidation(null);
    setSubmitted({
      q: '',
      tag: value,
      kind,
      ...(databaseId ? { database_id: databaseId } : {}),
    });
  };

  return (
    <div className="annotation-search" id={panelId} hidden={!open}>
      <form className="annotation-search-form" onSubmit={submit}>
        <label htmlFor={`${htmlId}-query`}>
          <span>{t('query')}</span>
          <input
            id={`${htmlId}-query`}
            type="search"
            value={q}
            placeholder={t('queryPlaceholder')}
            onChange={(event) => {
              setQuery(event.target.value);
              setValidation(null);
            }}
          />
        </label>
        <label htmlFor={`${htmlId}-tag`}>
          <span>{t('exactTag')}</span>
          <input
            id={`${htmlId}-tag`}
            value={tag}
            placeholder={t('exactTagPlaceholder')}
            onChange={(event) => {
              setTag(event.target.value);
              setValidation(null);
            }}
          />
        </label>
        <label htmlFor={`${htmlId}-kind`}>
          <span>{t('kind')}</span>
          <select
            id={`${htmlId}-kind`}
            value={kind}
            onChange={(event) =>
              setKind(event.target.value as AnnotationKind | '')
            }
          >
            <option value="">{t('allKinds')}</option>
            <option value="match">{t('matches')}</option>
            <option value="position">{t('positions')}</option>
            <option value="game">{l('game')}</option>
            <option value="move">{l('sourceMove')}</option>
          </select>
        </label>
        <ActionButton
          icon="search"
          label={t(loading ? 'searching' : 'search')}
          compact
          type="submit"
          className="primary"
          disabled={loading}
        >
          {t(loading ? 'searching' : 'search')}
        </ActionButton>
      </form>
      <p className="fineprint">{t('searchHelp')}</p>
      {validation && (
        <p className="error" role="alert">
          {t(validation)}
        </p>
      )}
      {!!error && (
        <p className="error" role="alert">
          {formatError(error)}
        </p>
      )}
      <div className="annotation-search-results" aria-busy={loading}>
        {loading && <p role="status">{t('searching')}</p>}
        {submitted && !loading && !error && (
          <p className="fineprint" role="status">
            {items.length
              ? t('resultCount', { count: results.total })
              : t('noResults')}
          </p>
        )}
        {items.map((hit) => (
          <article className="annotation-search-hit" key={hit.id}>
            <div className="annotation-hit-heading">
              <div>
                <span className="eyebrow">
                  {hit.target_kind === 'game'
                    ? l('game')
                    : hit.target_kind === 'move'
                      ? l('sourceMove')
                      : t(hit.target_kind === 'match' ? 'match' : 'position')}
                </span>
                <h4>
                  {hit.white} — {hit.black}
                </h4>
              </div>
              <ActionButton
                icon="open"
                label={t(
                  hit.target_kind === 'match' ? 'openMatch' : 'openPosition',
                )}
                compact
                type="button"
                onClick={() => onOpen(hit)}
              >
                {t(hit.target_kind === 'match' ? 'openMatch' : 'openPosition')}
              </ActionButton>
            </div>
            <p className="annotation-hit-context fineprint">
              <span>{hit.filename}</span>
              {hit.game_no != null && (
                <span>{t('game', { number: hit.game_no })}</span>
              )}
              {hit.ply != null && <span>{t('ply', { number: hit.ply })}</span>}
            </p>
            {!!hit.tags.length && (
              <div className="annotation-hit-tags">
                {hit.tags.map((value) => (
                  <Tooltip
                    as="button"
                    type="button"
                    key={value}
                    text={t('filterByTag', { tag: value })}
                    onClick={() => findTag(value)}
                  >
                    {value}
                  </Tooltip>
                ))}
              </div>
            )}
            {hit.comment_excerpt && (
              <p className="annotation-hit-excerpt">{hit.comment_excerpt}</p>
            )}
          </article>
        ))}
      </div>
      <SearchPagination
        page={results.page}
        lastPage={results.lastPage}
        busy={loading}
        onPageChange={results.setPage}
        label={t('searchTitle')}
      />
    </div>
  );
}
