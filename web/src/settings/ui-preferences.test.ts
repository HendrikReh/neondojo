import { expect, it } from 'vitest';
import {
  archiveThemesPreloadKey,
  readArchiveThemesPreloadHint,
  readTheme,
  saveArchiveThemesPreloadHint,
  saveTheme,
  usesArchiveTheme,
} from './ui-preferences';

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
  expect(values.get('neondojo.theme')).toBe('slate');
  expect(() =>
    saveTheme('forest', {
      setItem: () => {
        throw new Error('blocked');
      },
    }),
  ).not.toThrow();
});

it('persists and reads a mixed-BGBlitz archive-theme preload hint', () => {
  expect(usesArchiveTheme({ theme: 'forest' })).toBe(false);
  expect(usesArchiveTheme({ theme: 'bgblitz:x22' })).toBe(true);
  expect(
    usesArchiveTheme({ theme: 'forest', checkerTheme: 'bgblitz:x22' }),
  ).toBe(true);
  expect(usesArchiveTheme({ theme: 'forest', cubeTheme: 'bgblitz:x22' })).toBe(
    true,
  );
  expect(usesArchiveTheme({ theme: 'forest', diceTheme: 'bgblitz:x22' })).toBe(
    true,
  );

  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => {
      values.delete(key);
    },
  };
  saveTheme('forest', storage);
  expect(readArchiveThemesPreloadHint(storage)).toBe(false);

  saveArchiveThemesPreloadHint(true, storage);
  expect(values.get(archiveThemesPreloadKey)).toBe('1');
  expect(readArchiveThemesPreloadHint(storage)).toBe(true);

  saveArchiveThemesPreloadHint(false, storage);
  expect(values.has(archiveThemesPreloadKey)).toBe(false);
  expect(readArchiveThemesPreloadHint(storage)).toBe(false);

  expect(() =>
    saveArchiveThemesPreloadHint(true, {
      setItem: () => {
        throw new Error('blocked');
      },
      removeItem: () => {
        throw new Error('blocked');
      },
    }),
  ).not.toThrow();
});
