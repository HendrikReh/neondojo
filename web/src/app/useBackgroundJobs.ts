import { useCallback, useEffect, useRef, useState } from 'react';
import { terminal, mergeJob, type Job } from '../shared/api/api';
import { AppError } from '../shared/i18n/i18n-core';
import { watchJob } from '../shared/jobs/job-watch';

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
  // One shared watch per job. The global display reads periodically; a view
  // that shows the same job adds its event stream to the watch instead.
  const watching = useRef(new Map<string, () => void>());
  useEffect(() => {
    const wanted = new Set(ids ? ids.split(',') : []);
    for (const [id, stop] of watching.current)
      if (!wanted.has(id)) {
        stop();
        watching.current.delete(id);
      }
    for (const id of wanted)
      if (!watching.current.has(id))
        watching.current.set(
          id,
          watchJob(
            id,
            track,
            (error) => {
              if (error instanceof AppError && error.params.status === '404')
                setMissing((previous) => new Set([...previous, id]));
            },
            false,
          ),
        );
  }, [ids, track]);
  useEffect(() => {
    const current = watching.current;
    return () => {
      current.forEach((stop) => stop());
      current.clear();
    };
  }, []);
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
