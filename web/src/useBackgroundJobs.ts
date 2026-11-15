import { useCallback, useEffect, useState } from 'react';
import { request, terminal, mergeJob, type Job } from './api';
import { AppError } from './i18n-core';

/** Keep tracking accepted jobs even after their source position or game changes. */
export function useBackgroundJobs(watchedIds: readonly string[]) {
  const [tracked, setTracked] = useState<Record<string, Job>>({});
  const [missing, setMissing] = useState<ReadonlySet<string>>(() => new Set());
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
  const active = Object.values(tracked).filter(
    (job) => !terminal(job) && !missing.has(job.id),
  );
  const ids = [
    ...active.map((job) => job.id),
    ...watchedIds.filter((id) => !tracked[id] && !missing.has(id)),
  ]
    .sort()
    .join(',');
  useEffect(() => {
    if (!ids) return;
    const abort = new AbortController();
    const refresh = () =>
      Promise.allSettled(
        ids.split(',').map(async (id) => {
          try {
            const job = await request<Job>(`/jobs/${id}`, {
              signal: abort.signal,
            });
            if (!abort.signal.aborted) track(job);
          } catch (error) {
            if (
              !abort.signal.aborted &&
              error instanceof AppError &&
              error.params.status === '404'
            )
              setMissing((previous) => new Set([...previous, id]));
          }
        }),
      );
    void refresh();
    const timer = setInterval(() => void refresh(), 3000);
    return () => {
      abort.abort();
      clearInterval(timer);
    };
  }, [ids, track]);
  const assessmentRevision = Object.values(tracked)
    .filter(
      (job) =>
        job.status === 'completed' &&
        (job.kind === 'analysis' || job.kind === 'match'),
    )
    .map((job) => `${job.id}:${job.updated_at}`)
    .sort()
    .join('|');
  return { active, tracked, track, missing, assessmentRevision };
}
