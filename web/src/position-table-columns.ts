import { createColumnLayout } from './column-widths';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };

export const POSITION_COLUMNS = [
  { id: 'select', className: 'position-col-select', min: 32, defaultWidth: 36 },
  {
    id: 'origin',
    className: 'position-col-origin',
    min: 112,
    defaultWidth: 228,
  },
  {
    id: 'database',
    className: 'position-col-database',
    min: 70,
    defaultWidth: 130,
  },
  { id: 'game', className: 'position-col-game', min: 44, defaultWidth: 84 },
  { id: 'ply', className: 'position-col-ply', min: 44, defaultWidth: 84 },
  {
    id: 'player',
    className: 'position-col-player',
    min: 70,
    defaultWidth: 156,
  },
  {
    id: 'opponent',
    className: 'position-col-opponent',
    min: 70,
    defaultWidth: 144,
  },
  { id: 'dice', className: 'position-col-dice', min: 44, defaultWidth: 76 },
  { id: 'score', className: 'position-col-score', min: 60, defaultWidth: 100 },
  { id: 'cube', className: 'position-col-cube', min: 44, defaultWidth: 86 },
  {
    id: 'played_move',
    className: 'position-col-played',
    min: 80,
    defaultWidth: 140,
  },
  { id: 'loss', className: 'position-col-loss', min: 60, defaultWidth: 132 },
  { id: 'decks', className: 'position-col-decks', min: 80, defaultWidth: 144 },
  { id: 'notes', className: 'position-col-notes', min: 120, defaultWidth: 200 },
  {
    id: 'actions',
    className: 'position-col-actions',
    min: 60,
    defaultWidth: 60,
  },
] as const;
const baseLayout = createColumnLayout(
  POSITION_COLUMNS,
  'bgtutor.positions.columns',
  { decks: 144 },
);
export const POSITION_COLUMN_LAYOUT: typeof baseLayout = {
  ...baseLayout,
  read(storage) {
    try {
      let raw = (storage ?? localStorage).getItem(baseLayout.storageKey);
      if (!raw || raw.length > 4096) return;
      const profile = JSON.parse(raw);
      const actions = profile?.widths?.actions;
      if (Number.isInteger(actions) && actions >= 40 && actions < 60) {
        profile.widths.actions = 60;
        raw = JSON.stringify(profile);
      }
      return baseLayout.read({ getItem: () => raw });
    } catch {
      return;
    }
  },
};
export type PositionColumn = (typeof POSITION_COLUMNS)[number]['id'];
export type PositionDataColumn = Exclude<
  PositionColumn,
  'select' | 'actions' | 'played_move'
>;
export const POSITION_DATA_COLUMNS = POSITION_COLUMNS.map(
  ({ id }) => id,
).filter(
  (id): id is PositionDataColumn =>
    id !== 'select' && id !== 'actions' && id !== 'played_move',
);
export const DEFAULT_POSITION_COLUMNS = POSITION_DATA_COLUMNS.filter((id) =>
  configurationOptions.default_position_columns.includes(id),
);
export const POSITION_PLAYED_KEY = 'bgtutor.positions.showPlayed';
export function readPositionPlayed() {
  try {
    return localStorage.getItem(POSITION_PLAYED_KEY) === 'true';
  } catch {
    return false;
  }
}
export function savePositionPlayed(value: boolean) {
  try {
    localStorage.setItem(POSITION_PLAYED_KEY, String(value));
  } catch {
    /* Keep the active view usable without persistence. */
  }
}
