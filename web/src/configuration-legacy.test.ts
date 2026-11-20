import { describe, expect, it } from 'vitest';
import { readLegacyConfiguration } from './configuration-legacy';
import type { AppSettings } from './settings-api';
const settings: AppSettings = {
  version: 4,
  player_name: 'Existing Player',
  analysis_engine: 'gnubg',
  analysis_plies: 3,
  play_engine: 'gnubg',
  play_plies: 1,
  gnubg_path: '/machine/gnubg',
  default_play_mode: 'tutor',
  tutor_threshold: 0.1,
  training_auto_analysis: false,
  bearoff_direction: 'left',
};
const book = {
  version: 2,
  provider: 'off',
  path: '/books/a.sqlite',
  xg_path: '/books/b.ob',
};
function storage(entries: Record<string, string>): Storage {
  return {
    getItem: (key) => entries[key] ?? null,
    setItem: () => {},
    removeItem: () => {},
    clear: () => {},
    key: () => null,
    length: Object.keys(entries).length,
  };
}
describe('legacy preferences import', () => {
  it('combines validated browser preferences with authoritative backend settings', () => {
    const result = readLegacyConfiguration(
      settings,
      book,
      storage({
        'neondojo.locale': 'de',
        'neondojo.theme': 'walnut',
        'neondojo.play-sounds': 'off',
        'neondojo.ui-appearance': JSON.stringify({
          version: 1,
          theme: 'paper',
          mode: 'dark',
        }),
        'neondojo.workspace': 'private selection',
      }),
    );
    expect(result.general).toEqual({
      player_name: 'Existing Player',
      locale: 'de',
      play_sounds: false,
    });
    expect(result.analysis.plies).toBe(3);
    expect(result.appearance).toMatchObject({
      ui_theme: 'paper',
      mode: 'dark',
      board_theme: 'walnut',
      bearoff_direction: 'left',
    });
    expect(result.opening_book).toEqual({
      provider: 'off',
      path: '/books/a.sqlite',
      xg_path: '/books/b.ob',
    });
    expect(JSON.stringify(result)).not.toContain('private selection');
    expect(JSON.stringify(result)).not.toContain('/machine/gnubg');
  });
  it('uses the existing defaults when legacy values are malformed', () => {
    const result = readLegacyConfiguration(
      settings,
      book,
      storage({
        'neondojo.locale': 'no-such-language',
        'neondojo.theme': 'unknown',
        'neondojo.library.visibleColumns': '{broken',
        'neondojo.database': 'invalid',
      }),
    );
    expect(result.general.locale).toBe('en');
    expect(result.appearance.board_theme).toBe('forest');
    expect(result.library.visible_columns.length).toBeGreaterThan(0);
  });

  it('uses legacy defaults when older backend settings omit play options', () => {
    const result = readLegacyConfiguration(
      {
        ...settings,
        default_play_mode: undefined,
        tutor_threshold: undefined,
        training_auto_analysis: undefined,
        bearoff_direction: undefined,
      },
      book,
      storage({}),
    );

    expect(result.play).toMatchObject({
      default_mode: 'coach',
      tutor_threshold: 0.04,
      training_auto_analysis: true,
    });
    expect(result.appearance.bearoff_direction).toBe('right');
  });
});
