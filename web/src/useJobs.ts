import { useOptionalConfiguration } from './ConfigurationProvider';
import { useEffect, useMemo, useRef, useState } from 'react';
import { request, terminal, mergeJob, type Job } from './api';

export type Jobs = Partial<Record<'analysis' | 'dice', Job>>;
type Ids = Partial<Record<'analysis' | 'dice' | 'analysisResult', string>>;
const sessionIds = new Map<string, Ids>();

function saved(positionId: string): Ids {
  try {
    return {
      ...JSON.parse(localStorage.getItem(`bgtutor.jobs.${positionId}`) || '{}'),
      ...sessionIds.get(positionId),
    } as Ids;
  } catch {
    return sessionIds.get(positionId) ?? {};
  }
}

function remember(scope: string, ids: Ids) {
  sessionIds.set(scope, ids);
  try {
    localStorage.setItem(`bgtutor.jobs.${scope}`, JSON.stringify(ids));
  } catch {
    /* Session tracking remains active without storage. */
  }
}

export function useJobs(
  positionId: string | undefined,
  onAccepted?: (job: Job) => void,
) {
  const configuration = useOptionalConfiguration();
  const engineId = configuration?.current?.analysis.engine ?? 'gnubg';
  const storageKey =
    positionId &&
    (engineId === 'gnubg' ? positionId : `${positionId}.${engineId}`);
  const [jobs, setJobs] = useState<Jobs>({});
  const [savedAnalysis, setSavedAnalysis] = useState<{
    scope: string;
    job: Job;
  }>();
  const [revision, setRevision] = useState(0);
  const restoreKey = JSON.stringify([storageKey, revision]);
  const [restoredKey, setRestoredKey] = useState<string>();
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const currentPosition = useRef(storageKey);
  currentPosition.current = storageKey;
  useEffect(() => {
    setRestoredKey(undefined);
    setJobs({});
    setError(null);
    setSubmitting(false);
    if (!positionId || !storageKey) return;
    const abort = new AbortController();
    const streams: EventSource[] = [];
    const timers: ReturnType<typeof setInterval>[] = [];
    const restorations: Promise<void>[] = [];
    const scope = storageKey;
    const ids = saved(scope);
    for (const kind of ['analysis', 'dice', 'analysisResult'] as const) {
      const id = ids[kind];
      if (!id || (kind === 'analysisResult' && id === ids.analysis)) continue;
      let stream: EventSource | undefined;
      let timer: ReturnType<typeof setInterval> | undefined;
      const refresh = async () => {
        try {
          const job = await request<Job>(`/jobs/${id}`, {
            signal: abort.signal,
          });
          if (
            abort.signal.aborted ||
            job.position_uuid !== positionId ||
            (job.engine_snapshot?.engine_id ?? 'gnubg') !== engineId ||
            job.kind !== (kind === 'analysisResult' ? 'analysis' : kind)
          )
            return;
          if (kind === 'analysisResult' && job.status !== 'completed') return;
          if (job.kind === 'analysis' && job.status === 'completed') {
            const latest = saved(scope);
            if (kind === 'analysis' && latest.analysis === job.id) {
              if (latest.analysisResult !== job.id)
                remember(scope, { ...latest, analysisResult: job.id });
              setSavedAnalysis({ scope, job });
            } else if (latest.analysisResult === job.id) {
              setSavedAnalysis({ scope, job });
            }
          }
          if (kind !== 'analysisResult')
            setJobs((previous) => ({
              ...previous,
              [kind]: mergeJob(previous[kind], job),
            }));
          if (terminal(job)) {
            stream?.close();
            if (timer) clearInterval(timer);
          }
          return job;
        } catch (error) {
          if (!abort.signal.aborted) setError(error);
        }
      };
      restorations.push(
        refresh().then((job) => {
          if (!job || terminal(job) || abort.signal.aborted) return;
          stream = new EventSource(`/api/jobs/${id}/events`);
          streams.push(stream);
          for (const event of [
            'queued',
            'running',
            'progress',
            'completed',
            'failed',
          ]) {
            stream.addEventListener(event, () => {
              void refresh();
            });
          }
          // Polling also reconciles a terminal transition if a proxy closes SSE.
          timer = setInterval(() => {
            void refresh();
          }, 3000);
          timers.push(timer);
        }),
      );
    }
    void Promise.all(restorations).then(() => {
      if (!abort.signal.aborted) setRestoredKey(restoreKey);
    });
    return () => {
      abort.abort();
      streams.forEach((s) => s.close());
      timers.forEach(clearInterval);
    };
  }, [positionId, storageKey, engineId, restoreKey]);

  async function run(kind: 'analysis' | 'dice', plies: number) {
    if (!positionId || !storageKey) return;
    setSubmitting(true);
    setError(null);
    try {
      const start = () =>
        request<Job>('/jobs', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ position_id: positionId, kind, plies }),
        });
      const job = configuration
        ? await configuration.runWithConfiguration(start)
        : await start();
      onAccepted?.(job);
      const ids = { ...saved(storageKey), [kind]: job.id };
      if (kind === 'analysis' && job.status === 'completed')
        ids.analysisResult = job.id;
      remember(storageKey, ids);
      if (currentPosition.current === storageKey)
        setJobs((previous) => ({ ...previous, [kind]: job }));
      if (currentPosition.current === storageKey) setRevision((r) => r + 1);
    } catch (error) {
      if (currentPosition.current === storageKey) setError(error);
    } finally {
      if (currentPosition.current === storageKey) setSubmitting(false);
    }
  }
  // A position/engine switch renders before the restoration effect runs.
  // Never expose the previous scope's results during that render.
  const currentJobs = useMemo(() => {
    const current: Jobs = {};
    for (const kind of ['analysis', 'dice'] as const) {
      const job = jobs[kind];
      if (
        job &&
        job.position_uuid === positionId &&
        (job.engine_snapshot?.engine_id ?? 'gnubg') === engineId
      )
        current[kind] = job;
    }
    return current;
  }, [jobs, positionId, engineId]);
  return {
    jobs: currentJobs,
    completedAnalysis:
      currentJobs.analysis?.status === 'completed'
        ? currentJobs.analysis
        : savedAnalysis?.scope === storageKey
          ? savedAnalysis?.job
          : undefined,
    run,
    submitting,
    error,
    restoring: !!positionId && restoredKey !== restoreKey,
  };
}
