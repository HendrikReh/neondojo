export const uiThemes = ['graphite', 'club', 'paper'] as const;
export type UiTheme = (typeof uiThemes)[number];
export type UiMode = 'light' | 'dark' | 'system';
export type ResolvedUiMode = Exclude<UiMode, 'system'>;
export interface UiAppearance {
  theme: UiTheme;
  mode: UiMode;
}

export const uiAppearanceStorageKey = 'bgtutor.ui-appearance';
export const defaultUiAppearance: UiAppearance = {
  theme: 'graphite',
  mode: 'system',
};

export function readUiAppearance(
  storage?: Pick<Storage, 'getItem'>,
): UiAppearance {
  try {
    const value: unknown = JSON.parse(
      (storage ?? localStorage).getItem(uiAppearanceStorageKey) ?? 'null',
    );
    if (
      value &&
      typeof value === 'object' &&
      'version' in value &&
      value.version === 1 &&
      'theme' in value &&
      uiThemes.includes(value.theme as UiTheme) &&
      'mode' in value &&
      ['light', 'dark', 'system'].includes(value.mode as string)
    ) {
      return { theme: value.theme as UiTheme, mode: value.mode as UiMode };
    }
  } catch {
    // A fresh preference remains usable when browser storage is unavailable.
  }
  return { ...defaultUiAppearance };
}

export function saveUiAppearance(
  value: UiAppearance,
  storage?: Pick<Storage, 'setItem'>,
) {
  try {
    (storage ?? localStorage).setItem(
      uiAppearanceStorageKey,
      JSON.stringify({ version: 1, ...value }),
    );
  } catch {
    // Applying appearance in this session does not depend on persistent storage.
  }
}

export function resolveUiMode(
  mode: UiMode,
  systemDark: boolean,
): ResolvedUiMode {
  return mode === 'system' ? (systemDark ? 'dark' : 'light') : mode;
}

interface ColorSchemeQuery {
  matches: boolean;
  addEventListener: (
    event: 'change',
    listener: (event: { matches: boolean }) => void,
  ) => void;
  removeEventListener: (
    event: 'change',
    listener: (event: { matches: boolean }) => void,
  ) => void;
}

export function systemIsDark() {
  return typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : false;
}

export function observeSystemAppearance(
  onChange: (dark: boolean) => void,
  query?: ColorSchemeQuery,
) {
  const media = query ?? window.matchMedia('(prefers-color-scheme: dark)');
  const listener = (event: { matches: boolean }) => onChange(event.matches);
  onChange(media.matches);
  media.addEventListener('change', listener);
  return () => media.removeEventListener('change', listener);
}

export function applyUiAppearance(
  value: UiAppearance,
  systemDark: boolean,
  root: {
    dataset: DOMStringMap;
    style: Pick<CSSStyleDeclaration, 'colorScheme'>;
  } = document.documentElement,
) {
  const mode = resolveUiMode(value.mode, systemDark);
  root.dataset.uiTheme = value.theme;
  root.dataset.uiMode = mode;
  root.dataset.uiPreference = value.mode;
  root.style.colorScheme = mode;
}
