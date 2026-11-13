import { numberedSearchParams } from './cursor-search';
import {
  downloadResponse,
  request,
  requestResponse,
  type Position,
} from './api';
import type { components } from './generated-api';
import type { PlayMode } from './play-mode-messages';
type Schema = components['schemas'];

export type Database = Schema['Database'];
export const BOT_DATABASE_ID = '00000000-0000-0000-0000-000000000003';
export const PAUSED_MATCHES_DATABASE_ID =
  '00000000-0000-0000-0000-000000000004';
export const acceptsLibraryMatches = (id: string) =>
  id !== PAUSED_MATCHES_DATABASE_ID;
export type LibraryMatch = Required<
  Omit<
    Schema['DatabaseMatch'],
    | 'play_session_id'
    | 'play_phase'
    | 'play_paused'
    | 'play_mode'
    | 'play_assisted'
    | 'occurrence'
    | 'player_sides'
    | 'study_report_id'
  >
> &
  Pick<
    Schema['DatabaseMatch'],
    'play_session_id' | 'play_phase' | 'player_sides'
  > & {
    play_paused?: Schema['DatabaseMatch']['play_paused'];
    play_mode?: PlayMode | null;
    play_assisted?: boolean;
    occurrence?: Schema['LibraryOccurrence'] | null;
    study_report_id?: string | null;
  };
export const libraryRowKey = (match: LibraryMatch): string =>
  match.occurrence?.id ?? match.id;
export type OccurrenceSort =
  'occurrence_database' | 'occurrence_game' | 'occurrence_move';
export const isOccurrenceSort = (sort: LibrarySort): sort is OccurrenceSort =>
  ['occurrence_database', 'occurrence_game', 'occurrence_move'].includes(sort);
export type LibrarySort =
  | OccurrenceSort
  | 'id'
  | 'name'
  | 'white'
  | 'white_pr'
  | 'black'
  | 'black_pr'
  | 'match_length'
  | 'result'
  | 'score'
  | 'played_at'
  | 'created_at'
  | 'tournament'
  | 'format'
  | 'position_count'
  | 'analysis_status'
  | 'tags'
  | 'comment';
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
  sort: LibrarySort,
  direction: string,
  page = 1,
) {
  return numberedSearchParams(
    50,
    { sort, direction, database_id: databaseId, ...filters },
    page,
  );
}
export const loadLibrary = (
  databaseId: string,
  filters: LibraryFilters,
  sort: LibrarySort,
  direction: string,
  signal: AbortSignal,
  page: number,
) =>
  request<Omit<Schema['LibraryPage'], 'items'> & { items: LibraryMatch[] }>(
    `/library/matches?${libraryParams(databaseId, filters, sort, direction, page)}`,
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
  const response = await requestResponse('/library/export', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ match_ids: ids, format }),
  });
  await downloadResponse(
    response,
    ids.length > 1
      ? 'matches.zip'
      : `match.${format === 'original' ? 'sgf' : format}`,
  );
}
export function readDatabase() {
  try {
    const value = localStorage.getItem('bgtutor.database');
    return value && /^[a-f0-9-]{36}$/i.test(value) ? value : '';
  } catch {
    return '';
  }
}
export function saveDatabase(id: string) {
  try {
    localStorage.setItem('bgtutor.database', id);
  } catch {
    /* Read-only storage. */
  }
}
