import { expect, it } from 'vitest';
import { sourceText } from './analysis-source-messages';

it.each([
  [1, '1 Spielstandkontext'],
  [3, '3 Spielstandkontexte'],
  [5, '5 Spielstandkontexte'],
  [1000, '1.000 Spielstandkontexte'],
])('counts %s German score contexts', (count, expected) =>
  expect(sourceText('de', 'contextCount', { count })).toBe(expected),
);

it('writes counts in the language number format', () => {
  expect(sourceText('en', 'rolloutGames', { count: 1296 })).toBe(
    'Rollout · 1,296 games',
  );
  expect(sourceText('de', 'rolloutGames', { count: 1296 })).toBe(
    'Rollout · 1.296 Spiele',
  );
});
