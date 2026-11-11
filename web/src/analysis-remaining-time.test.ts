import { afterEach, expect, it, vi } from 'vitest';
import {
  remainingTimeAt,
  loadRemainingTime,
  type RemainingTime,
} from './analysis-remaining-time-api';
const sample: RemainingTime = {
  job_id: 'job',
  status: 'running',
  state: 'estimated',
  basis: 'benchmark',
  remaining_ms: 5000,
  total_moves: 100,
  completed_moves: 20,
  timed_moves: 20,
  measured_elapsed_ms: 1000,
  current_game_elapsed_ms: 100,
  sampled_at: '2026-11-10T12:00:00Z',
  attempt: 1,
};
afterEach(() => vi.unstubAllGlobals());
it('subtracts only monotonic time since receipt, independently of wall clock', () => {
  vi.spyOn(Date, 'now').mockReturnValue(99_000_000);
  expect(remainingTimeAt(sample, 100, 1600)).toMatchObject({
    state: 'estimated',
    remaining_ms: 3500,
  });
  vi.restoreAllMocks();
});
it('marks an overrun for recalculation instead of zero or negative seconds', () => {
  expect(remainingTimeAt(sample, 100, 5100)).toMatchObject({
    state: 'reestimating',
    remaining_ms: null,
  });
  expect(remainingTimeAt(sample, 100, 6100)).toMatchObject({
    state: 'reestimating',
    remaining_ms: null,
  });
});
it.each([
  'queued',
  'waiting',
  'unavailable',
  'reestimating',
  'finishing',
  'completed',
  'failed',
] as const)('does not count down %s states', (state) => {
  const timing = { ...sample, state, remaining_ms: null };
  expect(remainingTimeAt(timing, 100, 99_000)).toMatchObject({
    state,
    remaining_ms: null,
  });
});
it('does not count a negative clock delta as extra remaining time', () => {
  expect(remainingTimeAt(sample, 100, 90).remaining_ms).toBe(5000);
});
it('reads the persisted duration snapshot without starting any work', async () => {
  const fetch = vi.fn(
    async () => new Response(JSON.stringify(sample), { status: 200 }),
  );
  vi.stubGlobal('fetch', fetch);
  expect(await loadRemainingTime('job', new AbortController().signal)).toEqual(
    sample,
  );
  expect(fetch).toHaveBeenCalledWith(
    '/api/jobs/job/remaining-time',
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
});
it.each([
  { ...sample, state: 'unknown' },
  { ...sample, basis: 'invented' },
  { ...sample, attempt: -1 },
  { ...sample, sampled_at: 'not-a-date' },
  { ...sample, completed_moves: 101 },
  { ...sample, timed_moves: 21, completed_moves: 20 },
  { ...sample, remaining_ms: 0 },
  { ...sample, remaining_ms: -1 },
  { ...sample, measured_elapsed_ms: -1 },
  { ...sample, current_game_elapsed_ms: -1 },
])(
  'rejects malformed optional timing before it reaches the renderer',
  async (timing) => {
    vi.stubGlobal(
      'fetch',
      async () => new Response(JSON.stringify(timing), { status: 200 }),
    );
    await expect(
      loadRemainingTime('job', new AbortController().signal),
    ).rejects.toThrow();
  },
);
it('keeps a waiting snapshot frozen until actual engine work starts', async () => {
  const waiting = { ...sample, state: 'waiting' as const, remaining_ms: 5000 };
  vi.stubGlobal(
    'fetch',
    async () => new Response(JSON.stringify(waiting), { status: 200 }),
  );
  const value = await loadRemainingTime('job', new AbortController().signal);
  expect(remainingTimeAt(value, 100, 50_000).remaining_ms).toBe(5000);
});
