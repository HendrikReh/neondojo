import { describe, expect, it } from 'vitest';
import {
  formatMessage,
  placeholders,
  requiredPluralCategories,
  validatePluralForms,
  type Locale,
} from './i18n-core';
import { definedMessages, fromRows, rowLocales } from './messages';

// Load every feature catalog, so each one registers itself.
const sources = import.meta.glob<string>('../../**/*-messages.ts', {
  eager: true,
  query: '?raw',
  import: 'default',
});
import.meta.glob('../../**/*-messages.ts', { eager: true });

const locales = Object.keys(requiredPluralCategories) as Locale[];
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

it('defines every feature catalog through defineMessages', () => {
  const files = Object.entries(sources);
  expect(files.length).toBeGreaterThan(30);
  expect(
    files
      .filter(([, source]) => !source.includes('defineMessages('))
      .map(([path]) => path),
  ).toEqual([]);
});

it('builds catalogs from rows in the row order', () => {
  const catalogs = fromRows({ greeting: ['en', 'de', 'es', 'fr', 'pl', 'it'] });
  for (const locale of rowLocales)
    expect(catalogs[locale].greeting).toBe(locale);
});

describe.each([...definedMessages])('%s', (_name, catalogs) => {
  const english = catalogs.en;
  const keys = Object.keys(english).sort();

  it.each(locales)('%s has every key and no other', (locale) => {
    expect(Object.keys(catalogs[locale]).sort()).toEqual(keys);
  });

  it.each(locales)(
    '%s keeps placeholders and plural forms, and every message formats',
    (locale) => {
      for (const key of keys) {
        const text = catalogs[locale][key as keyof typeof english];
        expect(text, key).toBeTypeOf('string');
        expect(text.trim(), key).not.toBe('');
        expect(placeholders(text), key).toEqual(
          placeholders(english[key as keyof typeof english]),
        );
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
