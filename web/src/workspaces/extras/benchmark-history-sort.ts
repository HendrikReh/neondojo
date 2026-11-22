import type { BenchmarkRun } from './benchmark-api';

export type BenchmarkHistorySort =
  'date' | 'engine' | 'plies' | 'total' | 'average' | 'status';
export type SortDirection = 'asc' | 'desc';
export type BenchmarkHistoryOrder = {
  sort: BenchmarkHistorySort;
  direction: SortDirection;
};

/** The server returns the history newest first. */
export const DEFAULT_BENCHMARK_HISTORY_ORDER: BenchmarkHistoryOrder = {
  sort: 'date',
  direction: 'desc',
};

/**
 * Status order is a fixed lifecycle, independent of the localized labels:
 * active runs first, then results, then runs without a result.
 */
const STATUS_ORDER = ['running', 'queued', 'completed', 'failed', 'cancelled'];

/** Dates start newest first; other columns start ascending. */
export function nextBenchmarkHistoryOrder(
  current: BenchmarkHistoryOrder,
  sort: BenchmarkHistorySort,
): BenchmarkHistoryOrder {
  if (current.sort === sort)
    return { sort, direction: current.direction === 'asc' ? 'desc' : 'asc' };
  return { sort, direction: sort === 'date' ? 'desc' : 'asc' };
}

/** Only completed runs have published times; the table shows "—" otherwise. */
function measuredTime(run: BenchmarkRun, value: number | null | undefined) {
  return run.status === 'completed' && typeof value === 'number'
    ? value
    : undefined;
}

function sortValue(
  run: BenchmarkRun,
  sort: BenchmarkHistorySort,
  play: boolean,
): string | number | undefined {
  switch (sort) {
    case 'date':
      return Date.parse(run.created_at);
    case 'engine':
      return run.engine_name;
    case 'plies':
      return run.plies;
    case 'total':
      return measuredTime(run, run.total_elapsed_ms);
    case 'average':
      return measuredTime(
        run,
        play ? run.average_turn_ms : run.average_move_ms,
      );
    case 'status': {
      const rank = STATUS_ORDER.indexOf(run.status);
      return rank === -1 ? STATUS_ORDER.length : rank;
    }
  }
}

/**
 * Sorts one measurement type's history. Runs without a measured time stay at
 * the end in either direction, and ties keep the server's newest-first order.
 * Play histories have no total column, so a total order falls back to date.
 */
export function sortBenchmarkRuns(
  runs: readonly BenchmarkRun[],
  order: BenchmarkHistoryOrder,
  play: boolean,
  locale: string,
): BenchmarkRun[] {
  const { sort, direction } =
    play && order.sort === 'total' ? DEFAULT_BENCHMARK_HISTORY_ORDER : order;
  const sign = direction === 'asc' ? 1 : -1;
  const collator = new Intl.Collator(locale, { sensitivity: 'base' });
  return runs
    .map((run, index) => ({ run, index, value: sortValue(run, sort, play) }))
    .sort((a, b) => {
      if (a.value === undefined || b.value === undefined)
        return a.value === b.value
          ? a.index - b.index
          : a.value === undefined
            ? 1
            : -1;
      const compared =
        typeof a.value === 'string' && typeof b.value === 'string'
          ? collator.compare(a.value, b.value)
          : Number(a.value) - Number(b.value);
      return compared * sign || a.index - b.index;
    })
    .map(({ run }) => run);
}
