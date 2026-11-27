import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import type { Theme } from '../shared/board/themes';

export const themeStorageKey = 'neondojo.theme';
export const archiveThemesPreloadKey = 'neondojo.archive-themes-preload';

export interface BoardArtworkThemes {
  theme?: string | null;
  checkerTheme?: string | null;
  diceTheme?: string | null;
  cubeTheme?: string | null;
}

export function usesArchiveTheme(artwork: BoardArtworkThemes): boolean {
  return [
    artwork.theme,
    artwork.checkerTheme,
    artwork.diceTheme,
    artwork.cubeTheme,
  ].some((value) => typeof value === 'string' && value.startsWith('bgblitz:'));
}

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

export function readArchiveThemesPreloadHint(
  storage?: Pick<Storage, 'getItem'>,
): boolean {
  if (readTheme(storage).startsWith('bgblitz:')) return true;
  try {
    return (storage ?? localStorage).getItem(archiveThemesPreloadKey) === '1';
  } catch {
    return false;
  }
}

export function saveArchiveThemesPreloadHint(
  preload: boolean,
  storage?: Pick<Storage, 'setItem' | 'removeItem'>,
) {
  try {
    const target = storage ?? localStorage;
    if (preload) target.setItem(archiveThemesPreloadKey, '1');
    else target.removeItem(archiveThemesPreloadKey);
  } catch {
    // Startup preload hint is best-effort when persistent storage is unavailable.
  }
}
