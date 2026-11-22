import {
  createColumnLayout,
  type ColumnWidths as SharedColumnWidths,
} from './shared/hooks/column-widths';

export { MAX_COLUMN_WIDTH } from './shared/hooks/column-widths';

export const LIBRARY_COLUMNS = [
  { id: 'select', className: 'library-col-select', min: 32, defaultWidth: 32 },
  { id: 'id', className: 'library-col-id', min: 44, defaultWidth: 60 },
  { id: 'white', className: 'library-col-white', min: 60, defaultWidth: 180 },
  { id: 'white_pr', className: 'library-col-pr', min: 44, defaultWidth: 56 },
  { id: 'black', className: 'library-col-black', min: 60, defaultWidth: 180 },
  { id: 'black_pr', className: 'library-col-pr', min: 44, defaultWidth: 56 },
  {
    id: 'play_mode',
    className: 'library-col-play-mode',
    min: 90,
    defaultWidth: 210,
  },
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
    // Retain this width in the saved layout format for existing profiles.
    id: 'actions',
    className: 'library-col-actions',
    min: 160,
    defaultWidth: 160,
  },
] as const;
export type LibraryColumn = (typeof LIBRARY_COLUMNS)[number]['id'];
export type ColumnWidths = SharedColumnWidths<LibraryColumn>;
export const COLUMN_WIDTHS_KEY = 'neondojo.library.columns';
const baseLayout = createColumnLayout(LIBRARY_COLUMNS, COLUMN_WIDTHS_KEY, {
  comment: 220,
  play_mode: 210,
});
export const LIBRARY_COLUMN_LAYOUT: typeof baseLayout = {
  ...baseLayout,
  read(storage) {
    try {
      let raw = (storage ?? localStorage).getItem(COLUMN_WIDTHS_KEY);
      if (!raw || raw.length > 4096) return;
      const profile = JSON.parse(raw);
      const widths = profile?.widths;
      if (
        Number.isInteger(widths?.actions) &&
        widths.actions >= 88 &&
        widths.actions < 160
      ) {
        widths.actions = 160;
        raw = JSON.stringify(profile);
      }
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

// Search context columns have widths, but do not participate in the ordinary
// match-column visibility or ordering preferences.
export const OCCURRENCE_COLUMNS = [
  {
    id: 'occurrence_database',
    className: 'library-col-occurrence-database',
    min: 70,
    defaultWidth: 160,
  },
  {
    id: 'occurrence_game',
    className: 'library-col-occurrence-game',
    min: 44,
    defaultWidth: 70,
  },
  {
    id: 'occurrence_move',
    className: 'library-col-occurrence-move',
    min: 44,
    defaultWidth: 70,
  },
] as const;
export const LIBRARY_WIDTH_COLUMNS = [
  ...OCCURRENCE_COLUMNS,
  ...LIBRARY_COLUMNS,
] as const;
const resultLayout = createColumnLayout(
  LIBRARY_WIDTH_COLUMNS,
  COLUMN_WIDTHS_KEY,
  Object.fromEntries(
    OCCURRENCE_COLUMNS.map((column) => [column.id, column.defaultWidth]),
  ),
);
export const LIBRARY_RESIZABLE_LAYOUT: typeof resultLayout = {
  ...resultLayout,
  read(storage) {
    const normal = LIBRARY_COLUMN_LAYOUT.read(storage);
    if (!normal) return;
    try {
      const raw = (storage ?? localStorage).getItem(COLUMN_WIDTHS_KEY);
      if (!raw || raw.length > 4096) return;
      const saved = JSON.parse(raw).widths;
      const occurrence = Object.fromEntries(
        OCCURRENCE_COLUMNS.filter((column) =>
          Object.hasOwn(saved, column.id),
        ).map((column) => [column.id, saved[column.id]]),
      );
      return resultLayout.read({
        getItem: () =>
          JSON.stringify({ version: 1, widths: { ...normal, ...occurrence } }),
      });
    } catch {
      return;
    }
  },
};
