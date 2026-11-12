import { expect, it } from 'vitest';
import { benchmarkComparisons } from './benchmark-comparison';
import type { BenchmarkRun } from './benchmark-api';
// Values irrelevant to grouping are copied from the API-shaped baseline.
const base: BenchmarkRun = {
  id: 'a',
  status: 'completed',
  created_at: '2026-11-12',
  finished_at: '2026-11-12',
  started_at: null,
  machine: { id: 'm', name: 'Mac', os: 'macOS', architecture: 'aarch64' },
  engine_id: 'bgsage',
  engine_name: 'Open Sage',
  engine_version: 'model-a;threads4',
  engine_path: '/local/python',
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
it('uses three complete matching runs and never mixes providers, models or workload', () => {
  const values = [10, 30, 20].map((value, i) => ({
    ...base,
    id: String(i),
    average_move_ms: value,
  }));
  const result = benchmarkComparisons([
    ...values,
    { ...base, engine_id: 'gnubg', average_move_ms: 999 },
    { ...base, engine_version: 'model-b', average_move_ms: 999 },
    { ...base, status: 'failed', average_move_ms: 999 },
  ]);
  expect(result).toHaveLength(1);
  expect(result[0]).toMatchObject({
    median: 20,
    minimum: 10,
    maximum: 30,
    count: 3,
  });
  expect(benchmarkComparisons(values.slice(0, 2))).toEqual([]);
});
it('separates play measurements after a GNUbg rules upgrade', () => {
  const snapshot = {
    engine: {
      engine_id: 'bgsage',
      gnubg_path: '/rules/gnubg',
      engine_version: 'sage-model',
    },
    rules_version: 'GNU Backgammon old',
  };
  const values = [10, 20, 30].map((value, i) => ({
    ...base,
    id: String(i),
    average_move_ms: value,
    engine_snapshot: snapshot,
  }));
  expect(benchmarkComparisons(values)).toHaveLength(1);
  values[2] = {
    ...values[2],
    engine_snapshot: { ...snapshot, rules_version: 'GNU Backgammon new' },
  };
  expect(benchmarkComparisons(values)).toEqual([]);
});
