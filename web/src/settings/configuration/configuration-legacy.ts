import {
  defaultConfiguration,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import { applySettingsDraft } from './configuration-settings';
import type { AppSettings } from '../settings-api';
import type { BookConfig } from '../../workspaces/analysis/candidates/source-analysis-api';
import { readLocale, saveLocale } from '../../shared/i18n/i18n-core';
import { readTheme, saveTheme } from '../ui-preferences';
import {
  readUiAppearance,
  saveUiAppearance,
} from '../appearance/ui-appearance';
import { readPlaySounds, savePlaySounds } from '../../shared/board/play-audio';
import { readVisibleColumns } from '../../workspaces/library/library-column-preferences';
import { isCollectionScope } from '../../workspaces/study/flashcards/card-search-api';
import { LIBRARY_RESIZABLE_LAYOUT as LIBRARY_COLUMN_LAYOUT } from '../../workspaces/library/library-columns';
import {
  POSITION_COLUMN_LAYOUT,
  POSITION_PLAYED_KEY,
} from '../../workspaces/positions/position-table-columns';

export function readLegacyConfiguration(
  settings: AppSettings,
  book: BookConfig,
  storage?: Storage,
): ProfileConfiguration {
  const c = applySettingsDraft(structuredClone(defaultConfiguration), {
    settings,
    locale: readLocale(storage),
    sounds: readPlaySounds(storage),
    appearance: readUiAppearance(storage),
    board: {
      theme: readTheme(storage),
      checkerTheme: null,
      diceTheme: null,
      cubeTheme: null,
    },
  });
  c.opening_book = {
    provider: book.provider as ProfileConfiguration['opening_book']['provider'],
    path: book.path,
    xg_path: book.xg_path,
  };
  c.library.visible_columns = readVisibleColumns(storage);
  c.library.column_widths = LIBRARY_COLUMN_LAYOUT.read(storage) ?? null;
  c.positions.column_widths = POSITION_COLUMN_LAYOUT.read(storage) ?? null;
  try {
    const source = storage ?? localStorage;
    c.positions.show_played = source.getItem(POSITION_PLAYED_KEY) === 'true';
    const database = source.getItem('neondojo.database');
    if (
      database &&
      /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(database)
    )
      c.library.database_id = database;
    const deck = source.getItem('neondojo.deck');
    if (
      deck &&
      (isCollectionScope(deck) ||
        /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(deck))
    )
      c.learning.deck_id = deck;
  } catch {
    /* Existing defaults also work when storage is unavailable. */
  }
  return normalizeConfiguration(c);
}

/** Projection for first-paint appearance and audio, never an independent source. */
export function cacheConfiguration(c: ProfileConfiguration) {
  saveLocale(c.general.locale);
  saveTheme(c.appearance.board_theme);
  saveUiAppearance({ theme: c.appearance.ui_theme, mode: c.appearance.mode });
  savePlaySounds(c.general.play_sounds);
}
