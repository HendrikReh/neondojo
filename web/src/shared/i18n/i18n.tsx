import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useRef,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import {
  defaultLocale,
  formatLocalizedError,
  localizeDecision,
  numberFormat,
  readLocale,
  saveLocale,
  translate,
  type Locale,
  type TranslationParams,
} from './i18n-core';
import {
  loadLocale,
  localeMessages,
  type TranslationKey,
} from './i18n-catalogs';

export { languages, type Locale } from './i18n-core';
export type { TranslationKey } from './i18n-catalogs';

const createFormatters = (locale: Locale) => ({
  t: (key: TranslationKey, params?: TranslationParams) =>
    translate(locale, key, params),
  formatNumber: (value: number, options?: Intl.NumberFormatOptions) =>
    numberFormat(locale, options).format(value),
  formatPercent: (value: number, digits = 0) =>
    numberFormat(locale, {
      style: 'percent',
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value),
  formatEquity: (value: number, digits = 3) =>
    numberFormat(locale, {
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
  formatPoints: (count: number) => translate(locale, 'board.points', { count }),
});

const I18nContext = createContext({
  locale: defaultLocale,
  setLocale: (_locale: Locale) => {},
  ...createFormatters(defaultLocale),
});

export function I18nProvider({
  children,
  initialLocale,
}: {
  children: ReactNode;
  initialLocale?: Locale;
}) {
  const [locale, updateLocale] = useState<Locale>(
    () => initialLocale ?? readLocale(),
  );
  const request = useRef(0);
  const [failed, setFailed] = useState(false);
  const setLocale = useCallback((next: Locale) => {
    const id = ++request.current;
    setFailed(false);
    void loadLocale(next).then(
      () => {
        if (id === request.current) updateLocale(next);
      },
      () => {
        if (id === request.current) setFailed(true);
      },
    );
  }, []);
  useLayoutEffect(() => {
    saveLocale(locale);
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo(
    () => ({ locale, setLocale, ...createFormatters(locale) }),
    [locale, setLocale],
  );
  return (
    <I18nContext.Provider value={value}>
      {failed && (
        <div className="error" role="alert">
          {localeMessages(locale).workspace.chunkFailed}{' '}
          <button onClick={() => window.location.reload()}>
            {localeMessages(locale).workspace.reloadPage}
          </button>
        </div>
      )}
      {children}
    </I18nContext.Provider>
  );
}

export const useI18n = () => useContext(I18nContext);
