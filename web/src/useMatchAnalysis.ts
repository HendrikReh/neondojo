import { useEffect, useRef, useState } from 'react';
import { mergeJob, request, terminal, type Job } from './api';

/** Jobs and subscriptions belong to one match/game scope, including late responses. */
export function useMatchAnalysis(
  matchId: string,
  gameNo?: number,
  enabled = true,
  onAccepted?: (job: Job) => void,
) {
  const scope = `${matchId}:${gameNo ?? 'match'}`;
  const [stored, setStored] = useState<{ scope: string; job: Job }>();
  const [failure, setFailure] = useState<{ scope: string; error: unknown }>();
  const [pending, setPending] = useState<string>();
  const [revision, setRevision] = useState(0);
  const currentScope = useRef(scope);
  currentScope.current = scope;
  const mounted = useRef(false);
  const submission = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    let stream: EventSource | undefined;
    let timer: ReturnType<typeof setInterval> | undefined;
    const belongs = (job: Job) =>
      job.match_uuid === matchId && (job.game_no ?? undefined) === gameNo;
    function receive(incoming: Job) {
      if (abort.signal.aborted || !belongs(incoming)) return;
      setStored((previous) => ({
        scope,
        job: mergeJob(
          previous?.scope === scope ? previous.job : undefined,
          incoming,
        ),
      }));
      setFailure(undefined);
    }
    async function refresh(id: string) {
      try {
        const incoming = await request<Job>(`/jobs/${id}`, {
          signal: abort.signal,
        });
        receive(incoming);
        if (terminal(incoming)) {
          stream?.close();
          clearInterval(timer);
        }
      } catch (error) {
        if (!abort.signal.aborted) setFailure({ scope, error });
      }
    }
    void request<Job | null>(
      `/matches/${matchId}/analysis${gameNo === undefined ? '' : `?game_no=${gameNo}`}`,
      { signal: abort.signal },
    )
      .then((incoming) => {
        if (abort.signal.aborted || !incoming || !belongs(incoming)) return;
        receive(incoming);
        if (terminal(incoming)) return;
        stream = new EventSource(`/api/jobs/${incoming.id}/events`);
        for (const event of [
          'queued',
          'running',
          'progress',
          'completed',
          'failed',
        ]) {
          stream.addEventListener(event, () => {
            void refresh(incoming.id);
          });
        }
        timer = setInterval(() => {
          void refresh(incoming.id);
        }, 3000);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setFailure({ scope, error });
      });
    return () => {
      abort.abort();
      stream?.close();
      clearInterval(timer);
    };
  }, [matchId, gameNo, scope, revision, enabled]);

  async function run(plies: number) {
    if (!enabled) return;
    const ticket = ++submission.current;
    setPending(scope);
    setFailure(undefined);
    try {
      // Accepted work survives navigation; only its local display is scoped.
      const incoming = await request<Job>(`/matches/${matchId}/analysis`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plies, game_no: gameNo }),
      });
      onAccepted?.(incoming);
      if (
        !mounted.current ||
        currentScope.current !== scope ||
        ticket !== submission.current
      )
        return;
      setStored({ scope, job: incoming });
      setRevision((value) => value + 1);
    } catch (error) {
      if (
        mounted.current &&
        currentScope.current === scope &&
        ticket === submission.current
      )
        setFailure({ scope, error });
    } finally {
      if (mounted.current && ticket === submission.current)
        setPending(undefined);
    }
  }

  return {
    job: enabled && stored?.scope === scope ? stored.job : undefined,
    error: enabled && failure?.scope === scope ? failure.error : null,
    submitting: pending === scope,
    run,
  };
}
