import { expect, it } from 'vitest';
import { readTheme, saveTheme } from './ui-preferences';

it('preserves a known theme and falls back safely for missing or invalid values', () => {
  expect(readTheme({ getItem: () => 'walnut' })).toBe('walnut');
  expect(readTheme({ getItem: () => null })).toBe('forest');
  expect(readTheme({ getItem: () => 'removed-theme' })).toBe('forest');
  expect(
    readTheme({
      getItem: () => {
        throw new Error('blocked');
      },
    }),
  ).toBe('forest');
});

it('can save preferences even when browser storage is unavailable', () => {
  const values = new Map<string, string>();
  saveTheme('slate', { setItem: (key, value) => values.set(key, value) });
  expect(values.get('bgtutor.theme')).toBe('slate');
  expect(() =>
    saveTheme('forest', {
      setItem: () => {
        throw new Error('blocked');
      },
    }),
  ).not.toThrow();
});
