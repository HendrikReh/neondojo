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
