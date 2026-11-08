import type { components } from './generated-api';

type Schema = components['schemas'];
export type Match = Schema['MatchSummary'];
export type Position = Schema['Position'];
export type Candidate = Schema['Candidate'];
export type Analysis = Schema['Analysis'];
export type Roll = Schema['DiceEvaluation'];
export type Distribution = Schema['Distribution'];
export type Job = Schema['Job'];
export type DiceStats = Schema['DiceStats'];
export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

export async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const response = await fetch(`/api${path}`, options);
  const text = await response.text();
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    data = null;
  }
  if (!response.ok) {
    const error =
      data && typeof data === 'object' && 'error' in data
        ? String(data.error)
        : text;
    throw new Error(error || `HTTP ${response.status}`);
  }
  if (data === null)
    throw new Error('Das Backend hat keine gültige Antwort geliefert.');
  return data as T;
}

export const terminal = (job: Job) =>
  job.status === 'completed' || job.status === 'failed';
// SSE and polling can finish out of order. Terminal jobs are immutable;
// retries are accepted only when the persisted update is newer.
export function mergeJob(previous: Job | undefined, incoming: Job): Job {
  if (
    previous?.id === incoming.id &&
    (terminal(previous) ||
      Date.parse(previous.updated_at) > Date.parse(incoming.updated_at))
  ) {
    return previous;
  }
  return incoming;
}
export const equity = (value: number) =>
  `${value >= 0 ? '+' : ''}${value.toFixed(3)}`;
