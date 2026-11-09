import { useLibraryText } from './library-messages';
import { useMemo, useState } from 'react';
import { LIBRARY_COLUMN_LAYOUT, LIBRARY_COLUMNS } from './library-columns';
import {
  DEFAULT_LIBRARY_COLUMNS,
  LIBRARY_DATA_COLUMNS,
  readVisibleColumns,
  saveVisibleColumns,
  toggleVisibleColumn,
  type LibraryDataColumn,
} from './library-column-preferences';
import { useResizableColumns } from './useResizableColumns';

export function useLibraryColumns(visible: boolean) {
  const l = useLibraryText();
  const [shown, setShown] = useState(readVisibleColumns);
  const activeColumns = useMemo(
    () =>
      LIBRARY_COLUMNS.filter(
        (column) =>
          column.id === 'select' ||
          column.id === 'actions' ||
          shown.includes(column.id),
      ).map((column) => column.id),
    [shown],
  );
  const resized = useResizableColumns(LIBRARY_COLUMN_LAYOUT, {
    visible,
    activeColumns,
    resizeLabel: (label) => l('resizeColumn', { column: label }),
  });
  const apply = (value: LibraryDataColumn[], reset = false) => {
    setShown(value);
    saveVisibleColumns(reset ? undefined : value);
  };
  return {
    ...resized,
    shown,
    activeColumns,
    minimumWidth: LIBRARY_COLUMNS.filter((column) =>
      activeColumns.includes(column.id),
    ).reduce((sum, column) => sum + column.defaultWidth, 0),
    toggle: (id: LibraryDataColumn, checked: boolean) =>
      apply(toggleVisibleColumn(shown, id, checked)),
    showAll: () => apply([...LIBRARY_DATA_COLUMNS]),
    resetSelection: () => apply([...DEFAULT_LIBRARY_COLUMNS], true),
  };
}
