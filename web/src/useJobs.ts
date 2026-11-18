import { useEffect, useMemo, useRef, useState } from 'react';
import { request, terminal, mergeJob, jobEngine, type Job } from './api';
import type { components } from './generated-api';
import type { MatchAnalysisEngine } from './useMatchAnalysis';

export type Jobs = Partial<Record<'analysis' | 'dice', Job>>;
export function positionJobsPath(
  positionId: string,
  engineId?: MatchAnalysisEngine,
) {
  const path = `/positions/${encodeURIComponent(positionId)}/jobs`;
  return engineId
    ? `${path}?${new URLSearchParams({ engine_id: engineId })}`
    : path;
}

/**
 * Submission and progress only, for the active engine. Displayed checker
 * results belong to the catalog, which lists every engine's saved results.
 */
export function useJobs(
  positionId: string | undefined,
  engineId: MatchAnalysisEngine,
  onAccepted?: (job: Job) => void,
) {
  const scope = useMemo(
    () => ({ positionId, engineId }),
    [positionId, engineId],
  );
  const current = useRef(scope);
  current.current = scope;
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    scope: typeof scope;
    jobs: Jobs;
    error?: unknown;
    restored: boolean;
    failure?: keyof Jobs | 'restore';
  }>();
  const submissions = useRef(new Set<string>());
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set());
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (!positionId) return;
    const abort = new AbortController();
    const streams: EventSource[] = [];
    const timers: ReturnType<typeof setInterval>[] = [];
    const belongs = (job: Job) =>
      job.position_uuid === positionId &&
      jobEngine(job) === engineId &&
      (job.kind === 'analysis' || job.kind === 'dice');
    function receive(job: Job) {
      if (abort.signal.aborted || current.current !== scope || !belongs(job))
        return;
      const kind = job.kind === 'dice' ? 'dice' : 'analysis';
      setState((previous) => {
        const selected = previous?.scope === scope ? previous : undefined;
        return {
          scope,
          restored: true,
          jobs: {
            ...selected?.jobs,
            [kind]: mergeJob(selected?.jobs[kind], job),
          },
          failure: selected?.failure,
          error: selected?.failure ? selected.error : undefined,
        };
      });
    }
    function monitor(job: Job) {
      if (terminal(job)) return;
      const stream = new EventSource(`/api/jobs/${job.id}/events`);
      streams.push(stream);
      let timer: ReturnType<typeof setInterval>;
      async function refresh() {
        try {
          const incoming = await request<Job>(`/jobs/${job.id}`, {
            signal: abort.signal,
          });
          receive(incoming);
          if (terminal(incoming)) {
            stream.close();
            clearInterval(timer);
          }
        } catch (error) {
          if (!abort.signal.aborted && current.current === scope)
            setState((previous) => ({
              scope,
              jobs: previous?.scope === scope ? previous.jobs : {},
              restored: true,
              error,
            }));
        }
      }
      for (const event of [
        'queued',
        'running',
        'progress',
        'completed',
        'failed',
      ])
        stream.addEventListener(event, () => {
          void refresh();
        });
      timer = setInterval(() => {
        void refresh();
      }, 3000);
      timers.push(timer);
    }
    void request<Job[]>(positionJobsPath(positionId, engineId), {
      signal: abort.signal,
    }).then(
      (jobs) => {
        if (abort.signal.aborted || current.current !== scope) return;
        setState((previous) => {
          const failedSubmission =
            previous?.scope === scope &&
            previous.failure &&
            previous.failure !== 'restore';
          return {
            scope,
            jobs: {},
            restored: true,
            failure: failedSubmission ? previous.failure : undefined,
            error: failedSubmission ? previous.error : undefined,
          };
        });
        for (const job of jobs) {
          if (belongs(job)) {
            receive(job);
            monitor(job);
          }
        }
      },
      (error: unknown) => {
        if (!abort.signal.aborted && current.current === scope)
          setState({
            scope,
            jobs: {},
            restored: true,
            failure: 'restore',
            error,
          });
      },
    );
    return () => {
      abort.abort();
      streams.forEach((stream) => stream.close());
      timers.forEach(clearInterval);
    };
  }, [positionId, engineId, scope, revision]);

  async function run(kind: 'analysis' | 'dice', plies: number) {
    if (!positionId || submissions.current.has(positionId)) return;
    submissions.current.add(positionId);
    setPending(new Set(submissions.current));
    setState((previous) =>
      previous?.scope === scope
        ? { ...previous, error: undefined, failure: undefined }
        : previous,
    );
    try {
      const job = await request<Job>('/jobs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          position_id: positionId,
          kind,
          plies,
          engine_id: engineId,
        } satisfies components['schemas']['JobRequest']),
      });
      onAccepted?.(job);
      const selected = current.current;
      if (
        !mounted.current ||
        selected.positionId !== positionId ||
        selected.engineId !== engineId
      )
        return;
      setState((previous) => ({
        scope: selected,
        jobs: {
          ...(previous?.scope === selected ? previous.jobs : {}),
          [kind]: job,
        },
        restored: true,
      }));
      setRevision((value) => value + 1);
    } catch (error) {
      const selected = current.current;
      if (
        mounted.current &&
        selected.positionId === positionId &&
        selected.engineId === engineId
      )
        setState((previous) => ({
          scope: selected,
          jobs: previous?.scope === selected ? previous.jobs : {},
          restored: previous?.scope === selected && previous.restored,
          failure: kind,
          error,
        }));
    } finally {
      submissions.current.delete(positionId);
      if (mounted.current) setPending(new Set(submissions.current));
    }
  }
  return {
    jobs: state?.scope === scope ? state.jobs : {},
    run,
    refresh: () => setRevision((value) => value + 1),
    submitting: !!positionId && pending.has(positionId),
    error: state?.scope === scope ? state.error : undefined,
    failure: state?.scope === scope ? state.failure : undefined,
    restoring: !!positionId && (state?.scope !== scope || !state.restored),
  };
}
