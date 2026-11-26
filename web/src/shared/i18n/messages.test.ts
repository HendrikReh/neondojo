import { describe, expect, it } from 'vitest';
import {
  formatMessage,
  placeholders,
  languages,
  validatePluralForms,
} from './i18n-core';
import { defineMessages } from './messages';
import { localeMessages, type LocaleMessages } from './i18n-catalogs';

const locales = languages.map(({ id }) => id);
const pluralName = /\{([A-Za-z][A-Za-z0-9]*), plural,/g;

/** Sample arguments: a number for each plural count, text otherwise. */
function sample(template: string, count: number) {
  const plural = new Set(
    [...template.matchAll(pluralName)].map((match) => match[1]),
  );
  return Object.fromEntries(
    placeholders(template).map((name) => [
      name,
      plural.has(name) ? count : `‹${name}›`,
    ]),
  );
}

const groups = Object.keys(localeMessages('en')) as (keyof LocaleMessages)[];

it('exposes every feature catalog through defineMessages', () => {
  const files = import.meta.glob<string>(
    ['/src/**/*-messages.ts', '!/src/shared/i18n/locales/**'],
    { eager: true, query: '?raw', import: 'default' },
  );
  expect(Object.keys(files).length).toBeGreaterThan(30);
  expect(
    Object.entries(files)
      .filter(([, source]) => !source.includes('defineMessages('))
      .map(([path]) => path),
  ).toEqual([]);
});

it.each(locales)('%s includes every catalog', (locale) => {
  expect(Object.keys(localeMessages(locale)).sort()).toEqual(
    [...groups].sort(),
  );
});

it('shows an unknown key instead of failing', () => {
  const messages = defineMessages('workspace');
  expect(messages.text('de', 'settings')).toBe('Einstellungen');
  expect(messages.text('de', 'unexpected' as 'settings')).toBe('unexpected');
});

describe.each(groups)('%s', (group) => {
  const english: Readonly<Record<string, string>> = localeMessages('en')[group];
  const keys = Object.keys(english).sort();

  it.each(locales)('%s has every key and no other', (locale) => {
    expect(Object.keys(localeMessages(locale)[group]).sort()).toEqual(keys);
  });

  it.each(locales)(
    '%s keeps placeholders and plural forms, and every message formats',
    (locale) => {
      const catalog: Readonly<Record<string, string>> =
        localeMessages(locale)[group];
      for (const key of keys) {
        const text = catalog[key];
        expect(text, key).toBeTypeOf('string');
        expect(text.trim(), key).not.toBe('');
        expect(placeholders(text), key).toEqual(placeholders(english[key]));
        expect(() => validatePluralForms(locale, text), key).not.toThrow();
        for (const count of [0, 1, 2, 5, 22])
          expect(
            () => formatMessage(locale, text, sample(text, count)),
            key,
          ).not.toThrow();
      }
    },
  );
});
