import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import type { Theme } from '../shared/board/themes';

export const themeStorageKey = 'neondojo.theme';

export function readTheme(storage?: Pick<Storage, 'getItem'>): Theme {
  try {
    const value = (storage ?? localStorage).getItem(themeStorageKey);
    return value && configurationOptions.board_themes.includes(value)
      ? value
      : 'forest';
  } catch {
    return 'forest';
  }
}

export function saveTheme(theme: Theme, storage?: Pick<Storage, 'setItem'>) {
  try {
    (storage ?? localStorage).setItem(themeStorageKey, theme);
  } catch {
    // The selected board remains usable when persistent storage is unavailable.
  }
}
