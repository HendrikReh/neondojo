import { useCallback, useEffect, useState } from 'react';
import { request, terminal, mergeJob, type Job } from './api';

/** Keep tracking accepted jobs even after their source position or game changes. */
export function useBackgroundJobs() {
  const [tracked, setTracked] = useState<Record<string, Job>>({});
  const track = useCallback((job: Job | undefined) => {
    if (!job) return;
    setTracked((previous) => {
      const current = previous[job.id];
      const latest = mergeJob(current, job);
      if (
        current?.updated_at === latest.updated_at &&
        current?.status === latest.status &&
        current?.progress === latest.progress
      )
        return previous;
      return { ...previous, [job.id]: latest };
    });
  }, []);
  const active = Object.values(tracked).filter((job) => !terminal(job));
  const ids = active
    .map((job) => job.id)
    .sort()
    .join(',');
  useEffect(() => {
    if (!ids) return;
    const abort = new AbortController();
    const refresh = () =>
      Promise.allSettled(
        ids.split(',').map(async (id) => {
          const job = await request<Job>(`/jobs/${id}`, {
            signal: abort.signal,
          });
          if (!abort.signal.aborted) track(job);
        }),
      );
    void refresh();
    const timer = setInterval(() => void refresh(), 3000);
    return () => {
      abort.abort();
      clearInterval(timer);
    };
  }, [ids, track]);
  return { active, track };
}
