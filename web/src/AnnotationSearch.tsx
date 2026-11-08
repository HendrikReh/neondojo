import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import {
  normalizeAnnotationTag,
  searchAnnotations,
  searchValidation,
  type AnnotationFilters,
  type AnnotationHit,
  type AnnotationKind,
  type SearchValidation,
} from './annotation-api';
import { useAnnotationI18n } from './annotation-messages';

export type { AnnotationHit } from './annotation-api';

export default function AnnotationSearch({
  revision = 0,
  onOpen,
}: {
  revision?: number;
  onOpen: (hit: AnnotationHit) => void;
}) {
  const { t, formatError } = useAnnotationI18n();
  const htmlId = useId();
  const [q, setQuery] = useState('');
  const [tag, setTag] = useState('');
  const [kind, setKind] = useState<AnnotationKind | ''>('');
  const [submitted, setSubmitted] = useState<AnnotationFilters | null>(null);
  const [items, setItems] = useState<AnnotationHit[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [complete, setComplete] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [validation, setValidation] = useState<SearchValidation | null>(null);
  const moreController = useRef<AbortController | null>(null);

  useEffect(() => {
    if (!submitted) return;
    const controller = new AbortController();
    moreController.current?.abort();
    setItems([]);
    setCursor(null);
    setLoading(true);
    setLoadingMore(false);
    setComplete(false);
    setError(null);
    void searchAnnotations(submitted, null, controller.signal)
      .then((result) => {
        if (controller.signal.aborted) return;
        setItems(result.items);
        setCursor(result.next_cursor);
        setComplete(true);
      })
      .catch((failure) => {
        if (!controller.signal.aborted) setError(failure);
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => {
      controller.abort();
      moreController.current?.abort();
    };
  }, [submitted, revision]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const issue = searchValidation(q, tag);
    setValidation(issue);
    if (issue) return;
    setSubmitted({ q: q.trim(), tag: normalizeAnnotationTag(tag), kind });
  };

  const more = async () => {
    if (!submitted || !cursor || loading || loadingMore) return;
    const controller = new AbortController();
    moreController.current = controller;
    setLoadingMore(true);
    setError(null);
    try {
      const result = await searchAnnotations(
        submitted,
        cursor,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setItems((previous) => {
        const ids = new Set(previous.map((item) => item.id));
        return [
          ...previous,
          ...result.items.filter((item) => !ids.has(item.id)),
        ];
      });
      setCursor(result.next_cursor);
    } catch (failure) {
      if (!controller.signal.aborted) setError(failure);
    } finally {
      if (!controller.signal.aborted) setLoadingMore(false);
    }
  };

  const findTag = (value: string) => {
    setQuery('');
    setTag(value);
    setValidation(null);
    setSubmitted({ q: '', tag: value, kind });
  };

  return (
    <details className="annotation-search">
      <summary>{t('searchTitle')}</summary>
      <p className="fineprint">{t('searchDescription')}</p>
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
          </select>
        </label>
        <button type="submit" className="primary" disabled={loading}>
          {t(loading ? 'searching' : 'search')}
        </button>
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
      <div
        className="annotation-search-results"
        aria-busy={loading || loadingMore}
      >
        {loading && <p role="status">{t('searching')}</p>}
        {complete && !loading && (
          <p className="fineprint" role="status">
            {items.length
              ? t('resultCount', { count: items.length })
              : t('noResults')}
          </p>
        )}
        {items.map((hit) => (
          <article className="annotation-search-hit" key={hit.id}>
            <div className="annotation-hit-heading">
              <div>
                <span className="eyebrow">
                  {t(hit.target_kind === 'match' ? 'match' : 'position')}
                </span>
                <h4>
                  {hit.white} — {hit.black}
                </h4>
              </div>
              <button type="button" onClick={() => onOpen(hit)}>
                {t(hit.target_kind === 'match' ? 'openMatch' : 'openPosition')}
              </button>
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
                  <button
                    type="button"
                    key={value}
                    title={t('filterByTag', { tag: value })}
                    onClick={() => findTag(value)}
                  >
                    {value}
                  </button>
                ))}
              </div>
            )}
            {hit.comment_excerpt && (
              <p className="annotation-hit-excerpt">{hit.comment_excerpt}</p>
            )}
          </article>
        ))}
      </div>
      {cursor && (
        <div className="annotation-search-actions">
          <button
            type="button"
            disabled={loading || loadingMore}
            onClick={() => void more()}
          >
            {t(loadingMore ? 'searching' : 'more')}
          </button>
        </div>
      )}
    </details>
  );
}
