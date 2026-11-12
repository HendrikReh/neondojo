import { useOptionalConfiguration } from './ConfigurationProvider';
import { useEffect, useRef, useState } from 'react';
import { request, terminal, mergeJob, type Job } from './api';

export type Jobs = Partial<Record<'analysis' | 'dice', Job>>;
type Ids = Partial<Record<'analysis' | 'dice', string>>;
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

export function useJobs(
  positionId: string | undefined,
  onAccepted?: (job: Job) => void,
) {
  const configuration = useOptionalConfiguration();
  const engineId = configuration?.current?.analysis.engine ?? 'gnubg';
  const storageKey =
    engineId === 'gnubg' ? positionId : `${positionId}.${engineId}`;
  const [jobs, setJobs] = useState<Jobs>({});
  const [revision, setRevision] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const currentPosition = useRef(storageKey);
  currentPosition.current = storageKey;
  useEffect(() => {
    setJobs({});
    setError(null);
    setSubmitting(false);
    if (!positionId) return;
    const abort = new AbortController();
    const streams: EventSource[] = [];
    const timers: ReturnType<typeof setInterval>[] = [];
    for (const [kind, id] of Object.entries(saved(storageKey!))) {
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
            (job.engine_snapshot?.engine_id ?? 'gnubg') !== engineId
          )
            return;
          setJobs((previous) => ({
            ...previous,
            [kind]: mergeJob(previous[kind as keyof Jobs], job),
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
      void refresh().then((job) => {
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
      });
    }
    return () => {
      abort.abort();
      streams.forEach((s) => s.close());
      timers.forEach(clearInterval);
    };
  }, [positionId, storageKey, engineId, revision]);

  async function run(kind: 'analysis' | 'dice', plies: number) {
    if (!positionId) return;
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
      const ids = { ...saved(storageKey!), [kind]: job.id };
      sessionIds.set(storageKey!, ids);
      try {
        localStorage.setItem(`bgtutor.jobs.${storageKey}`, JSON.stringify(ids));
      } catch {
        /* Session tracking remains active without storage. */
      }
      if (currentPosition.current === storageKey)
        setJobs((previous) => ({ ...previous, [kind]: job }));
      if (currentPosition.current === storageKey) setRevision((r) => r + 1);
    } catch (error) {
      if (currentPosition.current === storageKey) setError(error);
    } finally {
      if (currentPosition.current === storageKey) setSubmitting(false);
    }
  }
  return { jobs, run, submitting, error };
}
