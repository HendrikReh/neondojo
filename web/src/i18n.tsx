import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  defaultLocale,
  formatLocalizedError,
  localizeDecision,
  readLocale,
  saveLocale,
  translate,
  type Locale,
  type TranslationParams,
} from './i18n-core';
import type { TranslationKey } from './i18n-catalogs';

export { languages, type Locale } from './i18n-core';
export type { TranslationKey } from './i18n-catalogs';

const createFormatters = (locale: Locale) => ({
  t: (key: TranslationKey, params?: TranslationParams) =>
    translate(locale, key, params),
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) =>
    new Intl.NumberFormat(locale, options).format(value),
  formatPercent: (value: number, digits = 0) =>
    new Intl.NumberFormat(locale, {
      style: 'percent',
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value),
  formatEquity: (value: number, digits = 3) =>
    new Intl.NumberFormat(locale, {
      signDisplay: 'always',
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value),
  formatDecision: (
    notation: string,
    position?: Parameters<typeof localizeDecision>[2],
  ) => localizeDecision(locale, notation, position),
  formatError: (error: unknown) => formatLocalizedError(locale, error),
  /** "1 point" or "5 points", in the interface language. */
  formatPoints: (count: number) =>
    translate(locale, count === 1 ? 'board.point' : 'board.points', {
      count,
    }),
});

const I18nContext = createContext({
  locale: defaultLocale,
  setLocale: (_locale: Locale) => {},
  ...createFormatters(defaultLocale),
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, updateLocale] = useState<Locale>(readLocale);
  const setLocale = useCallback((next: Locale) => updateLocale(next), []);
  useEffect(() => {
    saveLocale(locale);
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo(
    () => ({ locale, setLocale, ...createFormatters(locale) }),
    [locale, setLocale],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export const useI18n = () => useContext(I18nContext);
