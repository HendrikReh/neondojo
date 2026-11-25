import type { components } from '../../generated-api';
import { PAUSED_MATCHES_DATABASE_ID, type Database } from './library-api';
import { defineMessages } from '../../shared/i18n/messages';
type Key = keyof typeof messages.catalogs.en;
const messages = defineMessages('library');
export const libraryMessages = messages.catalogs;
export const libraryText = messages.text;
export const useLibraryText = messages.useText;
export function databaseDisplayName(
  database: Pick<Database, 'id' | 'name'>,
  t: ReturnType<typeof useLibraryText>,
) {
  return database.id === PAUSED_MATCHES_DATABASE_ID
    ? t('pausedMatches')
    : database.name;
}
export type LibraryMessageKey = Key;

export type ImportWarning = components['schemas']['ImportWarning'];
const importWarningKeys = {
  illegal_play_replaced: 'importWarningIllegalPlay',
  custom_start_position: 'importWarningCustomStart',
  unfinished_game: 'importWarningUnfinishedGame',
  illegal_play_truncated: 'importWarningIllegalTruncated',
  unreadable_play_truncated: 'importWarningUnreadableTruncated',
  score_mismatch: 'importWarningScoreMismatch',
} as const satisfies Record<ImportWarning['kind'], Key>;
const score = (value: readonly number[] | null | undefined) =>
  value ? value.join(':') : '–';
export function importWarningText(
  warning: ImportWarning,
  t: ReturnType<typeof useLibraryText>,
) {
  return t(importWarningKeys[warning.kind], {
    game: warning.game_no,
    ply: warning.source_ply ?? '–',
    // A line reference, not a quantity: no digit grouping.
    line: warning.source_line == null ? '–' : String(warning.source_line),
    text: warning.source_text ?? '',
    expected: score(warning.expected_score),
    recorded: score(warning.recorded_score),
  });
}
