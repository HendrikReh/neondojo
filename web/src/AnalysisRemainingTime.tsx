import { useEffect, useState } from 'react';
import type { Job } from './shared/api/api';
import { useI18n } from './shared/i18n/i18n';
import { formatAnalysisDuration } from './analysis-time-estimate-messages';
import {
  loadRemainingTime,
  remainingTimeAt,
  type RemainingTime,
} from './analysis-remaining-time-api';
import { remainingTimeText } from './analysis-remaining-time-messages';
import './analysis-remaining-time.css';

export default function AnalysisRemainingTime({ job }: { job: Job }) {
  const { locale } = useI18n();
  const enabled = job.kind === 'match' && job.status === 'running';
  const key = JSON.stringify([
    job.id,
    job.status,
    job.attempts,
    job.updated_at,
    job.completed_games,
  ]);
  const [saved, setSaved] = useState<{
    key: string;
    value?: RemainingTime;
    receivedAt: number;
    failed?: boolean;
  }>();
  const [now, setNow] = useState(() => performance.now());
  const stored = saved?.key === key ? saved : undefined;
  const timing = stored?.value
    ? remainingTimeAt(stored.value, stored.receivedAt, now)
    : undefined;
  const state =
    job.status === 'queued'
      ? 'queued'
      : (timing?.state ?? (stored?.failed ? 'unavailable' : 'loading'));
  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    let retry: ReturnType<typeof setTimeout> | undefined;
    async function refresh() {
      try {
        const value = await loadRemainingTime(job.id, abort.signal);
        if (abort.signal.aborted) return;
        if (value.attempt < job.attempts)
          throw new Error('Stale worker timing');
        const receivedAt = performance.now();
        setNow(receivedAt);
        setSaved({ key, value, receivedAt });
        if (
          ['queued', 'waiting', 'unavailable', 'finishing'].includes(
            value.state,
          )
        )
          retry = setTimeout(() => void refresh(), 3000);
      } catch {
        if (abort.signal.aborted) return;
        setSaved({ key, receivedAt: performance.now(), failed: true });
        retry = setTimeout(() => void refresh(), 3000);
      }
    }
    void refresh();
    return () => {
      abort.abort();
      clearTimeout(retry);
    };
  }, [enabled, key, job.id, job.attempts]);
  useEffect(() => {
    if (!enabled || state !== 'estimated') return;
    const timer = setInterval(() => setNow(performance.now()), 1000);
    return () => clearInterval(timer);
  }, [enabled, state, key]);
  if (
    job.kind !== 'match' ||
    ['completed', 'failed'].includes(job.status) ||
    ['completed', 'failed'].includes(state)
  )
    return null;
  const duration =
    timing?.remaining_ms != null
      ? formatAnalysisDuration(timing.remaining_ms)
      : null;
  const text =
    state === 'estimated' && duration
      ? remainingTimeText(locale, 'estimated', { duration })
      : remainingTimeText(
          locale,
          state === 'estimated'
            ? 'reestimating'
            : (state as
                | 'queued'
                | 'waiting'
                | 'loading'
                | 'unavailable'
                | 'finishing'
                | 'reestimating'),
        );
  return (
    <div
      className="analysis-remaining-time"
      aria-live="off"
      data-remaining-state={state}
    >
      <span>{text}</span>
      {timing?.basis && state === 'estimated' && (
        <small>{remainingTimeText(locale, timing.basis)}</small>
      )}
    </div>
  );
}
