import { type CardFilters } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { usePositionText } from './position-browser-messages';
export default function CardSearchControls({
  filters,
  onChange,
  onSearch,
  onClear,
}: {
  filters: CardFilters;
  onChange: (filters: CardFilters) => void;
  onSearch: () => void;
  onClear: () => void;
}) {
  const t = useCardSearchText(),
    p = usePositionText();
  const field = (
    key: 'q' | 'player' | 'tag' | 'comment' | 'position_id',
    label: string,
    maxLength: number,
  ) => (
    <label>
      <span>{label}</span>
      <input
        value={filters[key]}
        maxLength={maxLength}
        onChange={(event) =>
          onChange({ ...filters, [key]: event.target.value })
        }
      />
    </label>
  );
  return (
    <form
      className="card-search-controls"
      onSubmit={(event) => {
        event.preventDefault();
        onSearch();
      }}
    >
      <div className="card-search-row">
        {field('q', p('q'), 200)}
        {field('player', p('player'), 240)}
        <button className="primary">{t('search')}</button>
        <button type="button" onClick={onClear}>
          {p('clear')}
        </button>
      </div>
      <details>
        <summary>{p('filters')}</summary>
        <div className="card-filter-grid">
          {field('tag', p('tag'), 64)}
          {field('comment', p('comment'), 200)}
          {field('position_id', p('checkerId'), 14)}
          <label>
            <span>{t('kind')}</span>
            <select
              aria-label={t('kind')}
              value={filters.kind}
              onChange={(event) =>
                onChange({
                  ...filters,
                  kind: event.target.value as CardFilters['kind'],
                })
              }
            >
              <option value="">{t('allKinds')}</option>
              {(['checker', 'cube_offer', 'cube_response'] as const).map(
                (kind) => (
                  <option key={kind} value={kind}>
                    {t(kind)}
                  </option>
                ),
              )}
            </select>
          </label>
          <label>
            <span>{t('status')}</span>
            <select
              aria-label={t('status')}
              value={filters.status}
              onChange={(event) =>
                onChange({
                  ...filters,
                  status: event.target.value as CardFilters['status'],
                })
              }
            >
              <option value="">{t('allStatuses')}</option>
              {(['new', 'due', 'reviewed'] as const).map((status) => (
                <option key={status} value={status}>
                  {t(status)}
                </option>
              ))}
            </select>
          </label>
        </div>
      </details>
      <p>{t('scopeHint')}</p>
    </form>
  );
}
