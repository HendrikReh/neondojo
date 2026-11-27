import AppliedFilterChips from '../../shared/ui/AppliedFilterChips';
import SearchFilterForm, {
  SearchFilterInput,
  SearchFilterSelect,
  searchTextFields,
} from '../../shared/ui/SearchFilterForm';
import { useGamePlanText } from '../analysis/game-plans/game-plan-messages';
import { mistakeGamePlans } from '../study/mistakes/recurring-mistakes-api';
import { useI18n } from '../../shared/i18n/i18n';
import type { Database } from '../library/library-api';
import {
  positionFilterKeys,
  type PositionFilterDraft,
  type PositionFilterKey,
  type SavedPositionFilter,
} from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';
import { useSavedPositionSearches } from './SavedPositionSearches';

export { useSavedPositionSearches } from './SavedPositionSearches';

const filterLabels = {
  q: searchTextFields.q.label,
  player: searchTextFields.player.label,
  tag: searchTextFields.tag.label,
  comment: searchTextFields.comment.label,
  position_id: searchTextFields.position_id.label,
  database_id: 'database',
  match_name: 'matchName',
  tournament: 'tournament',
  round: 'round',
  match_points_min: 'minPoints',
  match_points_max: 'maxPoints',
  die1: 'die1',
  die2: 'die2',
  white_score: 'whiteScore',
  black_score: 'blackScore',
  crawford: 'crawford',
  cube_value: 'cube',
  analysis_state: 'analysis',
  min_loss: 'minLoss',
  max_loss: 'maxLoss',
} as const satisfies Record<
  Exclude<PositionFilterKey, 'game_plan'>,
  PositionMessageKey
>;

const analysisOptions = [
  ['', 'any'],
  ['ready', 'ready'],
  ['missing', 'missing'],
] as const;

export default function PositionBrowserControls({
  filters,
  applied,
  activeKeys,
  databases,
  onChange,
  onSearch,
  onClear,
  onRemoveFilter,
  onLoad,
  busy,
  revision,
}: {
  filters: PositionFilterDraft;
  applied: PositionFilterDraft;
  activeKeys: readonly PositionFilterKey[];
  databases: Database[];
  onChange: (filters: PositionFilterDraft) => void;
  onSearch: () => void;
  onClear: () => void;
  onRemoveFilter: (key: PositionFilterKey) => void;
  onLoad: (saved: SavedPositionFilter) => void;
  busy: boolean;
  revision: number;
}) {
  const p = usePositionText();
  const planText = useGamePlanText();
  const { formatNumber } = useI18n();
  const chips = activeKeys.flatMap<{ key: PositionFilterKey; text: string }>(
    (key) => {
      if (key === 'die2' && applied.die1) return [];
      if (key === 'die1' || key === 'die2') {
        const dice = [applied.die1, applied.die2]
          .filter(Boolean)
          .map((die) => formatNumber(Number(die)))
          .join('–');
        return [
          { key, text: p('appliedFilter', { label: p('dice'), value: dice }) },
        ];
      }
      const label =
        key === 'game_plan' ? planText('title') : p(filterLabels[key]);
      return [
        { key, text: p('appliedFilter', { label, value: filterValue(key) }) },
      ];
    },
  );
  const canReset =
    activeKeys.length > 0 ||
    positionFilterKeys.some((key) => filters[key] !== '') ||
    applied.sort !== 'match' ||
    !!applied.direction ||
    filters.sort !== 'match' ||
    !!filters.direction;

  function filterValue(key: PositionFilterKey) {
    const value = applied[key];
    switch (key) {
      case 'database_id':
        return (
          databases.find((database) => database.id === value)?.name ??
          `${p('missing')} (${value})`
        );
      case 'crawford':
        return p(value === 'true' ? 'yes' : 'no');
      case 'analysis_state':
        return p(value === 'ready' ? 'ready' : 'missing');
      case 'game_plan': {
        const plan = mistakeGamePlans.find((plan) => plan === value);
        return plan ? planText(plan) : value;
      }
      case 'position_id':
      case 'q':
      case 'player':
      case 'match_name':
      case 'tournament':
      case 'round':
      case 'tag':
      case 'comment':
        return value;
      default:
        return formatNumber(Number(value), { maximumFractionDigits: 20 });
    }
  }

  const field = (
    key: keyof typeof filterLabels,
    type = 'text',
    min = 0,
    max?: number,
  ) => (
    <SearchFilterInput
      label={p(filterLabels[key])}
      type={type}
      min={type === 'number' ? min : undefined}
      max={max}
      step={
        key === 'min_loss' || key === 'max_loss'
          ? 'any'
          : type === 'number'
            ? 1
            : undefined
      }
      value={filters[key]}
      onChange={(value) => onChange({ ...filters, [key]: value })}
    />
  );
  const select = (
    key: keyof PositionFilterDraft,
    label: PositionMessageKey,
    options: readonly (readonly [string, PositionMessageKey])[],
  ) => (
    <SearchFilterSelect
      label={p(label)}
      value={filters[key]}
      onChange={(value) => onChange({ ...filters, [key]: value })}
    >
      {options.map(([value, text]) => (
        <option key={value} value={value}>
          {p(text)}
        </option>
      ))}
    </SearchFilterSelect>
  );
  const saved = useSavedPositionSearches({ filters, onLoad, revision });
  const databaseMissing =
    !!filters.database_id &&
    !databases.some((database) => database.id === filters.database_id);
  return (
    <SearchFilterForm
      filters={filters}
      onChange={onChange}
      onSearch={onSearch}
      onClear={onClear}
      searchLabel={p('search')}
      searchDisabled={busy}
      compactToolbar={{
        reset: canReset,
        appliedFilters: (
          <AppliedFilterChips
            chips={chips}
            disabled={busy}
            onRemove={(key, button) => {
              button.form
                ?.querySelector<HTMLElement>(
                  '.position-filter-details > summary',
                )
                ?.focus();
              onRemoveFilter(key);
            }}
          />
        ),
      }}
      searchRowExtra={saved.field}
      savedSearches={saved.status}
      scope={
        <SearchFilterSelect
          label={p(filterLabels.database_id)}
          value={filters.database_id}
          onChange={(database_id) => onChange({ ...filters, database_id })}
        >
          <option value="">{p('allDatabases')}</option>
          {databaseMissing && (
            <option value={filters.database_id}>
              {p('missing')} ({filters.database_id})
            </option>
          )}
          {databases.map((database) => (
            <option key={database.id} value={database.id}>
              {database.name}
            </option>
          ))}
        </SearchFilterSelect>
      }
      filterGroups={(text) => (
        <div className="position-filter-groups">
          <fieldset className="position-filter-group">
            <legend>{p('groupMatch')}</legend>
            <div className="position-filter-grid">
              {field('match_name')}
              {field('tournament')}
              {field('round')}
              <div className="position-filter-pair">
                {field('match_points_min', 'number')}
                {field('match_points_max', 'number')}
              </div>
            </div>
          </fieldset>
          <fieldset className="position-filter-group">
            <legend>{p('groupPosition')}</legend>
            <div className="position-filter-grid">
              <SearchFilterSelect
                label={planText('title')}
                value={filters.game_plan ?? ''}
                onChange={(game_plan) => onChange({ ...filters, game_plan })}
              >
                <option value="">{p('any')}</option>
                {mistakeGamePlans.map((plan) => (
                  <option key={plan} value={plan}>
                    {planText(plan)}
                  </option>
                ))}
              </SearchFilterSelect>
              <div className="position-filter-pair">
                {field('die1', 'number', 1, 6)}
                {field('die2', 'number', 1, 6)}
              </div>
              {field('white_score', 'number')}
              {field('black_score', 'number')}
              {select('crawford', filterLabels.crawford, [
                ['', 'any'],
                ['true', 'yes'],
                ['false', 'no'],
              ])}
              {field('cube_value', 'number', 1, 32768)}
              {text.position_id}
            </div>
          </fieldset>
          <fieldset className="position-filter-group position-filter-group--half">
            <legend>{p('groupAnalysis')}</legend>
            <div className="position-filter-grid">
              {select(
                'analysis_state',
                filterLabels.analysis_state,
                analysisOptions,
              )}
              <div className="position-filter-pair">
                {field('min_loss', 'number')}
                {field('max_loss', 'number')}
              </div>
            </div>
          </fieldset>
          <fieldset className="position-filter-group position-filter-group--half">
            <legend>{p('groupNotes')}</legend>
            <div className="position-filter-grid">
              {text.tag}
              {text.comment}
            </div>
          </fieldset>
        </div>
      )}
    />
  );
}
