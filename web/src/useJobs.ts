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
  const [jobs, setJobs] = useState<Jobs>({});
  const [revision, setRevision] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const currentPosition = useRef(positionId);
  currentPosition.current = positionId;
  useEffect(() => {
    setJobs({});
    setError(null);
    setSubmitting(false);
    if (!positionId) return;
    const abort = new AbortController();
    const streams: EventSource[] = [];
    const timers: ReturnType<typeof setInterval>[] = [];
    for (const [kind, id] of Object.entries(saved(positionId))) {
      let stream: EventSource | undefined;
      let timer: ReturnType<typeof setInterval> | undefined;
      const refresh = async () => {
        try {
          const job = await request<Job>(`/jobs/${id}`, {
            signal: abort.signal,
          });
          if (abort.signal.aborted || job.position_uuid !== positionId) return;
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
  }, [positionId, revision]);

  async function run(kind: 'analysis' | 'dice', plies: number) {
    if (!positionId) return;
    setSubmitting(true);
    setError(null);
    try {
      const job = await request<Job>('/jobs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ position_id: positionId, kind, plies }),
      });
      onAccepted?.(job);
      const ids = { ...saved(positionId), [kind]: job.id };
      sessionIds.set(positionId, ids);
      try {
        localStorage.setItem(`bgtutor.jobs.${positionId}`, JSON.stringify(ids));
      } catch {
        /* Session tracking remains active without storage. */
      }
      if (currentPosition.current === positionId)
        setJobs((previous) => ({ ...previous, [kind]: job }));
      if (currentPosition.current === positionId) setRevision((r) => r + 1);
    } catch (error) {
      if (currentPosition.current === positionId) setError(error);
    } finally {
      if (currentPosition.current === positionId) setSubmitting(false);
    }
  }
  return { jobs, run, submitting, error };
}
