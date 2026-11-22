import { expect, it } from 'vitest';
import { rollingMean, rollingRatings } from './PlayerRatingTrend';

const point = (value: number, decisions: number) => ({
  match_id: `${value}-${decisions}`,
  date: '2026-11-01',
  profile: 'gnubg-2ply',
  value,
  decisions,
  checker_value: null,
  cube_value: null,
});

it('weights the rolling rating by rated decisions within the window', () => {
  const points = [point(10, 100), point(4, 300), point(8, 100)];
  expect(rollingRatings(points, 2)).toEqual([10, 5.5, 5]);
  // A match without rated decisions falls back to the plain mean.
  expect(rollingRatings([point(6, 0), point(2, 0)], 2)).toEqual([6, 4]);
});

it('averages only the split ratings that exist', () => {
  expect(rollingMean([null, 6, null, 2], 2)).toEqual([null, 6, 6, 2]);
  expect(rollingMean([4, 8, 6], 2)).toEqual([4, 6, 7]);
});
