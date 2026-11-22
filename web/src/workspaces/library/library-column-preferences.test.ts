import { expect, test } from 'vitest';
import {
  LIBRARY_DATA_COLUMNS,
  DEFAULT_LIBRARY_COLUMNS,
  VISIBLE_COLUMNS_KEY,
  readVisibleColumns,
  saveVisibleColumns,
  toggleVisibleColumn,
} from './library-column-preferences';

test('the default keeps tags visible while tournament and comment are opt-in', () => {
  expect(DEFAULT_LIBRARY_COLUMNS).toContain('tags');
  expect(DEFAULT_LIBRARY_COLUMNS).not.toContain('tournament');
  expect(DEFAULT_LIBRARY_COLUMNS).not.toContain('comment');
  expect(readVisibleColumns({ getItem: () => null })).toEqual(
    DEFAULT_LIBRARY_COLUMNS,
  );
});
test('visibility restores canonical data-column order and ignores future and structural keys', () => {
  const storage = {
    getItem: () =>
      JSON.stringify({
        version: 1,
        columns: ['comment', 'white', 'select', 'future', 'white'],
      }),
  };
  expect(readVisibleColumns(storage)).toEqual([
    'white',
    'play_mode',
    'comment',
  ]);
  for (const raw of [
    'bad',
    'null',
    JSON.stringify({ version: 3, columns: ['id'] }),
    JSON.stringify({ version: 1, columns: [] }),
    JSON.stringify({ version: 1, columns: [42] }),
  ])
    expect(readVisibleColumns({ getItem: () => raw })).toEqual(
      DEFAULT_LIBRARY_COLUMNS,
    );
  expect(
    readVisibleColumns({
      getItem: () => {
        throw Error('blocked');
      },
    }),
  ).toEqual(DEFAULT_LIBRARY_COLUMNS);
});
test('the last data column stays visible and toggles never reorder columns', () => {
  expect(toggleVisibleColumn(['id'], 'id', false)).toEqual(['id']);
  expect(toggleVisibleColumn(['id'], 'comment', true)).toEqual([
    'id',
    'comment',
  ]);
  expect(toggleVisibleColumn(['comment'], 'white', true)).toEqual([
    'white',
    'comment',
  ]);
  expect(
    toggleVisibleColumn(LIBRARY_DATA_COLUMNS, 'tournament', false),
  ).not.toContain('tournament');
});
test('visibility can be saved and reset independently of width profiles', () => {
  const values = new Map<string, string>([
    ['neondojo.library.columns', 'existing widths'],
  ]);
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    },
  };
  saveVisibleColumns(['id', 'comment'], storage);
  expect(readVisibleColumns(storage)).toEqual(['id', 'comment']);
  expect(values.get('neondojo.library.columns')).toBe('existing widths');
  saveVisibleColumns(undefined, storage);
  expect(values.has(VISIBLE_COLUMNS_KEY)).toBe(false);
  expect(readVisibleColumns(storage)).toEqual(DEFAULT_LIBRARY_COLUMNS);
  expect(() =>
    saveVisibleColumns(['id'], {
      setItem: () => {
        throw Error('blocked');
      },
      removeItem: () => {
        throw Error('blocked');
      },
    }),
  ).not.toThrow();
});

test('shows a separate play-mode column by default and upgrades older saved visibility', () => {
  expect(DEFAULT_LIBRARY_COLUMNS).toContain('play_mode');
  expect(
    readVisibleColumns({
      getItem: () =>
        JSON.stringify({ version: 1, columns: ['id', 'white', 'comment'] }),
    }),
  ).toEqual(['id', 'white', 'play_mode', 'comment']);
});
test('new visibility profiles preserve an explicitly hidden play-mode column', () => {
  let raw = '';
  saveVisibleColumns(['id', 'white'], {
    setItem: (_key, value) => {
      raw = value;
    },
    removeItem: () => {},
  });
  expect(JSON.parse(raw).version).toBe(2);
  expect(readVisibleColumns({ getItem: () => raw })).toEqual(['id', 'white']);
});
