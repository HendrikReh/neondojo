import { themes, type Theme } from './themes';

export const themeStorageKey = 'bgtutor.theme';

export function readTheme(storage?: Pick<Storage, 'getItem'>): Theme {
  try {
    const value = (storage ?? localStorage).getItem(themeStorageKey);
    return themes.find(({ id }) => id === value)?.id ?? 'forest';
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
