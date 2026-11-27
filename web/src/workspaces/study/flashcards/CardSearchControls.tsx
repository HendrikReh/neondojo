import type { ReactNode } from 'react';
import AppliedFilterChips from '../../../shared/ui/AppliedFilterChips';
import SearchFilterForm, {
  SearchFilterInput,
  SearchFilterSelect,
  searchTextFields,
} from '../../../shared/ui/SearchFilterForm';
import { usePositionText } from '../../positions/position-browser-messages';
import { useCardPracticeText } from './card-practice-messages';
import { useGamePlanText } from '../../analysis/game-plans/game-plan-messages';
import type { CardFilters } from './card-search-api';
import { useCardSearchText } from './card-search-messages';

const cardFilterKeys: readonly (keyof CardFilters)[] = [
  'q',
  'player',
  'tag',
  'comment',
  'position_id',
  'topics',
  'game_plan',
  'kind',
  'status',
];

export default function CardSearchControls({
  filters,
  applied,
  busy = false,
  onChange,
  onSearch,
  onClear,
  onRemoveFilter,
  children,
}: {
  filters: CardFilters;
  applied?: CardFilters;
  busy?: boolean;
  onChange: (filters: CardFilters) => void;
  onSearch: () => void;
  onClear: () => void;
  onRemoveFilter?: (key: keyof CardFilters) => void;
  children: ReactNode;
}) {
  const t = useCardSearchText();
  const p = usePositionText();
  const practice = useCardPracticeText();
  const gamePlan = useGamePlanText();
  const plans = [
    'opening',
    'pure_race',
    'racing',
    'attacking',
    'priming',
    'anchoring',
  ] as const;
  const active = applied ?? filters;
  const chips = cardFilterKeys.flatMap<{
    key: keyof CardFilters;
    text: string;
  }>((key) => {
    const raw = (active[key] ?? '').trim();
    if (!raw) return [];
    let label: string;
    let value = raw;
    switch (key) {
      case 'q':
      case 'player':
      case 'tag':
      case 'comment':
      case 'position_id':
        label = p(searchTextFields[key].label);
        break;
      case 'topics':
        label = practice('topicFilter');
        break;
      case 'game_plan':
        label = gamePlan('title');
        value = plans.includes(raw as (typeof plans)[number])
          ? gamePlan(raw as (typeof plans)[number])
          : raw;
        break;
      case 'kind':
        label = t('kind');
        value = t(raw as Exclude<CardFilters['kind'], ''>);
        break;
      case 'status':
        label = t('status');
        value = t(raw as Exclude<CardFilters['status'], ''>);
        break;
    }
    return [{ key, text: p('appliedFilter', { label, value }) }];
  });
  const canReset = cardFilterKeys.some(
    (key) =>
      Boolean((filters[key] ?? '').trim()) ||
      Boolean((active[key] ?? '').trim()),
  );
  return (
    <SearchFilterForm
      filters={filters}
      onChange={onChange}
      onSearch={onSearch}
      onClear={onClear}
      searchLabel={t('search')}
      limitTextLength
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
              if (onRemoveFilter) onRemoveFilter(key);
              else onChange({ ...filters, [key]: '' });
            }}
          />
        ),
      }}
      scope={children}
    >
      <SearchFilterInput
        label={practice('topicFilter')}
        value={filters.topics ?? ''}
        maxLength={650}
        onChange={(topics) => onChange({ ...filters, topics })}
      />
      <SearchFilterSelect
        label={gamePlan('title')}
        value={filters.game_plan ?? ''}
        onChange={(value) =>
          onChange({
            ...filters,
            game_plan: plans.find((plan) => plan === value) ?? '',
          })
        }
      >
        <option value="">—</option>
        {plans.map((plan) => (
          <option value={plan} key={plan}>
            {gamePlan(plan)}
          </option>
        ))}
      </SearchFilterSelect>
      <SearchFilterSelect
        label={t('kind')}
        value={filters.kind}
        onChange={(value) =>
          onChange({ ...filters, kind: value as CardFilters['kind'] })
        }
      >
        <option value="">{t('allKinds')}</option>
        {(['checker', 'cube_offer', 'cube_response', 'anki'] as const).map(
          (kind) => (
            <option key={kind} value={kind}>
              {t(kind)}
            </option>
          ),
        )}
      </SearchFilterSelect>
      <SearchFilterSelect
        label={t('status')}
        value={filters.status}
        onChange={(value) =>
          onChange({ ...filters, status: value as CardFilters['status'] })
        }
      >
        <option value="">{t('allStatuses')}</option>
        {(['new', 'due', 'reviewed'] as const).map((status) => (
          <option key={status} value={status}>
            {t(status)}
          </option>
        ))}
      </SearchFilterSelect>
    </SearchFilterForm>
  );
}
