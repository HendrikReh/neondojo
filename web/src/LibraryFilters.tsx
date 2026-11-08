import type { LibraryFilters as Filters } from './library-api';
import { useLibraryText } from './library-messages';
import type { Position } from './api';
import { useI18n } from './i18n';
export default function LibraryFilters({
  filters,
  onChange,
  onClear,
  currentPosition,
}: {
  filters: Filters;
  onChange: (filters: Filters) => void;
  onClear: () => void;
  currentPosition?: Position;
}) {
  const l = useLibraryText(),
    { t } = useI18n();
  const field = (key: keyof Filters, label: string, type = 'text') => (
    <label key={key}>
      <span>{label}</span>
      <input
        type={type}
        min={type === 'number' ? 0 : undefined}
        value={filters[key]}
        onChange={(e) => onChange({ ...filters, [key]: e.target.value })}
      />
    </label>
  );
  return (
    <div className="library-filters">
      {field('player', l('player'))}
      {field('match_name', l('matchName'))}
      {field('tournament', l('tournament'))}
      {field('round', l('round'))}
      <label>
        <span>{l('dateField')}</span>
        <select
          aria-label={l('dateField')}
          value={filters.date_field}
          onChange={(e) => onChange({ ...filters, date_field: e.target.value })}
        >
          <option value="played">{l('played')}</option>
          <option value="imported">{l('imported')}</option>
        </select>
      </label>
      {field('date_from', l('dateFrom'), 'date')}
      {field('date_to', l('dateTo'), 'date')}
      {field('tag', l('tag'))}
      {field('comment', l('comment'))}
      <label>
        <span>{l('type')}</span>
        <select
          aria-label={l('type')}
          value={filters.match_type}
          onChange={(e) => onChange({ ...filters, match_type: e.target.value })}
        >
          <option value="">{l('any')}</option>
          <option value="match">{l('match')}</option>
          <option value="money">{l('money')}</option>
        </select>
      </label>
      <label>
        <span>{l('result')}</span>
        <select
          aria-label={l('result')}
          value={filters.result}
          onChange={(e) => onChange({ ...filters, result: e.target.value })}
        >
          <option value="">{l('any')}</option>
          <option value="completed">{l('completed')}</option>
          <option value="white_won">{l('whiteWon')}</option>
          <option value="black_won">{l('blackWon')}</option>
          <option value="draw">{l('draw')}</option>
          <option value="ongoing">{l('ongoing')}</option>
          <option value="unknown">{l('unknown')}</option>
        </select>
      </label>
      {field('match_points_min', l('minPoints'), 'number')}
      {field('match_points_max', l('maxPoints'), 'number')}
      {field('position_id', l('position'))}
      <div className="library-filter-actions">
        {currentPosition && (
          <button
            onClick={() =>
              onChange({ ...filters, position_id: currentPosition.position_id })
            }
          >
            {t('app.usePosition')}
          </button>
        )}
        <button onClick={onClear}>{l('clearFilters')}</button>
      </div>
    </div>
  );
}
