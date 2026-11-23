import { expect, it } from 'vitest';
import { sourceText } from './analysis-source-messages';

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
