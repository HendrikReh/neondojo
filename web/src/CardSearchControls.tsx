import type { ReactNode } from 'react';
import SearchFilterForm, { SearchFilterSelect } from './SearchFilterForm';
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
