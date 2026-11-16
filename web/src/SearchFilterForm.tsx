import type { InputHTMLAttributes, ReactNode, Ref } from 'react';
import ActionButton from './ActionButton';
import { usePositionText } from './position-browser-messages';
import './position-browser.css';

export const searchTextFields = {
  q: { label: 'q', maxLength: 200 },
  player: { label: 'player', maxLength: 240 },
  tag: { label: 'tag', maxLength: 64 },
  comment: { label: 'comment', maxLength: 200 },
  position_id: { label: 'checkerId', maxLength: 14 },
} as const;
type TextFilter = keyof typeof searchTextFields;

export default function SearchFilterForm<T extends Record<TextFilter, string>>({
  filters,
  onChange,
  onSearch,
  onClear,
  searchLabel,
  searchDisabled,
  limitTextLength,
  savedSearches,
  scope,
  searchRowExtra,
  beforeTextFilters,
  footer,
  compactToolbar,
  filterGroups,
  children,
}: {
  filters: T;
  onChange: (filters: T) => void;
  onSearch: () => void;
  onClear: () => void;
  searchLabel: string;
  searchDisabled?: boolean;
  limitTextLength?: boolean;
  savedSearches?: ReactNode;
  scope: ReactNode;
  /** A further field in the search row, before the search button. */
  searchRowExtra?: ReactNode;
  beforeTextFilters?: ReactNode;
  footer?: ReactNode;
  compactToolbar?: { appliedFilters: ReactNode; reset: boolean };
  /** Replaces the flat grid with the caller's own grouping of all filters. */
  filterGroups?: (
    text: Record<'tag' | 'comment' | 'position_id', ReactNode>,
  ) => ReactNode;
  children?: ReactNode;
}) {
  const t = usePositionText();
  const textField = (key: TextFilter) => {
    const { label, maxLength } = searchTextFields[key];
    return (
      <SearchFilterInput
        label={t(label)}
        value={filters[key]}
        maxLength={limitTextLength ? maxLength : undefined}
        onChange={(value) => onChange({ ...filters, [key]: value })}
      />
    );
  };

  const clear = (
    <ActionButton
      icon="clear"
      label={t('clear')}
      compact={!compactToolbar}
      type="button"
      onClick={onClear}
    />
  );

  return (
    <div
      className={
        compactToolbar
          ? 'position-controls position-controls--compact'
          : 'position-controls'
      }
    >
      {!compactToolbar && savedSearches}
      <form
        className="position-filter-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSearch();
        }}
      >
        <div
          className={
            searchRowExtra
              ? 'position-search-row position-search-row--extra'
              : 'position-search-row'
          }
        >
          {textField('q')}
          {textField('player')}
          {scope}
          {searchRowExtra}
          <ActionButton
            icon="search"
            label={searchLabel}
            compact={!compactToolbar}
            className="primary"
            type="submit"
            disabled={searchDisabled}
          />
        </div>
        <div className={compactToolbar ? 'position-filter-toolbar' : undefined}>
          <details className="position-filter-details">
            <summary>{t('filters')}</summary>
            {filterGroups ? (
              filterGroups({
                tag: textField('tag'),
                comment: textField('comment'),
                position_id: textField('position_id'),
              })
            ) : (
              <div className="position-filter-grid">
                {beforeTextFilters}
                {textField('tag')}
                {textField('comment')}
                {textField('position_id')}
                {children}
                {compactToolbar && footer}
              </div>
            )}
          </details>
          {compactToolbar?.reset && clear}
        </div>
        {compactToolbar ? (
          compactToolbar.appliedFilters
        ) : (
          <div className="position-filter-footer">
            {footer}
            {clear}
          </div>
        )}
      </form>
      {compactToolbar && savedSearches}
    </div>
  );
}

export function SearchFilterInput({
  label,
  onChange,
  ...input
}: Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange'> & {
  label: string;
  onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>{label}</span>
      <input {...input} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

export function SearchFilterSelect({
  label,
  value,
  onChange,
  children,
  selectRef,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  children: ReactNode;
  selectRef?: Ref<HTMLSelectElement>;
}) {
  return (
    <label>
      <span>{label}</span>
      <select
        aria-label={label}
        ref={selectRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      >
        {children}
      </select>
    </label>
  );
}
