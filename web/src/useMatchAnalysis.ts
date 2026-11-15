import { useEffect, useRef, useState } from 'react';
import { mergeJob, request, terminal, type Job } from './api';
import {
  submitMatchAnalysis,
  type MatchAnalysisEngine,
} from './match-analysis-api';

export type { MatchAnalysisEngine } from './match-analysis-api';

/** Jobs and subscriptions belong to the selected match, game scope and engine. */
export function useMatchAnalysis(
  matchId: string,
  engineId: MatchAnalysisEngine,
  gameNo?: number,
  enabled = true,
  onAccepted?: (job: Job) => void,
  externalRevision?: string,
) {
  const scope = `${matchId}:${gameNo ?? 'match'}:${engineId}`;
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
      job.match_uuid === matchId &&
      (job.game_no ?? undefined) === gameNo &&
      (job.engine_snapshot?.engine_id ?? 'gnubg') === engineId;
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
    const query = new URLSearchParams({ engine_id: engineId });
    if (gameNo !== undefined) query.set('game_no', String(gameNo));
    void request<Job | null>(`/matches/${matchId}/analysis?${query}`, {
      signal: abort.signal,
    })
      .then((incoming) => {
        if (abort.signal.aborted) return;
        if (!incoming) {
          setStored((previous) =>
            previous?.scope === scope ? undefined : previous,
          );
          setFailure((previous) =>
            previous?.scope === scope ? undefined : previous,
          );
          return;
        }
        if (!belongs(incoming)) return;
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
  }, [matchId, gameNo, engineId, scope, revision, enabled, externalRevision]);

  async function run(plies: number) {
    if (!enabled) return;
    const ticket = ++submission.current;
    setPending(scope);
    setFailure(undefined);
    try {
      const incoming = await submitMatchAnalysis(
        matchId,
        engineId,
        plies,
        gameNo,
      );
      // Accepted work survives navigation; only its local display is scoped.
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
