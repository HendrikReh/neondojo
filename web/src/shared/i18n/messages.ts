import { useCallback } from 'react';
import { useI18n } from './i18n';
import {
  formatMessage,
  type Locale,
  type TranslationParams,
} from './i18n-core';

/** The order of the translations in a message row. */
export const rowLocales = [
  'en',
  'de',
  'es',
  'fr',
  'pl',
  'it',
] as const satisfies readonly Locale[];

/** One message in every language, in the order of `rowLocales`. */
export type MessageRow = readonly [
  en: string,
  de: string,
  es: string,
  fr: string,
  pl: string,
  it: string,
];

/** A feature's messages in every language; every language has every key. */
export type Catalogs<K extends string> = Readonly<
  Record<Locale, Readonly<Record<K, string>>>
>;

/** Builds per-language catalogs from rows of six translations. */
export function fromRows<K extends string>(
  rows: Readonly<Record<K, MessageRow>>,
): Catalogs<K> {
  const entries = Object.entries(rows) as [K, MessageRow][];
  return Object.fromEntries(
    rowLocales.map((locale, index) => [
      locale,
      Object.fromEntries(entries.map(([key, row]) => [key, row[index]])),
    ]),
  ) as Catalogs<K>;
}

export type MessageText<K extends string> = (
  key: K,
  params?: TranslationParams,
) => string;

export interface Messages<K extends string> {
  readonly name: string;
  readonly catalogs: Catalogs<K>;
  /** The message in this language, with localized numbers and plurals. */
  text(locale: Locale, key: K, params?: TranslationParams): string;
  /** `text` in the interface language. */
  useText(): MessageText<K>;
}

const registry = new Map<string, Catalogs<string>>();

/** Every catalog defined so far, by name, for the completeness tests. */
export const definedMessages: ReadonlyMap<string, Catalogs<string>> = registry;

/**
 * Defines a feature's messages. The English catalog sets the keys, and every
 * other language must translate each of them; messages.test.ts checks the
 * keys, placeholders and plural forms of every defined catalog.
 */
export function defineMessages<K extends string>(
  name: string,
  catalogs: { readonly en: Readonly<Record<K, string>> } & Readonly<
    Record<Exclude<Locale, 'en'>, Readonly<Record<NoInfer<K>, string>>>
  >,
): Messages<K> {
  // Set, not add: a module re-evaluated by hot reload defines its catalog again.
  registry.set(name, catalogs);
  const text = (locale: Locale, key: K, params: TranslationParams = {}) =>
    formatMessage(locale, catalogs[locale][key], params);
  return {
    name,
    catalogs,
    text,
    useText() {
      const { locale } = useI18n();
      return useCallback(
        (key: K, params?: TranslationParams) => text(locale, key, params),
        [locale],
      );
    },
  };
}
