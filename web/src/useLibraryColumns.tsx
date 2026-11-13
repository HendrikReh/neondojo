import { useOptionalConfiguration } from './ConfigurationProvider';
import { useProfileValue } from './useProfileValue';
import { useLibraryText } from './library-messages';
import { useMemo } from 'react';
import {
  LIBRARY_RESIZABLE_LAYOUT,
  LIBRARY_COLUMNS,
  OCCURRENCE_COLUMNS,
  type LibraryColumn,
} from './library-columns';
import {
  DEFAULT_LIBRARY_COLUMNS,
  LIBRARY_DATA_COLUMNS,
  readVisibleColumns,
  saveVisibleColumns,
  toggleVisibleColumn,
  type LibraryDataColumn,
} from './library-column-preferences';
import { useResizableColumns } from './useResizableColumns';

const automaticWidths: Partial<Record<LibraryColumn, number>> = {
  white: 160,
  black: 160,
  play_mode: 90,
  tags: 124,
};

export function useLibraryColumns(visible: boolean, positionSearch = false) {
  const l = useLibraryText();
  const profile = useOptionalConfiguration();
  const [shown, setShown] = useProfileValue<LibraryDataColumn[]>(
    (c) => c.library.visible_columns,
    (c, visible_columns) => ({
      ...c,
      library: { ...c.library, visible_columns },
    }),
    readVisibleColumns,
  );
  const [order, setOrder] = useProfileValue<LibraryDataColumn[]>(
    (c) => c.library.column_order,
    (c, column_order) => ({ ...c, library: { ...c.library, column_order } }),
    () => [...LIBRARY_DATA_COLUMNS],
  );
  const activeColumns = useMemo(
    () =>
      [
        'select',
        ...order.filter((id) => shown.includes(id)),
        'actions',
      ] as const,
    [shown, order],
  );
  const resizeColumns = useMemo(
    () =>
      positionSearch
        ? [...OCCURRENCE_COLUMNS.map((column) => column.id), ...activeColumns]
        : activeColumns,
    [positionSearch, activeColumns],
  );
  const resized = useResizableColumns(LIBRARY_RESIZABLE_LAYOUT, {
    visible,
    activeColumns: resizeColumns,
    ...(profile?.current
      ? {
          widths: profile.current.library.column_widths,
          configurationGeneration: profile.generation,
          onWidthsChange: (column_widths: Record<string, number> | undefined) =>
            profile.updateCurrent((c) => ({
              ...c,
              library: { ...c.library, column_widths: column_widths ?? null },
            })),
        }
      : {}),
    resizeLabel: (label) => l('resizeColumn', { column: label }),
  });
  const apply = (value: LibraryDataColumn[], reset = false) => {
    setShown(value);
    if (!profile) saveVisibleColumns(reset ? undefined : value);
  };
  const activeSpecs = activeColumns.map((id) =>
    LIBRARY_COLUMNS.find((column) => column.id === id)!,
  );
  return {
    ...resized,
    shown,
    order,
    activeColumns,
    tableColumns: activeSpecs.map((column) => ({
      id: column.id,
      className: column.className,
      width: resized.widths
        ? resized.widths[column.id]
        : column.id === 'white' || column.id === 'black'
          ? undefined
          : (automaticWidths[column.id] ?? column.defaultWidth),
    })),
    move: (id: LibraryDataColumn, delta: -1 | 1) => {
      const from = order.indexOf(id),
        to = from + delta;
      if (from < 0 || to < 0 || to >= order.length) return;
      const next = [...order];
      [next[from], next[to]] = [next[to], next[from]];
      setOrder(next);
    },
    minimumWidth: activeSpecs.reduce(
      (sum, column) =>
        sum + (automaticWidths[column.id] ?? column.defaultWidth),
      positionSearch
        ? OCCURRENCE_COLUMNS.reduce(
            (sum, column) => sum + column.defaultWidth,
            0,
          )
        : 0,
    ),
    toggle: (id: LibraryDataColumn, checked: boolean) =>
      apply(toggleVisibleColumn(shown, id, checked)),
    showAll: () => apply([...LIBRARY_DATA_COLUMNS]),
    resetSelection: () => {
      apply([...DEFAULT_LIBRARY_COLUMNS], true);
      setOrder([...LIBRARY_DATA_COLUMNS]);
      resized.reset();
    },
  };
}
