import { useOptionalConfiguration } from './ConfigurationProvider';
import { useEffect, useRef, useState } from 'react';
import { request, terminal, mergeJob, type Job } from './api';
import type { MatchAnalysisEngine } from './useMatchAnalysis';

export type Jobs = Partial<Record<'analysis' | 'dice', Job>>;
export function positionJobsPath(
  positionId: string,
  engineId: MatchAnalysisEngine,
) {
  return `/positions/${encodeURIComponent(positionId)}/jobs?engine_id=${engineId}`;
}

/** Submission and progress only. Displayed checker results belong to the catalog. */
export function useJobs(
  positionId: string | undefined,
  onAccepted?: (job: Job) => void,
) {
  const configuration = useOptionalConfiguration();
  const engineId = configuration?.current?.analysis.engine ?? 'gnubg';
  const scope = JSON.stringify([positionId, engineId]);
  const belongs = (job: Job) =>
    job.position_uuid === positionId &&
    (job.engine_snapshot?.engine_id ?? 'gnubg') === engineId &&
    (job.kind === 'analysis' || job.kind === 'dice');
  const current = useRef(scope);
  current.current = scope;
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    scope: string;
    jobs: Jobs;
    error?: unknown;
    restored: boolean;
  }>();
  const [pending, setPending] = useState<string>();
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
    function receive(job: Job) {
      if (abort.signal.aborted || current.current !== scope || !belongs(job))
        return;
      const kind = job.kind === 'dice' ? 'dice' : 'analysis';
      setState((previous) => ({
        scope,
        restored: true,
        jobs: {
          ...(previous?.scope === scope ? previous.jobs : {}),
          [kind]: mergeJob(
            previous?.scope === scope ? previous.jobs[kind] : undefined,
            job,
          ),
        },
      }));
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
        setState({ scope, jobs: {}, restored: true });
        for (const job of jobs) {
          if (belongs(job)) {
            receive(job);
            monitor(job);
          }
        }
      },
      (error: unknown) => {
        if (!abort.signal.aborted && current.current === scope)
          setState({ scope, jobs: {}, restored: true, error });
      },
    );
    return () => {
      abort.abort();
      streams.forEach((stream) => stream.close());
      timers.forEach(clearInterval);
    };
  }, [positionId, engineId, scope, revision]);

  async function run(kind: 'analysis' | 'dice', plies: number) {
    if (!positionId || pending === scope) return;
    setPending(scope);
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
      if (!mounted.current || current.current !== scope || !belongs(job))
        return;
      setState((previous) => ({
        scope,
        jobs: {
          ...(previous?.scope === scope ? previous.jobs : {}),
          [kind]: job,
        },
        restored: true,
      }));
      setRevision((value) => value + 1);
    } catch (error) {
      if (mounted.current && current.current === scope)
        setState((previous) => ({
          scope,
          jobs: previous?.scope === scope ? previous.jobs : {},
          restored: true,
          error,
        }));
    } finally {
      if (mounted.current)
        setPending((value) => (value === scope ? undefined : value));
    }
  }
  return {
    jobs: state?.scope === scope ? state.jobs : {},
    run,
    refresh: () => setRevision((value) => value + 1),
    submitting: pending === scope,
    error: state?.scope === scope ? state.error : undefined,
    restoring: !!positionId && (state?.scope !== scope || !state.restored),
  };
}
