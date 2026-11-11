import { LIBRARY_COLUMNS, type LibraryColumn } from './library-columns';

export type LibraryDataColumn = Exclude<LibraryColumn, 'select' | 'actions'>;
export const LIBRARY_DATA_COLUMNS = LIBRARY_COLUMNS.map(
  (column) => column.id,
).filter((id): id is LibraryDataColumn => id !== 'select' && id !== 'actions');
export const DEFAULT_LIBRARY_COLUMNS = LIBRARY_DATA_COLUMNS.filter(
  (id) => id !== 'tournament' && id !== 'comment',
);
export const VISIBLE_COLUMNS_KEY = 'bgtutor.library.visibleColumns';
const canonical = (columns: readonly string[]) =>
  LIBRARY_DATA_COLUMNS.filter((id) => columns.includes(id));

export function readVisibleColumns(
  storage?: Pick<Storage, 'getItem'>,
): LibraryDataColumn[] {
  try {
    const raw = (storage ?? localStorage).getItem(VISIBLE_COLUMNS_KEY);
    if (raw && raw.length <= 4096) {
      const profile = JSON.parse(raw);
      if (
        (profile?.version === 1 || profile?.version === 2) &&
        Array.isArray(profile.columns) &&
        profile.columns.every((value: unknown) => typeof value === 'string')
      ) {
        const columns = canonical(profile.columns);
        if (columns.length)
          return profile.version === 1
            ? canonical([...columns, 'play_mode'])
            : columns;
      }
    }
  } catch {
    /* Keep the default view usable without local storage. */
  }
  return [...DEFAULT_LIBRARY_COLUMNS];
}
export function saveVisibleColumns(
  columns?: readonly LibraryDataColumn[],
  storage?: Pick<Storage, 'setItem' | 'removeItem'>,
) {
  try {
    const target = storage ?? localStorage;
    if (!columns) target.removeItem(VISIBLE_COLUMNS_KEY);
    else {
      const normalized = canonical(columns);
      if (normalized.length)
        target.setItem(
          VISIBLE_COLUMNS_KEY,
          JSON.stringify({ version: 2, columns: normalized }),
        );
    }
  } catch {
    /* Selection remains usable without persistence. */
  }
}
export function toggleVisibleColumn(
  current: readonly LibraryDataColumn[],
  id: LibraryDataColumn,
  checked: boolean,
): LibraryDataColumn[] {
  const next = canonical(
    checked ? [...current, id] : current.filter((column) => column !== id),
  );
  return next.length ? next : [...current];
}
