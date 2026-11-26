import archiveThemes from './archive-themes';
import { expect, it } from 'vitest';
import { pointColumns, pointStackGeometry, stackGeometry } from './themes';

it('uses XML column positions for both player directions, the bar and bear-off trays', () => {
  expect(pointColumns.map((p) => p.point)).toEqual([
    13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 12, 11, 10, 9, 8, 7, 6, 5,
    4, 3, 2, 1,
  ]);
  expect(pointColumns[0].column).toBe(1);
  expect(pointColumns[6].column).toBe(8);
  expect(pointColumns[23]).toEqual({ point: 1, column: 13, bottom: true });
  for (const theme of archiveThemes) {
    for (const bottom of [false, true]) {
      const geometry = stackGeometry(
        theme,
        theme.checkers.white[0],
        13,
        15,
        bottom,
      );
      expect(geometry.visible).toBe(5);
      expect(geometry.total).toBe(15);
      expect(geometry.x + geometry.width / 2).toBeCloseTo(
        theme.columns[13] +
          theme.checkerOffsetX +
          theme.checkers.white[0].width / 2,
      );
      const end = geometry.y + (bottom ? -1 : 1) * geometry.step * 4;
      expect(Math.min(geometry.y, end)).toBeGreaterThanOrEqual(0);
      expect(Math.max(geometry.y, end) + geometry.height).toBeLessThan(
        theme.background.height,
      );
    }
  }
});

it('uses the original checker height as stack pitch on points, bars and off trays', () => {
  for (const theme of [undefined, ...archiveThemes]) {
    const height = theme?.background.height ?? 470;
    for (const total of [5, 8]) {
      for (const player of [0, 1]) {
        const count = total * (player === 0 ? 1 : -1);
        for (const point of [
          ...pointColumns.map(({ point }) => point),
          player === 0 ? 25 : 0,
          player === 0 ? -1 : 26,
        ]) {
          const stack = pointStackGeometry(point, count, theme);
          expect(stack.visible).toBe(5);
          expect(stack.total).toBe(total);
          const bottom =
            point === -1 || point === 25 || (point >= 1 && point <= 12);
          const top = stack.y - (bottom ? 4 * stack.step : 0);
          const end = top + stack.height + 4 * stack.step;
          expect(stack.step).toBe(stack.height);
          if (theme) {
            const image = theme.checkers[player === 0 ? 'white' : 'black'][0];
            expect([stack.width, stack.height]).toEqual([
              image.width,
              image.height,
            ]);
          }
          expect(top).toBeGreaterThan(0);
          expect(end).toBeLessThan(height);
          if (bottom) expect(top).toBeGreaterThan(height / 2);
          else expect(end).toBeLessThan(height / 2);
        }
      }
    }
  }
});

it('matches the X22 checker-to-point proportions on built-in boards', () => {
  const first = pointStackGeometry(13, 5);
  const next = pointStackGeometry(14, 1);
  expect(first.width / (next.x - first.x)).toBeCloseTo(84 / 90, 8);
  expect(first.step / first.height).toBe(1);
});
