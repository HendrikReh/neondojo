import type { ReactNode } from 'react';
import SearchFilterForm, {
  SearchFilterInput,
  SearchFilterSelect,
} from './shared/ui/SearchFilterForm';
import { useCardPracticeText } from './card-practice-messages';
import { useGamePlanText } from './game-plan-messages';
import type { CardFilters } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
export default function CardSearchControls({
  filters,
  onChange,
  onSearch,
  onClear,
  children,
}: {
  filters: CardFilters;
  onChange: (filters: CardFilters) => void;
  onSearch: () => void;
  onClear: () => void;
  children: ReactNode;
}) {
  const t = useCardSearchText();
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
  return (
    <SearchFilterForm
      filters={filters}
      onChange={onChange}
      onSearch={onSearch}
      onClear={onClear}
      searchLabel={t('search')}
      limitTextLength
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
