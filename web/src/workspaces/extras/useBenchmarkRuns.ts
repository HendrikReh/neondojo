import { useEffect, useRef, useState } from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import { usePolling } from '../../shared/hooks/usePolling';
import { loadEngines, type EngineInfo } from '../../settings/settings-api';
import {
  cancelBenchmark,
  deleteBenchmark,
  loadBenchmarkReference,
  loadBenchmarks,
  startBenchmarks,
  startPlayBenchmark,
  type BenchmarkReference,
  type BenchmarkRun,
} from './benchmark-api';
import type { useBenchmarkText } from './benchmark-messages';

type BenchmarkText = ReturnType<typeof useBenchmarkText>;

export function benchmarkProgress(
  run: BenchmarkRun,
  t: BenchmarkText,
  formatNumber: (value: number) => string,
) {
  return run.play_reference
    ? t('playProgress', {
        completed: formatNumber(run.completed_turns),
        total: formatNumber(run.play_reference.total_turns),
      })
    : t('progress', {
        completed: formatNumber(run.completed_games),
        total: formatNumber(run.reference?.total_games ?? 0),
        moves: formatNumber(run.analyzed_moves),
        totalMoves: formatNumber(run.reference?.total_moves ?? 0),
      });
}

export function benchmarkStatusLabel(status: string, t: BenchmarkText) {
  switch (status) {
    case 'queued':
    case 'running':
    case 'completed':
    case 'failed':
    case 'cancelled':
      return t(status);
    default:
      return status;
  }
}

export function useBenchmarkRuns({
  visible,
  activeElsewhere = false,
  onActivity,
}: {
  visible: boolean;
  activeElsewhere?: boolean;
  onActivity?: (active: boolean) => void;
}) {
  const [reference, setReference] = useState<BenchmarkReference>();
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineId, setEngineId] = useState('gnubg');
  const [runs, setRuns] = useState<BenchmarkRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [referenceError, setReferenceError] = useState<unknown>();
  const [engineError, setEngineError] = useState<unknown>();
  const [historyError, setHistoryError] = useState<unknown>();
  const [actionError, setActionError] = useState<unknown>();
  const [starting, setStarting] = useState<'analysis' | 'play'>();
  const [cancelling, setCancelling] = useState<string>();
  const [deleting, setDeleting] = useState<string>();
  const [revision, setRevision] = useState(0);
  const [configurationRevision, setConfigurationRevision] = useState(0);
  const [queueActive, setQueueActive] = useState(false);
  const previousActive = useRef(false);
  const activity = useLatest(onActivity);
  // A run the global status learned of, for example on window focus, that
  // this view has not read yet.
  const unseenRun = activeElsewhere && !queueActive;

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    setReferenceError(undefined);
    setEngineError(undefined);
    void loadBenchmarkReference(abort.signal)
      .then((nextReference) => {
        if (!abort.signal.aborted) setReference(nextReference);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setReferenceError(error);
      });
    void loadEngines(abort.signal)
      .then((catalog) => {
        if (abort.signal.aborted) return;
        setEngines(catalog);
        setEngineId((current) =>
          catalog.some((e) => e.id === current && e.available)
            ? current
            : (catalog.find((e) => e.available)?.id ?? ''),
        );
      })
      .catch((error) => {
        if (!abort.signal.aborted) setEngineError(error);
      });
    return () => abort.abort();
  }, [visible, configurationRevision]);

  usePolling(
    async (signal) => {
      try {
        const result = await loadBenchmarks(signal);
        if (signal.aborted) return false;
        setRuns(result.runs);
        setQueueActive(result.active);
        if (previousActive.current && !result.active)
          setConfigurationRevision((r) => r + 1);
        previousActive.current = result.active;
        activity.current?.(result.active);
        setHistoryError(undefined);
      } catch (error) {
        if (!signal.aborted) setHistoryError(error);
      } finally {
        if (!signal.aborted) setLoading(false);
      }
      // Follow an active queue closely. An idle history is read again when
      // a run starts here, the window regains focus, or the global status
      // learns of a run started elsewhere.
      return previousActive.current;
    },
    {
      active: visible,
      intervalMs: 1000,
      refreshOnVisible: true,
      deps: [revision, unseenRun],
    },
  );

  const engine = engines.find((e) => e.id === engineId && e.available);
  const activeRuns = runs.filter(
    (run) => run.status === 'running' || run.status === 'queued',
  );
  const canStart =
    !starting && !queueActive && !loading && engineError === undefined;

  async function startAnalysis(plies: number[]) {
    if (
      !canStart ||
      referenceError !== undefined ||
      !reference ||
      !engine ||
      plies.length === 0
    ) {
      return;
    }
    setStarting('analysis');
    setActionError(undefined);
    try {
      const created = await startBenchmarks(engineId, plies);
      setRuns((current) => [...created, ...current]);
      setQueueActive(true);
      previousActive.current = true;
      activity.current?.(true);
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setStarting(undefined);
    }
  }

  async function startPlay(depth: number | undefined) {
    if (!canStart || depth === undefined) return;
    setStarting('play');
    setActionError(undefined);
    try {
      const created = [await startPlayBenchmark(engineId, depth)];
      setRuns((current) => [...created, ...current]);
      setQueueActive(true);
      previousActive.current = true;
      activity.current?.(true);
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setStarting(undefined);
    }
  }

  async function cancel(id: string) {
    if (cancelling) return;
    setCancelling(id);
    setActionError(undefined);
    try {
      await cancelBenchmark(id);
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setCancelling(undefined);
    }
  }

  async function remove(id: string): Promise<boolean> {
    if (deleting) return false;
    setDeleting(id);
    setActionError(undefined);
    try {
      await deleteBenchmark(id);
      setRuns((current) => current.filter((run) => run.id !== id));
      setRevision((r) => r + 1);
      return true;
    } catch (error) {
      setActionError(error);
      setRevision((r) => r + 1);
      return false;
    } finally {
      setDeleting(undefined);
    }
  }

  function retry() {
    setActionError(undefined);
    setConfigurationRevision((r) => r + 1);
    setRevision((r) => r + 1);
  }

  return {
    reference,
    engines,
    engineId,
    setEngineId,
    engine,
    runs,
    activeRuns,
    loading,
    queueActive,
    canStart,
    referenceError,
    engineError,
    historyError,
    actionError,
    starting,
    cancelling,
    deleting,
    startAnalysis,
    startPlay,
    cancel,
    remove,
    retry,
  };
}
