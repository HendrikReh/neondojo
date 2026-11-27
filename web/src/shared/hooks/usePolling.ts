import { useEffect, type DependencyList } from 'react';
import { useLatest } from './useLatest';

/**
 * Runs an abortable polling callback while `active` is true.
 * Returning `true` from `poll` schedules another run after `intervalMs`.
 */
export function usePolling(
  poll: (signal: AbortSignal) => Promise<boolean | void> | boolean | void,
  {
    active,
    intervalMs,
    refreshOnVisible = false,
    deps = [],
  }: {
    active: boolean;
    intervalMs: number;
    refreshOnVisible?: boolean;
    deps?: DependencyList;
  },
) {
  const latestPoll = useLatest(poll);

  useEffect(() => {
    if (!active) return;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reading = false;

    async function run() {
      reading = true;
      timer = undefined;
      let pollAgain = false;
      try {
        pollAgain = Boolean(await latestPoll.current(abort.signal));
      } finally {
        reading = false;
        if (!abort.signal.aborted && pollAgain) {
          timer = setTimeout(() => void run(), intervalMs);
        }
      }
    }

    void run();

    if (!refreshOnVisible) {
      return () => {
        abort.abort();
        clearTimeout(timer);
      };
    }

    const refresh = () => {
      if (document.visibilityState === 'visible' && !reading && !timer) {
        void run();
      }
    };
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', refresh);
    return () => {
      abort.abort();
      clearTimeout(timer);
      window.removeEventListener('focus', refresh);
      document.removeEventListener('visibilitychange', refresh);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, intervalMs, refreshOnVisible, latestPoll, ...deps]);
}
