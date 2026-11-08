import { useEffect, useState } from 'react';
import { mergeJob, request, terminal, type Job } from './api';

/** The backend stores the latest match job; reopening a match needs no local cache. */
export function useMatchAnalysis(matchId: string) {
  const [job, setJob] = useState<Job>();
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [revision, setRevision] = useState(0);
  const [starting, setStarting] = useState<AbortController>();
  useEffect(() => () => starting?.abort(), [starting]);
  useEffect(() => {
    const abort = new AbortController();
    let stream: EventSource | undefined;
    let timer: ReturnType<typeof setInterval> | undefined;
    async function refresh(id: string) {
      try {
        const incoming = await request<Job>(`/jobs/${id}`, {
          signal: abort.signal,
        });
        if (abort.signal.aborted) return;
        setJob((previous) => mergeJob(previous, incoming));
        setError('');
        if (terminal(incoming)) {
          stream?.close();
          clearInterval(timer);
        }
      } catch (error) {
        if (!abort.signal.aborted) setError(String(error));
      }
    }
    void request<Job | null>(`/matches/${matchId}/analysis`, {
      signal: abort.signal,
    })
      .then((incoming) => {
        if (abort.signal.aborted || !incoming) return;
        setJob((previous) => mergeJob(previous, incoming));
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
        if (!abort.signal.aborted) setError(String(error));
      });
    return () => {
      abort.abort();
      stream?.close();
      clearInterval(timer);
    };
  }, [matchId, revision]);

  async function run(plies: number) {
    const abort = new AbortController();
    setStarting(abort);
    setSubmitting(true);
    setError('');
    try {
      const incoming = await request<Job>(`/matches/${matchId}/analysis`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ plies }),
        signal: abort.signal,
      });
      if (abort.signal.aborted) return;
      setJob(incoming);
      setRevision((value) => value + 1);
    } catch (error) {
      if (!abort.signal.aborted) setError(String(error));
    } finally {
      if (!abort.signal.aborted) setSubmitting(false);
    }
  }
  return { job, error, submitting, run };
}
