import { expect, it } from 'vitest';
import {
  sourceMessageLocales,
  sourceMessageRows,
  sourceText,
} from './analysis-source-messages';
import {
  languages,
  placeholders,
  validatePluralForms,
} from '../../../shared/i18n/i18n-core';

it('keeps one translation with the same placeholders and plural forms per language', () => {
  expect([...sourceMessageLocales].sort()).toEqual(
    languages.map(({ id }) => id).sort(),
  );
  for (const [key, row] of Object.entries(sourceMessageRows)) {
    expect(row, key).toHaveLength(sourceMessageLocales.length);
    const english = placeholders(row[0]);
    sourceMessageLocales.forEach((locale, index) => {
      const text = row[index] ?? '';
      expect(text, `${locale}.${key}`).toBeTruthy();
      expect(placeholders(text), `${locale}.${key}`).toEqual(english);
      expect(() => validatePluralForms(locale, text)).not.toThrow();
    });
  }
});

it.each([
  [1, '1 kontekst wyniku'],
  [3, '3 konteksty wyniku'],
  [5, '5 kontekstów wyniku'],
  [1000, '1000 kontekstów wyniku'],
])('counts %s Polish score contexts', (count, expected) =>
  expect(sourceText('pl', 'contextCount', { count })).toBe(expected),
);

it('writes counts in the language number format', () => {
  expect(sourceText('en', 'rolloutGames', { count: 1296 })).toBe(
    'Rollout · 1,296 games',
  );
  expect(sourceText('de', 'rolloutGames', { count: 1296 })).toBe(
    'Rollout · 1.296 Spiele',
  );
});
