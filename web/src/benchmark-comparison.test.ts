import { expect, it } from 'vitest';
import {
  benchmarkComparisons,
  benchmarkThreadCount,
} from './benchmark-comparison';
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
it('separates play measurements by the recorded GNUbg rules and protocol', () => {
  const snapshot = {
    engine: {
      engine_id: 'bgsage',
      gnubg_path: '/rules/gnubg',
      engine_version: 'sage-model',
    },
    rules_version: 'GNU Backgammon 1.07',
  };
  const reference = {
    id: 'wc2014-checker-turns-v1',
    protocol_id: 'bgsage-checker-turn-v3-2ply-2threads-gnubg-rules',
    sha256: 'abc',
    total_turns: 11,
  };
  const firstGroup: BenchmarkRun[] = [10, 20, 30].map((value, i) => ({
    ...base,
    id: `original-${i}`,
    average_move_ms: null,
    average_turn_ms: value,
    total_elapsed_ms: value * reference.total_turns,
    completed_games: 0,
    analyzed_moves: 0,
    completed_turns: reference.total_turns,
    play_reference: reference,
    engine_snapshot: snapshot,
  }));
  const secondGroup = firstGroup.map((run) => ({
    ...run,
    id: `new-rules-${run.id}`,
    average_turn_ms: run.average_turn_ms! + 100,
    engine_snapshot: { ...snapshot, rules_version: 'GNU Backgammon 1.08' },
  }));
  const changedProtocol = {
    ...firstGroup[0],
    id: 'new-protocol',
    average_turn_ms: 999,
    play_reference: { ...reference, protocol_id: 'play-alternate' },
  };
  expect(
    benchmarkComparisons([
      ...firstGroup.slice(0, 2),
      ...secondGroup.slice(0, 2),
      changedProtocol,
    ]),
  ).toEqual([]);
  const groups = benchmarkComparisons([...firstGroup, ...secondGroup]);
  expect(groups.map((group) => group.median)).toEqual([20, 120]);
});

it('displays recorded thread counts for snapshots and older Open Sage runs', () => {
  expect(
    benchmarkThreadCount({
      ...base,
      engine_version:
        'Open Sage 2.0; model abc; 2ply cubeful; threads 4; adapter 1',
    }),
  ).toBe(4);
  expect(
    benchmarkThreadCount({
      ...base,
      engine_id: 'gnubg',
      engine_snapshot: {
        engine: { engine_id: 'gnubg', gnubg_path: '/gnubg', gnubg_threads: 2 },
      },
    }),
  ).toBe(2);
  expect(
    benchmarkThreadCount({
      ...base,
      engine_version: 'Open Sage 2.0; threads 4; adapter 1',
      engine_snapshot: {
        engine: {
          engine_id: 'bgsage',
          gnubg_path: '/gnubg',
          open_sage: {
            adapter_version: 1,
            artifact_fingerprint: 'model-a',
            package_version: '2.0',
            plies: 2,
            python: '/python',
            threads: 2,
          },
        },
      },
    }),
  ).toBe(2);
  expect(benchmarkThreadCount(base)).toBeUndefined();
});
