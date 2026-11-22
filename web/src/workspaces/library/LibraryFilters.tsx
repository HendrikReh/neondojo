import ActionButton from '../../shared/ui/ActionButton';
import AppliedFilterChips, {
  type AppliedFilterChip,
} from '../../shared/ui/AppliedFilterChips';
import type { LibraryFilters as Filters } from './library-api';
import { useLibraryText, type LibraryMessageKey } from './library-messages';
import { useI18n } from '../../shared/i18n/i18n';
import { usePositionText } from '../positions/position-browser-messages';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';

export type LibraryFilterKey = Exclude<keyof Filters, 'date_field'>;
const labels = {
  player: 'player',
  match_name: 'matchName',
  tournament: 'tournament',
  round: 'round',
  date_from: 'dateFrom',
  date_to: 'dateTo',
  tag: 'tag',
  comment: 'comment',
  match_type: 'type',
  result: 'result',
  match_points_min: 'minPoints',
  match_points_max: 'maxPoints',
  position_id: 'position',
} as const satisfies Record<Exclude<LibraryFilterKey, 'q'>, LibraryMessageKey>;
const types = [
  ['', 'any'],
  ['match', 'match'],
  ['money', 'money'],
] as const;
const results = [
  ['', 'any'],
  ['completed', 'completed'],
  ['white_won', 'whiteWon'],
  ['black_won', 'blackWon'],
  ['draw', 'draw'],
  ['ongoing', 'ongoing'],
  ['unknown', 'unknown'],
] as const;
const dateLabel = (field: string) =>
  field === 'imported' ? 'imported' : 'played';

export function LibraryFilterChips({
  filters,
  onRemove,
}: {
  filters: Filters;
  onRemove: (key: LibraryFilterKey) => void;
}) {
  const l = useLibraryText();
  const p = usePositionText();
  const { t, locale, formatNumber } = useI18n();
  const formatDate = dateTimeFormat(locale, { dateStyle: 'medium' });
  const keys: LibraryFilterKey[] = [
    'q',
    ...(Object.keys(labels) as (keyof typeof labels)[]),
  ];
  const chips: AppliedFilterChip<LibraryFilterKey>[] = [];
  for (const key of keys) {
    let value = filters[key].trim();
    if (!value) continue;
    let label = key === 'q' ? p('q') : l(labels[key]);
    switch (key) {
      case 'date_from':
      case 'date_to': {
        label = `${l(dateLabel(filters.date_field))} · ${label}`;
        const date = new Date(`${value}T12:00:00`);
        if (Number.isFinite(date.getTime())) value = formatDate.format(date);
        break;
      }
      case 'match_points_min':
      case 'match_points_max': {
        const points = Number(value);
        if (Number.isFinite(points)) value = formatNumber(points);
        break;
      }
      case 'match_type':
      case 'result': {
        const options = key === 'match_type' ? types : results;
        const option = options.find(([id]) => id === value);
        if (option) value = l(option[1]);
        break;
      }
      case 'position_id':
        label = t('app.positionId');
        break;
    }
    const text = p('appliedFilter', { label, value });
    chips.push({
      key,
      text,
      removeLabel: key === 'position_id' ? l('clearPositionFilter') : undefined,
    });
  }
  return <AppliedFilterChips chips={chips} onRemove={onRemove} />;
}

export default function LibraryFilters({
  filters,
  onChange,
  onClear,
  panelId,
  open,
}: {
  filters: Filters;
  onChange: (filters: Filters) => void;
  onClear: () => void;
  panelId: string;
  open: boolean;
}) {
  const l = useLibraryText();
  const field = (key: keyof typeof labels, type = 'text') => (
    <label key={key}>
      <span>{l(labels[key])}</span>
      <input
        type={type}
        min={type === 'number' ? 0 : undefined}
        value={filters[key]}
        onChange={(e) => onChange({ ...filters, [key]: e.target.value })}
      />
    </label>
  );
  const select = (
    key: 'match_type' | 'result',
    options: readonly (readonly [string, LibraryMessageKey])[],
  ) => (
    <label>
      <span>{l(labels[key])}</span>
      <select
        aria-label={l(labels[key])}
        value={filters[key]}
        onChange={(e) => onChange({ ...filters, [key]: e.target.value })}
      >
        {options.map(([value, label]) => (
          <option key={value} value={value}>
            {l(label)}
          </option>
        ))}
      </select>
    </label>
  );
  return (
    <div className="library-filters" id={panelId} hidden={!open}>
      {field('player')}
      {field('match_name')}
      {field('tournament')}
      {field('round')}
      <label>
        <span>{l('dateField')}</span>
        <select
          aria-label={l('dateField')}
          value={filters.date_field}
          onChange={(e) => onChange({ ...filters, date_field: e.target.value })}
        >
          <option value="played">{l(dateLabel('played'))}</option>
          <option value="imported">{l(dateLabel('imported'))}</option>
        </select>
      </label>
      {field('date_from', 'date')}
      {field('date_to', 'date')}
      {field('tag')}
      {field('comment')}
      {select('match_type', types)}
      {select('result', results)}
      {field('match_points_min', 'number')}
      {field('match_points_max', 'number')}
      <div className="library-filter-actions">
        <ActionButton
          icon="clear"
          label={l('clearFilters')}
          compact
          onClick={onClear}
        />
      </div>
    </div>
  );
}
