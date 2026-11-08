import { request, type Page, type Position } from './api';
import { AppError } from './i18n-core';
import type { components } from './generated-api';
type Schema = components['schemas'];

export const defaultDatabaseId = '00000000-0000-0000-0000-000000000001';
export type Database = Schema['Database'];
export type LibraryMatch = Required<Schema['DatabaseMatch']>;
export type StoredMove = Schema['StoredMove'];
export type GameDecisions = Schema['GameDecisions'];
export const movePosition = (move: StoredMove): Position => ({
  id: move.position_uuid ?? move.id,
  match_uuid: move.match_uuid,
  ordinal: move.move_no,
  game_no: move.game_no,
  ply: move.move_no,
  player: move.player,
  dice: move.dice,
  board: move.board,
  position_id: move.position_id,
  gnubg_match_id: move.gnubg_match_id,
  notation: move.notation,
});
export type LibraryFilters = Record<
  | 'q'
  | 'player'
  | 'date_from'
  | 'date_to'
  | 'date_field'
  | 'tag'
  | 'comment'
  | 'position_id'
  | 'tournament'
  | 'round'
  | 'match_name'
  | 'match_type'
  | 'result'
  | 'match_points_min'
  | 'match_points_max',
  string
>;
export const emptyFilters = (): LibraryFilters => ({
  q: '',
  player: '',
  date_from: '',
  date_to: '',
  date_field: 'played',
  tag: '',
  comment: '',
  position_id: '',
  tournament: '',
  round: '',
  match_name: '',
  match_type: '',
  result: '',
  match_points_min: '',
  match_points_max: '',
});
export function libraryParams(
  databaseId: string,
  filters: LibraryFilters,
  sort: string,
  direction: string,
  after?: string,
) {
  const params = new URLSearchParams({ sort, direction, limit: '50' });
  if (databaseId) params.set('database_id', databaseId);
  Object.entries(filters).forEach(([key, value]) => {
    if (value.trim()) params.set(key, value.trim());
  });
  if (after) params.set('after', after);
  return params;
}
export const loadLibrary = (
  databaseId: string,
  filters: LibraryFilters,
  sort: string,
  direction: string,
  signal: AbortSignal,
  after?: string,
) =>
  request<Page<LibraryMatch>>(
    `/library/matches?${libraryParams(databaseId, filters, sort, direction, after)}`,
    { signal },
  );
export const loadDecisions = (
  matchId: string,
  gameNo: number,
  signal: AbortSignal,
) =>
  request<GameDecisions>(`/matches/${matchId}/games/${gameNo}/decisions`, {
    signal,
  });
export type ActionItem = Schema['ActionItem'];
export type ActionResponse = Schema['ActionResponse'];
export type BatchAction = 'copy' | 'move' | 'delete' | 'tags';
export async function exportMatches(ids: string[], format: 'original' | 'sgf') {
  const response = await fetch('/api/library/export', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ match_ids: ids, format }),
  });
  if (!response.ok) {
    const text = await response.text();
    let detail = text;
    try {
      detail = JSON.parse(text).error ?? text;
    } catch {
      /* Plain HTTP errors. */
    }
    throw new AppError(
      'error.request',
      { status: String(response.status) },
      detail,
    );
  }
  const blob = await response.blob();
  const header = response.headers.get('Content-Disposition') ?? '';
  const filename =
    /filename="([^"]+)"/.exec(header)?.[1] ??
    (ids.length > 1
      ? 'matches.zip'
      : `match.${format === 'original' ? 'sgf' : format}`);
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function readDatabase() {
  try {
    const value = localStorage.getItem('bgtutor.database');
    return value && /^[a-f0-9-]{36}$/i.test(value) ? value : defaultDatabaseId;
  } catch {
    return defaultDatabaseId;
  }
}
export function saveDatabase(id: string) {
  try {
    localStorage.setItem('bgtutor.database', id);
  } catch {
    /* Read-only storage. */
  }
}
