import { expect, it } from 'vitest';
import { captureColumnWidths, createColumnLayout } from './column-widths';

const columns = [
  { id: 'position', className: 'position-col', min: 80 },
  { id: 'decision', className: 'decision-col', min: 60, defaultWidth: 140 },
  { id: 'played_move', className: 'played-col', min: 50, defaultWidth: 100 },
] as const;
const widths = { position: 180, decision: 160, played_move: 120 };
const activeColumns = ['position', 'decision'] as const;

function memoryStorage() {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  };
}

it('keeps complete versioned layouts in their own storage namespaces', () => {
  const storage = memoryStorage();
  const positions = createColumnLayout(columns, 'positions.columns');
  const matches = createColumnLayout(columns, 'matches.columns');
  positions.save(widths, storage);
  matches.save({ ...widths, position: 220 }, storage);
  expect(positions.read(storage)).toEqual(widths);
  expect(matches.read(storage)).toEqual({ ...widths, position: 220 });
  expect(JSON.parse(storage.values.get('positions.columns')!)).toEqual({
    version: 1,
    widths,
  });
  positions.save(undefined, storage);
  expect(positions.read(storage)).toBeUndefined();
  expect(matches.read(storage)?.position).toBe(220);
});

it('rejects incomplete, invalid and unsupported saved layouts', () => {
  const layout = createColumnLayout(columns, 'positions.columns');
  for (const raw of [
    'invalid',
    'null',
    JSON.stringify({ version: 2, widths }),
    JSON.stringify({ version: 1, widths: { position: 180, decision: 160 } }),
    ...[null, -1, 79, 180.5, 2401].map((position) =>
      JSON.stringify({ version: 1, widths: { ...widths, position } }),
    ),
  ]) {
    expect(layout.read({ getItem: () => raw })).toBeUndefined();
  }
});

it('keeps legacy widths when a new column supplies an explicit migration default', () => {
  const expanded = [
    ...columns,
    { id: 'decks', className: 'decks-col', min: 80 },
  ] as const;
  const layout = createColumnLayout(expanded, 'positions.columns', {
    decks: 144,
  });
  const stored = (values: object) => ({
    getItem: () => JSON.stringify({ version: 1, widths: values }),
  });
  expect(layout.read(stored(widths))).toEqual({ ...widths, decks: 144 });
  expect(layout.read(stored({ ...widths, decks: 215 }))).toEqual({
    ...widths,
    decks: 215,
  });
  expect(layout.read(stored({ ...widths, decks: 0 }))).toBeUndefined();
  expect(layout.read(stored({ position: 180, decks: 144 }))).toBeUndefined();
});

it('totals the active columns while retaining widths for optional columns', () => {
  const layout = createColumnLayout(columns, 'positions.columns');
  expect(layout.total(widths)).toBe(460);
  expect(layout.total(widths, activeColumns)).toBe(340);
  const resized = layout.resize(widths, 'decision', 199.5);
  expect(resized).toEqual({ ...widths, decision: 200 });
  expect(widths.decision).toBe(160);
  expect(layout.total(resized, activeColumns)).toBe(380);
  expect(layout.resize(widths, 'position', -1).position).toBe(80);
  expect(layout.resize(widths, 'position', 99999).position).toBe(2400);
  expect(layout.resize(widths, 'position', Number.NaN)).toBe(widths);
});

it('captures active widths and retains saved widths for missing optional columns', () => {
  const layout = createColumnLayout(columns, 'positions.columns');
  const captured = captureColumnWidths(
    layout,
    (id) => (id === 'played_move' ? undefined : 210.4),
    { widths, activeColumns },
  );
  expect(captured).toEqual({ position: 210, decision: 210, played_move: 120 });
});

it('uses default or minimum widths for inactive columns without saved widths', () => {
  const layout = createColumnLayout(columns, 'positions.columns');
  expect(
    captureColumnWidths(layout, () => 170, {
      activeColumns: ['position'],
    }),
  ).toEqual({ position: 170, decision: 140, played_move: 100 });
  expect(
    captureColumnWidths(layout, () => 170, {
      activeColumns: ['decision'],
    }),
  ).toEqual({ position: 80, decision: 170, played_move: 100 });
});

it('does not capture a layout when an active header is missing or hidden', () => {
  const layout = createColumnLayout(columns, 'positions.columns');
  for (const missingWidth of [undefined, 0, -1, Number.NaN]) {
    expect(
      captureColumnWidths(
        layout,
        (id) => (id === 'decision' ? missingWidth : 170),
        { widths, activeColumns },
      ),
    ).toBeUndefined();
  }
});

it('bounds every captured width and fallback so saved defaults remain reloadable', () => {
  const layout = createColumnLayout(
    [
      ...columns,
      { id: 'notes', className: 'notes-col', min: 96, defaultWidth: 9000 },
    ] as const,
    'positions.columns',
  );
  const captured = captureColumnWidths(layout, () => 5000, {
    activeColumns,
  });
  expect(captured).toEqual({
    position: 2400,
    decision: 2400,
    played_move: 100,
    notes: 2400,
  });
  const storage = memoryStorage();
  layout.save(captured, storage);
  expect(layout.read(storage)).toEqual(captured);
});
