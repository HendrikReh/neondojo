import { catalogs, type Catalog, type TranslationKey } from './i18n-catalogs';

export type Locale = keyof typeof catalogs;
export type TranslationParams = Record<string, string | number>;
export const defaultLocale: Locale = 'de';
export const localeStorageKey = 'bgtutor.locale';
export const languages: ReadonlyArray<{ id: Locale; name: string }> = [
  { id: 'de', name: 'Deutsch' },
  { id: 'es', name: 'Español' },
  { id: 'fr', name: 'Français' },
  { id: 'pl', name: 'Polski' },
  { id: 'it', name: 'Italiano' },
  { id: 'en', name: 'English' },
];

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && Object.hasOwn(catalogs, value);
}

export function readLocale(storage?: Pick<Storage, 'getItem'>): Locale {
  try {
    const value = (storage ?? localStorage).getItem(localeStorageKey);
    return isLocale(value) ? value : defaultLocale;
  } catch {
    return defaultLocale;
  }
}

export function saveLocale(locale: Locale, storage?: Pick<Storage, 'setItem'>) {
  try {
    (storage ?? localStorage).setItem(localeStorageKey, locale);
  } catch {
    // Language switching remains available when browser storage is disabled.
  }
}

const placeholderPattern = /\{([A-Za-z][A-Za-z0-9]*)\}/g;
const placeholders = (text: string) =>
  [
    ...new Set([...text.matchAll(placeholderPattern)].map((match) => match[1])),
  ].sort();

export function interpolate(template: string, params: TranslationParams = {}) {
  return template.replace(placeholderPattern, (_, key: string) => {
    if (!Object.hasOwn(params, key))
      throw new Error(`Missing translation parameter: ${key}`);
    return String(params[key]);
  });
}

/** Fail early if a translated sentence loses an argument or a catalog key. */
export function validateCatalogs(all: Record<Locale, Catalog>) {
  const keys = Object.keys(all.de) as TranslationKey[];
  for (const [locale, catalog] of Object.entries(all)) {
    if (Object.keys(catalog).length !== keys.length)
      throw new Error(`Incomplete catalog: ${locale}`);
    for (const key of keys) {
      if (
        !catalog[key] ||
        placeholders(catalog[key]).join(',') !==
          placeholders(all.de[key]).join(',')
      ) {
        throw new Error(`Invalid translation placeholders: ${locale}.${key}`);
      }
    }
  }
}
validateCatalogs(catalogs);

export function translate(
  locale: Locale,
  key: TranslationKey,
  params: TranslationParams = {},
) {
  const localized = Object.fromEntries(
    Object.entries(params).map(([name, value]) => [
      name,
      typeof value === 'number'
        ? new Intl.NumberFormat(locale).format(value)
        : value,
    ]),
  );
  return interpolate(catalogs[locale][key], localized);
}

/** Keep errors as structured data so a visible error also changes language. */
export class AppError extends Error {
  constructor(
    readonly key: TranslationKey,
    readonly params: TranslationParams = {},
    readonly diagnostic = '',
  ) {
    super(diagnostic || key);
    this.name = 'AppError';
  }
}

export function formatLocalizedError(locale: Locale, error: unknown): string {
  if (!error) return '';
  if (error instanceof AppError) {
    const message = translate(locale, error.key, error.params);
    return error.diagnostic ? `${message} ${error.diagnostic}` : message;
  }
  const diagnostic = error instanceof Error ? error.message : String(error);
  return `${translate(locale, 'error.generic')} ${diagnostic}`;
}

const decisionKeys: Record<string, TranslationKey> = {
  'no double': 'decision.noDouble',
  double: 'decision.double',
  take: 'decision.take',
  pass: 'decision.drop',
  drop: 'decision.drop',
  'cannot move': 'decision.cannotMove',
  dance: 'decision.cannotMove',
};

export function localizeDecision(locale: Locale, notation: string) {
  const key = decisionKeys[notation.trim().toLowerCase()];
  return key ? translate(locale, key) : notation;
}
