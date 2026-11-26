import archiveThemes from './archive-themes';
import { expect, it, vi } from 'vitest';
import { loadArchiveThemes, pointColumns, stackGeometry } from './themes';

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
      expect(geometry.x).toBe(theme.columns[13] + theme.checkerOffsetX);
      const end = geometry.y + (bottom ? -1 : 1) * geometry.step * 4;
      expect(Math.min(geometry.y, end)).toBeGreaterThanOrEqual(0);
      expect(
        Math.max(geometry.y, end) + theme.checkers.white[0].height,
      ).toBeLessThan(theme.background.height);
    }
  }
});

it('shares one archive request and retries after a transient chunk failure', async () => {
  vi.doMock('./archive-themes', () => {
    throw new TypeError('Failed to fetch dynamically imported module');
  });
  const failed = loadArchiveThemes();
  expect(loadArchiveThemes()).toBe(failed);
  try {
    await expect(failed).rejects.toThrow();
  } finally {
    vi.doUnmock('./archive-themes');
  }
  const retry = loadArchiveThemes();
  expect(retry).not.toBe(failed);
  await expect(retry).resolves.toEqual(archiveThemes);
  expect(loadArchiveThemes()).toBe(retry);
});
