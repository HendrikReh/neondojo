import { useI18n } from './i18n';
import { useMemo } from 'react';
import ActionButton from './ActionButton';
import { usePositionText } from './position-browser-messages';

export function lastSearchPage(count: number, pageSize: number) {
  return Math.max(0, Math.ceil(count / pageSize) - 1);
}

export function useSearchResultPage<T>(
  items: readonly T[],
  pageSize: number,
  pageIndex: number,
) {
  return useMemo(() => {
    const lastPage = lastSearchPage(items.length, pageSize);
    const page = Math.min(pageIndex, lastPage);
    const start = page * pageSize;
    return {
      page,
      lastPage,
      start,
      rows: items.slice(start, start + pageSize),
    };
  }, [items, pageSize, pageIndex]);
}

export default function SearchPagination({
  page,
  lastPage,
  busy,
  onPageChange,
  onMore,
  moreLabel,
}: {
  page: number;
  lastPage: number;
  busy: boolean;
  onPageChange: (page: number) => void;
  onMore?: () => Promise<boolean>;
  moreLabel: string;
}) {
  const p = usePositionText();
  async function loadMore() {
    if (await onMore?.()) onPageChange(page + 1);
  }
  return (
    <nav className="position-pagination" aria-label={p('results')}>
      <ActionButton
        icon="back"
        label={p('previousPage')}
        compact
        disabled={page === 0 || busy}
        onClick={() => onPageChange(page - 1)}
      />
      <span>{p('page', { number: page + 1 })}</span>
      {page < lastPage ? (
        <ActionButton
          icon="next"
          label={p('nextPage')}
          compact
          disabled={busy}
          onClick={() => onPageChange(page + 1)}
        />
      ) : (
        <ActionButton
          icon="more"
          label={moreLabel}
          disabled={!onMore || busy}
          onClick={() => void loadMore()}
        />
      )}
    </nav>
  );
}

export function NumberedSearchPagination({
  page,
  lastPage,
  busy = false,
  disabled = false,
  onPageChange,
  label,
}: {
  page: number;
  lastPage: number;
  busy?: boolean;
  disabled?: boolean;
  onPageChange: (page: number) => void;
  label?: string;
}) {
  const p = usePositionText();
  const { formatNumber } = useI18n();
  if (lastPage <= 1) return null;

  const first = Math.max(1, Math.min(page - 2, lastPage - 4));
  return (
    <nav
      className="search-pagination text-navigation text-navigation--compact"
      aria-label={label ?? p('results')}
      aria-busy={busy}
    >
      <button
        type="button"
        disabled={disabled || page === 1}
        onClick={() => onPageChange(1)}
      >
        {p('firstPage')}
      </button>
      <button
        type="button"
        aria-label={p('previousPage')}
        disabled={disabled || page === 1}
        onClick={() => onPageChange(page - 1)}
      >
        ‹
      </button>
      {Array.from({ length: Math.min(lastPage, 5) }, (_, index) => {
        const number = first + index;
        return (
          <button
            key={number}
            type="button"
            aria-current={number === page ? 'page' : undefined}
            aria-label={p('page', { number })}
            disabled={disabled}
            onClick={() => onPageChange(number)}
          >
            {formatNumber(number)}
          </button>
        );
      })}
      <button
        type="button"
        aria-label={p('nextPage')}
        disabled={disabled || page === lastPage}
        onClick={() => onPageChange(page + 1)}
      >
        ›
      </button>
      <button
        type="button"
        disabled={disabled || page === lastPage}
        onClick={() => onPageChange(lastPage)}
      >
        {p('lastPage')}
      </button>
    </nav>
  );
}
