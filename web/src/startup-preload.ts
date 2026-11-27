import { loadArchiveThemes } from './shared/board/themes';
import { loadLocale } from './shared/i18n/i18n-catalogs';
import type { Locale } from './shared/i18n/i18n-core';
import { readArchiveThemesPreloadHint } from './settings/ui-preferences';

export interface StartupPreloadDeps {
  loadLocale: (locale: Locale) => Promise<unknown>;
  loadArchiveThemes: () => Promise<unknown>;
  shouldPreloadArchiveThemes: () => boolean;
}

const defaultDeps: StartupPreloadDeps = {
  loadLocale,
  loadArchiveThemes,
  shouldPreloadArchiveThemes: () => readArchiveThemesPreloadHint(),
};

/**
 * Loads the initial UI locale before first paint and best-effort preloads
 * BGBlitz archive themes when the board or mixed piece artwork needs them.
 * Archive theme chunk failures are swallowed so LazyPanel + RetryableChunkError
 * handle recovery without wiping #root.
 */
export async function preloadStartupAssets(
  initialLocale: Locale,
  deps: StartupPreloadDeps = defaultDeps,
) {
  await Promise.all([
    deps.loadLocale(initialLocale),
    deps.shouldPreloadArchiveThemes()
      ? deps.loadArchiveThemes().catch(() => undefined)
      : undefined,
  ]);
}
