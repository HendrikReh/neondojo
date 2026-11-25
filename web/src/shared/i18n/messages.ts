import { useCallback } from 'react';
import { useI18n } from './i18n';
import { catalogsFor, type LocaleMessages } from './i18n-catalogs';
import {
  formatMessage,
  type Locale,
  type TranslationParams,
} from './i18n-core';

export type MessageText<K extends string> = (
  key: K,
  params?: TranslationParams,
) => string;

/** The English pack defines the keys. Each loaded language implements the same schema. */
export function defineMessages<N extends keyof LocaleMessages>(name: N) {
  type Key = keyof LocaleMessages[N] & string;
  const catalogs = catalogsFor(name);
  const text = (locale: Locale, key: Key, params?: TranslationParams) => {
    const catalog: Readonly<Record<string, string>> = catalogs[locale];
    const template = catalog[key];
    return typeof template === 'string'
      ? formatMessage(locale, template, params)
      : key;
  };
  return {
    catalogs,
    text,
    useText(): MessageText<Key> {
      const { locale } = useI18n();
      return useCallback((key, params) => text(locale, key, params), [locale]);
    },
  };
}
