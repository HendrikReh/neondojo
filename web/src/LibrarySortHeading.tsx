import { useLibraryText } from './library-messages';
import type { LibrarySort } from './library-api';
import type { ReactNode } from 'react';

export default function LibrarySortHeading<Sort extends string = LibrarySort>({
  fields,
  sort,
  direction,
  onSort,
  column,
  resizeHandle,
  disabled = false,
}: {
  fields: readonly { key: Sort; label: string; name?: string }[];
  sort: Sort;
  direction: 'asc' | 'desc';
  onSort: (sort: Sort) => void;
  column?: string;
  resizeHandle?: ReactNode;
  disabled?: boolean;
}) {
  const l = useLibraryText();
  const active = fields.some((field) => field.key === sort);
  return (
    <th
      scope="col"
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
            <button
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
              <span className="library-sort-indicator" aria-hidden="true">
                {selected ? (direction === 'asc' ? '↑' : '↓') : '↕'}
              </span>
            </button>
          );
        })}
      </div>
      {resizeHandle}
    </th>
  );
}
