import { expect, it } from 'vitest';
import type { BenchmarkRun } from './benchmark-api';
import {
  DEFAULT_BENCHMARK_HISTORY_ORDER,
  nextBenchmarkHistoryOrder,
  sortBenchmarkRuns,
  type BenchmarkHistoryOrder,
} from './benchmark-history-sort';

const base: BenchmarkRun = {
  id: 'a',
  status: 'completed',
  created_at: '2026-11-12T10:00:00Z',
  finished_at: '2026-11-12T10:05:00Z',
  started_at: null,
  machine: { id: 'm', name: 'Mac', os: 'macOS', architecture: 'aarch64' },
  engine_id: 'gnubg',
  engine_name: 'GNUbg',
  engine_version: 'GNUbg 1.08',
  engine_path: '/usr/bin/gnubg',
  plies: 2,
  reference: null,
  protocol: null,
  play_reference: null,
  total_elapsed_ms: 4600,
  average_move_ms: 10,
  average_turn_ms: null,
  completed_games: 10,
  analyzed_moves: 460,
  completed_turns: 0,
  error: null,
};
// Server order: newest first.
const runs: BenchmarkRun[] = [
  {
    ...base,
    id: 'running',
    status: 'running',
    created_at: '2026-11-15T10:00:00Z',
    engine_name: 'NeonGammon',
    plies: 3,
    total_elapsed_ms: null,
    average_move_ms: null,
  },
  {
    ...base,
    id: 'slow',
    created_at: '2026-11-14T10:00:00Z',
    plies: 4,
    total_elapsed_ms: 9200,
    average_move_ms: 20,
  },
  {
    ...base,
    id: 'failed',
    status: 'failed',
    created_at: '2026-11-13T10:00:00Z',
    plies: 1,
    total_elapsed_ms: null,
    average_move_ms: null,
  },
  {
    ...base,
    id: 'fast',
    created_at: '2026-11-12T10:00:00Z',
    engine_name: 'neongammon',
    plies: 1,
    total_elapsed_ms: 460,
    average_move_ms: 1,
  },
  {
    ...base,
    id: 'cancelled',
    status: 'cancelled',
    created_at: '2026-11-11T10:00:00Z',
    plies: 2,
    total_elapsed_ms: null,
    average_move_ms: null,
  },
  {
    ...base,
    id: 'queued',
    status: 'queued',
    created_at: '2026-11-10T10:00:00Z',
    plies: 5,
    total_elapsed_ms: null,
    average_move_ms: null,
  },
];
const ids = (order: BenchmarkHistoryOrder, play = false, list = runs) =>
  sortBenchmarkRuns(list, order, play, 'en').map((run) => run.id);

it('keeps the server order by default and reverses dates', () => {
  expect(ids(DEFAULT_BENCHMARK_HISTORY_ORDER)).toEqual(runs.map((r) => r.id));
  expect(ids({ sort: 'date', direction: 'asc' })).toEqual(
    runs.map((r) => r.id).reverse(),
  );
});

it('sorts engines case-insensitively and keeps newest first within a name', () => {
  expect(ids({ sort: 'engine', direction: 'asc' })).toEqual([
    'slow',
    'failed',
    'cancelled',
    'queued',
    'running',
    'fast',
  ]);
});

it('sorts plies numerically with stable ties', () => {
  expect(ids({ sort: 'plies', direction: 'asc' })).toEqual([
    'failed',
    'fast',
    'cancelled',
    'running',
    'slow',
    'queued',
  ]);
  expect(ids({ sort: 'plies', direction: 'desc' })).toEqual([
    'queued',
    'slow',
    'running',
    'cancelled',
    'failed',
    'fast',
  ]);
});

it('puts runs without a measured time last in both directions', () => {
  const unmeasured = ['running', 'failed', 'cancelled', 'queued'];
  for (const sort of ['total', 'average'] as const) {
    expect(ids({ sort, direction: 'asc' })).toEqual([
      'fast',
      'slow',
      ...unmeasured,
    ]);
    expect(ids({ sort, direction: 'desc' })).toEqual([
      'slow',
      'fast',
      ...unmeasured,
    ]);
  }
});

it('ignores times stored on runs that did not complete', () => {
  const stale = { ...runs[2], average_move_ms: 0.5 };
  expect(
    ids({ sort: 'average', direction: 'asc' }, false, [stale, runs[3]]),
  ).toEqual(['fast', 'failed']);
});

it('sorts status by lifecycle, not by localized label', () => {
  expect(ids({ sort: 'status', direction: 'asc' })).toEqual([
    'running',
    'queued',
    'slow',
    'fast',
    'failed',
    'cancelled',
  ]);
  expect(ids({ sort: 'status', direction: 'desc' })).toEqual([
    'cancelled',
    'failed',
    'slow',
    'fast',
    'queued',
    'running',
  ]);
});

it('sorts play latency by average turn time and ignores the analysis total', () => {
  const play = runs.slice(1, 4).map((run, index) => ({
    ...run,
    average_move_ms: null,
    average_turn_ms:
      run.status === 'completed' ? [0, 40, 0, 12][index + 1] : null,
  }));
  expect(ids({ sort: 'average', direction: 'asc' }, true, play)).toEqual([
    'fast',
    'slow',
    'failed',
  ]);
  expect(ids({ sort: 'total', direction: 'asc' }, true, play)).toEqual(
    play.map((run) => run.id),
  );
});

it('toggles the active column and starts each new column in its natural direction', () => {
  const date = DEFAULT_BENCHMARK_HISTORY_ORDER;
  expect(nextBenchmarkHistoryOrder(date, 'date')).toEqual({
    sort: 'date',
    direction: 'asc',
  });
  for (const sort of ['engine', 'plies', 'total', 'average', 'status'] as const)
    expect(nextBenchmarkHistoryOrder(date, sort)).toEqual({
      sort,
      direction: 'asc',
    });
  expect(
    nextBenchmarkHistoryOrder({ sort: 'plies', direction: 'asc' }, 'date'),
  ).toEqual(date);
});
