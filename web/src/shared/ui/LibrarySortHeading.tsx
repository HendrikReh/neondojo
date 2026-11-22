import ActionButton from './ActionButton';
import { useLibraryText } from '../../library-messages';
import type { LibrarySort } from '../../library-api';
import type { ReactNode } from 'react';

export default function LibrarySortHeading<Sort extends string = LibrarySort>({
  fields,
  sort,
  direction,
  onSort,
  column,
  className,
  resizeHandle,
  disabled = false,
}: {
  fields: readonly { key: Sort; label: string; name?: string }[];
  sort: Sort;
  direction: 'asc' | 'desc';
  onSort: (sort: Sort) => void;
  column?: string;
  className?: string;
  resizeHandle?: ReactNode;
  disabled?: boolean;
}) {
  const l = useLibraryText();
  const active = fields.some((field) => field.key === sort);
  return (
    <th
      scope="col"
      className={className}
      data-column={column}
      aria-sort={
        active ? (direction === 'asc' ? 'ascending' : 'descending') : 'none'
      }
    >
      <div className="library-sort-heading">
        {fields.map((field) => {
          const selected = field.key === sort;
          const nextDescending = selected && direction === 'asc';
          const name = field.name ?? field.label;
          return (
            <ActionButton
              icon={selected && direction === 'desc' ? 'sort-down' : 'sort-up'}
              label={l('sortColumn', { column: name })}
              key={field.key}
              type="button"
              disabled={disabled}
              data-sort={field.key}
              aria-label={l('sortColumn', { column: name })}
              aria-pressed={selected}
              title={`${name} · ${l(nextDescending ? 'descending' : 'ascending')}`}
              onClick={() => onSort(field.key)}
            >
              <span className="library-sort-label">{field.label}</span>
            </ActionButton>
          );
        })}
      </div>
      {resizeHandle}
    </th>
  );
}
