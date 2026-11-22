import { useI18n } from '../i18n/i18n';
import { usePositionText } from '../../position-browser-messages';

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

export default NumberedSearchPagination;
