import {
  defaultConfiguration,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import type { AppSettings } from './settings-api';
import type { BookConfig } from './source-analysis-api';
import { readLocale, saveLocale } from './i18n-core';
import { readTheme, saveTheme } from './ui-preferences';
import { readUiAppearance, saveUiAppearance } from './ui-appearance';
import { readPlaySounds, savePlaySounds } from './play-audio';
import { readVisibleColumns } from './library-column-preferences';
import { LIBRARY_RESIZABLE_LAYOUT as LIBRARY_COLUMN_LAYOUT } from './library-columns';
import {
  POSITION_COLUMN_LAYOUT,
  POSITION_PLAYED_KEY,
} from './position-table-columns';

export function readLegacyConfiguration(
  settings: AppSettings,
  book: BookConfig,
  storage?: Storage,
): ProfileConfiguration {
  const c = structuredClone(defaultConfiguration);
  c.general = {
    player_name: settings.player_name,
    locale: readLocale(storage),
    play_sounds: readPlaySounds(storage),
  };
  c.analysis.plies = settings.analysis_plies;
  c.analysis.engine = settings.analysis_engine;
  c.play = {
    ...c.play,
    engine: settings.play_engine,
    plies: settings.play_plies,
    default_mode: settings.default_play_mode ?? 'coach',
    tutor_threshold: settings.tutor_threshold ?? 0.04,
    training_auto_analysis: settings.training_auto_analysis ?? true,
  };
  const appearance = readUiAppearance(storage);
  c.appearance = {
    ...c.appearance,
    ui_theme: appearance.theme,
    mode: appearance.mode,
    board_theme: readTheme(storage),
    bearoff_direction: settings.bearoff_direction ?? 'right',
  };
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
    const database = source.getItem('bgtutor.database');
    if (
      database &&
      /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(database)
    )
      c.library.database_id = database;
    const deck = source.getItem('bgtutor.deck');
    if (
      deck &&
      (deck === '__all__' ||
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
