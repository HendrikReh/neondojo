import { expect, it } from 'vitest';
import {
  LIBRARY_COLUMNS,
  COLUMN_WIDTHS_KEY,
  readColumnWidths,
  saveColumnWidths,
  resizeColumn,
  totalColumnWidth,
  type ColumnWidths,
} from './library-columns';

const widths = Object.fromEntries(
  LIBRARY_COLUMNS.map((column) => [column.id, Math.max(120, column.min)]),
) as ColumnWidths;
it('restores a complete saved layout and resets its storage', () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
    removeItem: (key: string) => {
      values.delete(key);
    },
  };
  saveColumnWidths(widths, storage);
  expect(values.has(COLUMN_WIDTHS_KEY)).toBe(true);
  expect(readColumnWidths(storage)).toEqual(widths);
  saveColumnWidths(undefined, storage);
  expect(readColumnWidths(storage)).toBeUndefined();
});
it('ignores corrupt, partial, unsupported and unsafe persisted profiles', () => {
  for (const raw of [
    'oops',
    'null',
    JSON.stringify({ version: 2, widths }),
    JSON.stringify({ version: 1, widths: { white: 150 } }),
    ...[null, -1, 0, 100000].map((white) =>
      JSON.stringify({ version: 1, widths: { ...widths, white } }),
    ),
  ])
    expect(readColumnWidths({ getItem: () => raw })).toBeUndefined();
  expect(
    readColumnWidths({
      getItem: () => {
        throw Error('blocked');
      },
    }),
  ).toBeUndefined();
  expect(() =>
    saveColumnWidths(widths, {
      setItem: () => {
        throw Error('blocked');
      },
      removeItem: () => {
        throw Error('blocked');
      },
    }),
  ).not.toThrow();
});
it('changes only the requested column without mutating the captured layout', () => {
  const resized = resizeColumn(widths, 'white', widths.white + 83.4);
  expect(resized.white).toBe(widths.white + 83);
  expect(widths.white).toBe(120);
  expect({ ...resized, white: widths.white }).toEqual(widths);
  expect(totalColumnWidth(resized) - totalColumnWidth(widths)).toBe(83);
});
it('keeps checkbox, player and action columns usable and bounds oversized input', () => {
  for (const column of LIBRARY_COLUMNS) {
    expect(resizeColumn(widths, column.id, -999)[column.id]).toBe(column.min);
    expect(resizeColumn(widths, column.id, 999999)[column.id]).toBe(2400);
  }
  expect(resizeColumn(widths, 'white', Number.NaN)).toEqual(widths);
});

it('migrates the old notes width to tags without resetting other custom widths', () => {
  const legacy = {
    ...Object.fromEntries(
      Object.entries(widths).filter(
        ([id]) => id !== 'tags' && id !== 'comment',
      ),
    ),
    notes: 199,
    white: 211,
  };
  const restored = readColumnWidths({
    getItem: () => JSON.stringify({ version: 1, widths: legacy }),
  }) as Record<string, number>;
  expect(restored.white).toBe(211);
  expect(restored.tags).toBe(199);
  expect(restored.comment).toBe(220);
});

it('adds the mode width to old profiles without resetting customized player widths', () => {
  const legacy = Object.fromEntries(
    Object.entries(widths).filter(([key]) => key !== 'play_mode'),
  );
  legacy.white = 211;
  const restored = readColumnWidths({
    getItem: () => JSON.stringify({ version: 1, widths: legacy }),
  }) as Record<string, number>;
  expect(restored.white).toBe(211);
  expect(restored.play_mode).toBe(210);
});

it('widens saved action controls without resetting other customized columns', () => {
  const restored = readColumnWidths({
    getItem: () =>
      JSON.stringify({
        version: 1,
        widths: { ...widths, actions: 88, white: 211 },
      }),
  });
  expect(restored).toEqual({ ...widths, actions: 160, white: 211 });
  expect(resizeColumn(widths, 'actions', 88).actions).toBe(160);
});

it('restores search widths alongside legacy match widths without requiring new keys in old profiles', async () => {
  const { LIBRARY_RESIZABLE_LAYOUT } = await import('./library-columns');
  const legacy = JSON.stringify({
    version: 1,
    widths: { ...widths, white: 211 },
  });
  const restored = LIBRARY_RESIZABLE_LAYOUT.read({ getItem: () => legacy });
  expect(restored).toEqual({
    ...widths,
    white: 211,
    occurrence_database: 160,
    occurrence_game: 70,
    occurrence_move: 70,
  });
  const customized = {
    ...restored!,
    occurrence_database: 240,
    occurrence_game: 90,
    occurrence_move: 100,
  };
  let stored = '';
  LIBRARY_RESIZABLE_LAYOUT.save(customized, {
    setItem: (_key, value) => {
      stored = value;
    },
    removeItem: () => {},
  });
  expect(LIBRARY_RESIZABLE_LAYOUT.read({ getItem: () => stored })).toEqual(
    customized,
  );
  expect(
    LIBRARY_RESIZABLE_LAYOUT.total(customized, [
      'occurrence_database',
      'occurrence_game',
      'occurrence_move',
    ]),
  ).toBe(430);
  for (const occurrence_database of [0, 2401, 99.5])
    expect(
      LIBRARY_RESIZABLE_LAYOUT.read({
        getItem: () =>
          JSON.stringify({
            version: 1,
            widths: { ...customized, occurrence_database },
          }),
      }),
    ).toBeUndefined();
});
