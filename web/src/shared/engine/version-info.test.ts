import { expect, test } from 'vitest';
import { shortEngineVersion } from './engine-version';

test.each([
  ['gnubg', 'GNU Backgammon 1.08.003 20261108', '1.08.003'],
  ['gnubg', 'GNUbg 1.08', '1.08'],
  [
    'bgsage',
    'NeonGammon 3.0.0; model abcd; threads 4; adapter 1',
    '3.0.0',
  ],
  ['bgsage', 'NeonGammon 0.8.0 (adapter 1)', '0.8.0'],
  ['bgsage', 'NeonGammon 2.1.0-rc1; adapter 1', '2.1.0-rc1'],
] as const)(
  'extracts the release for %s from %s',
  (engine, details, version) => {
    expect(shortEngineVersion(engine, details)).toBe(version);
  },
);

test('leaves an unrecognized version string for the details view', () => {
  expect(
    shortEngineVersion('bgsage', 'Model release: 3.0.0'),
  ).toBeUndefined();
});
