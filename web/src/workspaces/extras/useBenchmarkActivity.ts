import { useCallback, useEffect, useRef, useState } from 'react';
import { loadBenchmarkStatus } from './benchmark-api';

/** Status reads while a benchmark run is queued or running. */
export const BENCHMARK_STATUS_POLL_MS = 2_000;

/**
 * Whether any benchmark run is queued or running, for the global notice.
 * The status is read at start, whenever the window regains focus or becomes
 * visible, and periodically only while a run is active; the benchmark view
 * reports what its own reads and starts learn.
 */
export function useBenchmarkActivity() {
  const [active, setActive] = useState(false);
  // A newer read or report wins over a read still in flight.
  const sequence = useRef(0);
  const report = useCallback((value: boolean) => {
    sequence.current += 1;
    setActive(value);
  }, []);
  const [checks, setChecks] = useState(0);
  useEffect(() => {
    const check = () => {
      if (document.visibilityState === 'visible') setChecks((n) => n + 1);
    };
    window.addEventListener('focus', check);
    document.addEventListener('visibilitychange', check);
    return () => {
      window.removeEventListener('focus', check);
      document.removeEventListener('visibilitychange', check);
    };
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    const read = ++sequence.current;
    loadBenchmarkStatus(abort.signal)
      .then((status) => {
        if (!abort.signal.aborted && read === sequence.current)
          setActive(status.active === true);
      })
      .catch(() => {
        // Status is advisory; a temporary failure must not block the app.
      });
    return () => abort.abort();
  }, [checks]);
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(
      () => setChecks((n) => n + 1),
      BENCHMARK_STATUS_POLL_MS,
    );
    return () => clearInterval(timer);
  }, [active]);
  return { active, report };
}
