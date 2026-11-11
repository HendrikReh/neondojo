import { expect, it } from 'vitest';
import {
  POSITION_COLUMNS,
  POSITION_COLUMN_LAYOUT,
  type PositionColumn,
} from './position-table-columns';

it('retains saved position widths while giving the remove action a complete click target', () => {
  const widths = Object.fromEntries(
    POSITION_COLUMNS.map((column) => [column.id, Math.max(120, column.min)]),
  ) as Record<PositionColumn, number>;
  const restored = POSITION_COLUMN_LAYOUT.read({
    getItem: () =>
      JSON.stringify({
        version: 1,
        widths: { ...widths, actions: 40, origin: 211 },
      }),
  });
  expect(restored).toEqual({ ...widths, actions: 60, origin: 211 });
  expect(POSITION_COLUMN_LAYOUT.resize(widths, 'actions', 40).actions).toBe(60);
});
