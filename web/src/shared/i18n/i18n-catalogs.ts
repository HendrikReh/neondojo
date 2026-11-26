import type { components } from '../../generated-api';
import type english from './locales/en';

export type LocaleMessages = typeof english;
export type Catalog = LocaleMessages['app'];
export type TranslationKey = keyof Catalog;

export type Locale = components['schemas']['ConfigurationLocale'];

const loaders: Record<Locale, () => Promise<{ default: LocaleMessages }>> = {
  en: () => import('./locales/en'),
  de: () => import('./locales/de'),
};

const loaded = new Map<Locale, LocaleMessages>();

export async function loadLocale(locale: Locale) {
  if (!loaded.has(locale)) {
    const { default: messages } = await loaders[locale]();
    loaded.set(locale, messages);
  }
}

export function localeMessages(locale: Locale): LocaleMessages {
  const messages = loaded.get(locale);
  if (!messages) throw new Error(`Language has not been loaded: ${locale}`);
  return messages;
}

/** Access catalogs by language, including languages prepared by the test harness. */
export function catalogsFor<N extends keyof LocaleMessages>(name: N) {
  const read = (locale: Locale) => localeMessages(locale)[name];
  return {
    get en() {
      return read('en');
    },
    get de() {
      return read('de');
    },
  };
}

export const catalogs = catalogsFor('app');
