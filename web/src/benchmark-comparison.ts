import type { BenchmarkRun } from './benchmark-api';
/** Only compare complete measurements with identical machine, profile and workload. */
export function benchmarkComparisons(runs: BenchmarkRun[]) {
  const groups = new Map<string, BenchmarkRun[]>();
  for (const run of runs) {
    const value = run.play_reference
      ? run.average_turn_ms
      : run.average_move_ms;
    if (
      run.status !== 'completed' ||
      value == null ||
      !Number.isFinite(value) ||
      value <= 0
    )
      continue;
    const key = JSON.stringify([
      run.machine,
      run.engine_id,
      run.engine_version,
      run.engine_path,
      run.engine_snapshot,
      run.plies,
      run.reference,
      run.protocol,
      run.play_reference,
    ]);
    const group = groups.get(key) ?? [];
    group.push(run);
    groups.set(key, group);
  }
  return [...groups.values()]
    .filter((group) => group.length >= 3)
    .map((group) => {
      const sample = [...group]
        .sort((a, b) =>
          (b.finished_at ?? '').localeCompare(a.finished_at ?? ''),
        )
        .slice(0, 3);
      const times = sample
        .map(
          (run) =>
            (run.play_reference ? run.average_turn_ms : run.average_move_ms)!,
        )
        .sort((a, b) => a - b);
      return {
        run: sample[0],
        count: 3,
        median: times[1],
        minimum: times[0],
        maximum: times[2],
      };
    });
}
