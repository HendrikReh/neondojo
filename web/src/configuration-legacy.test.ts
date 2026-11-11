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
        'bgtutor.locale': 'de',
        'bgtutor.theme': 'walnut',
        'bgtutor.play-sounds': 'off',
        'bgtutor.ui-appearance': JSON.stringify({
          version: 1,
          theme: 'paper',
          mode: 'dark',
        }),
        'bgtutor.workspace': 'private selection',
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
        'bgtutor.locale': 'no-such-language',
        'bgtutor.theme': 'unknown',
        'bgtutor.library.visibleColumns': '{broken',
        'bgtutor.database': 'invalid',
      }),
    );
    expect(result.general.locale).toBe('en');
    expect(result.appearance.board_theme).toBe('forest');
    expect(result.library.visible_columns.length).toBeGreaterThan(0);
  });
});
