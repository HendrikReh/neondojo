import { request } from './shared/api/api';
import type { components } from './generated-api';

export type RemainingTime = Required<
  components['schemas']['AnalysisRemainingTime']
>;
export type RemainingState = RemainingTime['state'];

const states: RemainingState[] = [
  'queued',
  'waiting',
  'estimated',
  'reestimating',
  'unavailable',
  'finishing',
  'completed',
  'failed',
];
const object = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
const integer = (v: unknown): v is number =>
  typeof v === 'number' && Number.isSafeInteger(v) && v >= 0;
const finite = (v: unknown): v is number =>
  typeof v === 'number' && Number.isFinite(v) && v >= 0;
function parseRemaining(value: unknown): RemainingTime {
  if (
    !object(value) ||
    typeof value.job_id !== 'string' ||
    !value.job_id ||
    !['queued', 'running', 'completed', 'failed'].includes(
      String(value.status),
    ) ||
    !states.includes(value.state as RemainingState) ||
    !['benchmark', 'observed', null].includes(
      value.basis as 'benchmark' | 'observed' | null,
    ) ||
    !integer(value.total_moves) ||
    !integer(value.completed_moves) ||
    value.completed_moves > value.total_moves ||
    !integer(value.timed_moves) ||
    value.timed_moves > value.completed_moves ||
    !integer(value.attempt) ||
    !finite(value.measured_elapsed_ms) ||
    !finite(value.current_game_elapsed_ms) ||
    typeof value.sampled_at !== 'string' ||
    !Number.isFinite(Date.parse(value.sampled_at)) ||
    (value.state === 'estimated'
      ? value.status !== 'running' ||
        value.basis === null ||
        !finite(value.remaining_ms) ||
        value.remaining_ms <= 0
      : value.state === 'waiting'
        ? value.remaining_ms !== null &&
          (!finite(value.remaining_ms) || value.remaining_ms <= 0)
        : value.remaining_ms !== null)
  )
    throw new Error('Invalid remaining analysis time');
  return value as unknown as RemainingTime;
}
export async function loadRemainingTime(jobId: string, signal: AbortSignal) {
  const value = await request<unknown>(
    `/jobs/${encodeURIComponent(jobId)}/remaining-time`,
    { signal },
  );
  const result = parseRemaining(value);
  if (result.job_id !== jobId)
    throw new Error('Remaining time belongs to another job');
  return result;
}
export function remainingTimeAt(
  value: RemainingTime,
  receivedAt: number,
  now: number,
): RemainingTime {
  if (value.state !== 'estimated' || value.remaining_ms === null) return value;
  const remaining = value.remaining_ms - Math.max(0, now - receivedAt);
  return remaining > 0
    ? { ...value, remaining_ms: remaining }
    : { ...value, state: 'reestimating', remaining_ms: null };
}
