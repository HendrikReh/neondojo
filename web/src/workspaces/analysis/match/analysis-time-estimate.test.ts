import { expect, it } from 'vitest';
import {
  formatAnalysisDuration,
  analysisEstimateText,
} from './analysis-time-estimate-messages';
it.each([
  [0.1, '<1s'],
  [999, '<1s'],
  [1000, '1s'],
  [1499, '1s'],
  [59_600, '1m'],
  [61_000, '1m 1s'],
  [3_599_600, '1h'],
  [3_660_000, '1h 1m'],
  [90_000_000, '25h'],
])('formats %s ms without zero units', (ms, expected) =>
  expect(formatAnalysisDuration(Number(ms))).toBe(expected),
);
it.each([0, -1, NaN, Infinity])('does not fabricate duration for %s', (ms) =>
  expect(formatAnalysisDuration(ms)).toBeNull(),
);
it('localizes estimates and falls back to English', () => {
  expect(analysisEstimateText('de', 'estimate', { duration: '40s' })).toBe(
    'Geschätzte Analysezeit: ca. 40s',
  );
  expect(analysisEstimateText('en', 'estimate', { duration: '40s' })).toBe(
    'Estimated analysis time: approx. 40s',
  );
  expect(analysisEstimateText('fr', 'unavailable')).toBe(
    'Estimate unavailable',
  );
});

import { afterEach, vi } from 'vitest';
import { loadAnalysisTimeEstimate } from './analysis-time-estimate-api';
const validEstimate = {
  match_id: 'match',
  game_no: 1,
  engine_id: 'gnubg',
  engine_version: '1.08',
  plies: 2,
  position_count: 2,
  estimated_ms: 80,
  unavailable_reason: null,
  benchmark: {
    id: 'benchmark',
    finished_at: '2026-11-10T12:00:00Z',
    reference_name: 'WC2014',
    average_move_ms: 40,
    protocol_id: 'benchmark-v2',
  },
};
afterEach(() => vi.unstubAllGlobals());
function loadFixture(data: unknown) {
  vi.stubGlobal(
    'fetch',
    async () => new Response(JSON.stringify(data), { status: 200 }),
  );
  return loadAnalysisTimeEstimate(
    'match',
    2,
    'gnubg',
    1,
    new AbortController().signal,
  );
}
it('accepts a completed benchmark estimate with its timing basis', async () => {
  await expect(loadFixture(validEstimate)).resolves.toEqual(validEstimate);
});
it.each([
  [
    'invalid date',
    {
      ...validEstimate,
      benchmark: { ...validEstimate.benchmark, finished_at: 'not-a-date' },
    },
  ],
  [
    'unknown reason',
    {
      ...validEstimate,
      estimated_ms: null,
      benchmark: null,
      unavailable_reason: 'unknown',
    },
  ],
  ['negative timing', { ...validEstimate, estimated_ms: -1 }],
  ['zero timing', { ...validEstimate, estimated_ms: 0 }],
  ['non-finite timing', { ...validEstimate, estimated_ms: Infinity }],
  ['fractional count', { ...validEstimate, position_count: 1.5 }],
  ['negative count', { ...validEstimate, position_count: -1 }],
  ['missing basis', { ...validEstimate, benchmark: null }],
  [
    'failed result with timing',
    { ...validEstimate, unavailable_reason: 'no_matching_benchmark' },
  ],
  [
    'invalid average',
    {
      ...validEstimate,
      benchmark: { ...validEstimate.benchmark, average_move_ms: 0 },
    },
  ],
])('rejects %s from the optional estimate endpoint', async (_name, data) => {
  await expect(loadFixture(data)).rejects.toThrow();
});
it.each(['no_matching_benchmark', 'engine_unavailable', 'no_checker_moves'])(
  'accepts unavailable reason %s with no timing basis',
  async (reason) => {
    const data = {
      ...validEstimate,
      estimated_ms: null,
      benchmark: null,
      unavailable_reason: reason,
      position_count: reason === 'no_checker_moves' ? 0 : 2,
    };
    await expect(loadFixture(data)).resolves.toEqual(data);
  },
);
