import {
  createColumnLayout,
  type ColumnWidths as SharedColumnWidths,
} from './column-widths';

export { MAX_COLUMN_WIDTH } from './column-widths';

export const LIBRARY_COLUMNS = [
  { id: 'select', className: 'library-col-select', min: 32, defaultWidth: 32 },
  { id: 'id', className: 'library-col-id', min: 44, defaultWidth: 60 },
  { id: 'white', className: 'library-col-white', min: 60, defaultWidth: 180 },
  { id: 'white_pr', className: 'library-col-pr', min: 44, defaultWidth: 56 },
  { id: 'black', className: 'library-col-black', min: 60, defaultWidth: 180 },
  { id: 'black_pr', className: 'library-col-pr', min: 44, defaultWidth: 56 },
  {
    id: 'match_length',
    className: 'library-col-length',
    min: 44,
    defaultWidth: 86,
  },
  { id: 'score', className: 'library-col-result', min: 54, defaultWidth: 76 },
  {
    id: 'played_at',
    className: 'library-col-date',
    min: 70,
    defaultWidth: 100,
  },
  {
    id: 'created_at',
    className: 'library-col-date',
    min: 70,
    defaultWidth: 100,
  },
  {
    id: 'tournament',
    className: 'library-col-tournament',
    min: 70,
    defaultWidth: 140,
  },
  {
    id: 'position_count',
    className: 'library-col-positions',
    min: 70,
    defaultWidth: 96,
  },
  {
    id: 'analysis_status',
    className: 'library-col-analysis',
    min: 70,
    defaultWidth: 126,
  },
  { id: 'tags', className: 'library-col-tags', min: 96, defaultWidth: 160 },
  {
    id: 'comment',
    className: 'library-col-comment',
    min: 96,
    defaultWidth: 220,
  },
  {
    id: 'actions',
    className: 'library-col-actions',
    min: 88,
    defaultWidth: 88,
  },
] as const;
export type LibraryColumn = (typeof LIBRARY_COLUMNS)[number]['id'];
export type ColumnWidths = SharedColumnWidths<LibraryColumn>;
export const COLUMN_WIDTHS_KEY = 'bgtutor.library.columns';
const baseLayout = createColumnLayout(LIBRARY_COLUMNS, COLUMN_WIDTHS_KEY, {
  comment: 220,
});
export const LIBRARY_COLUMN_LAYOUT: typeof baseLayout = {
  ...baseLayout,
  read(storage) {
    try {
      const raw = (storage ?? localStorage).getItem(COLUMN_WIDTHS_KEY);
      if (!raw || raw.length > 4096) return;
      const profile = JSON.parse(raw);
      const widths = profile?.widths;
      if (
        profile?.version === 1 &&
        widths &&
        !Array.isArray(widths) &&
        typeof widths === 'object' &&
        !('tags' in widths) &&
        'notes' in widths
      ) {
        if (
          !Number.isInteger(widths.notes) ||
          widths.notes < 96 ||
          widths.notes > 2400
        )
          return;
        const { notes, ...other } = widths;
        return baseLayout.read({
          getItem: () =>
            JSON.stringify({ version: 1, widths: { ...other, tags: notes } }),
        });
      }
      return baseLayout.read({ getItem: () => raw });
    } catch {
      return;
    }
  },
};
export const minimumColumnWidth = LIBRARY_COLUMN_LAYOUT.minimum;
export const readColumnWidths = LIBRARY_COLUMN_LAYOUT.read;
export const saveColumnWidths = LIBRARY_COLUMN_LAYOUT.save;
export const resizeColumn = LIBRARY_COLUMN_LAYOUT.resize;
export const totalColumnWidth = LIBRARY_COLUMN_LAYOUT.total;
