import { createColumnLayout } from './column-widths';

export const POSITION_COLUMNS = [
  { id: 'select', className: 'position-col-select', min: 32 },
  { id: 'origin', className: 'position-col-origin', min: 112 },
  { id: 'database', className: 'position-col-database', min: 70 },
  { id: 'game', className: 'position-col-game', min: 44 },
  { id: 'ply', className: 'position-col-ply', min: 44 },
  { id: 'player', className: 'position-col-player', min: 70 },
  { id: 'opponent', className: 'position-col-opponent', min: 70 },
  { id: 'dice', className: 'position-col-dice', min: 44 },
  { id: 'score', className: 'position-col-score', min: 60 },
  { id: 'cube', className: 'position-col-cube', min: 44 },
  {
    id: 'played_move',
    className: 'position-col-played',
    min: 80,
    defaultWidth: 128,
  },
  { id: 'loss', className: 'position-col-loss', min: 60 },
  { id: 'decks', className: 'position-col-decks', min: 80, defaultWidth: 144 },
  { id: 'notes', className: 'position-col-notes', min: 120 },
  { id: 'actions', className: 'position-col-actions', min: 40 },
] as const;
export const POSITION_COLUMN_LAYOUT = createColumnLayout(
  POSITION_COLUMNS,
  'bgtutor.positions.columns',
  { decks: 144 },
);
export type PositionColumn = (typeof POSITION_COLUMNS)[number]['id'];
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
