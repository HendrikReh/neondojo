import type { components } from './generated-api';
import { AppError } from './i18n-core';

type Schema = components['schemas'];
export type Match = Schema['MatchSummary'];
export type MatchGame = Schema['MatchGame'];
export type Position = Schema['Position'];
export type Candidate = Schema['Candidate'];
export type Analysis = Schema['Analysis'];
export type Roll = Schema['DiceEvaluation'];
export type Distribution = Schema['Distribution'];
export type Job = Schema['Job'];
export type DiceStats = Schema['DiceStats'];
export type MatchAnalysisSummary = Schema['MatchAnalysisSummary'];
export type MoveAnalysis = Schema['MoveAnalysis'];
export type MoveAnalysisPage = Schema['MoveAnalysisPage'];
export interface Page<T> {
  items: T[];
  next_cursor: string | null;
}

export async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api${path}`, options);
  } catch (error) {
    if (error instanceof Error && error.name === 'AbortError') throw error;
    throw new AppError(
      'error.network',
      {},
      error instanceof Error ? error.message : String(error),
    );
  }
  const text = await response.text();
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    data = undefined;
  }
  if (!response.ok) {
    const error =
      data && typeof data === 'object' && 'error' in data
        ? String(data.error)
        : text;
    throw new AppError(
      'error.request',
      { status: String(response.status) },
      error,
    );
  }
  if (response.status === 204) return undefined as T;
  if (data === undefined) throw new AppError('error.invalidResponse');
  return data as T;
}

export const terminal = (job: Job) =>
  job.status === 'completed' || job.status === 'failed';
// SSE and polling can finish out of order. A terminal state for the same job
// is authoritative even if its persisted timestamp moved backwards.
export function mergeJob(previous: Job | undefined, incoming: Job): Job {
  if (
    previous?.id === incoming.id &&
    (terminal(previous) ||
      (!terminal(incoming) &&
        Date.parse(previous.updated_at) > Date.parse(incoming.updated_at)))
  ) {
    return previous;
  }
  return incoming;
}
export const equity = (value: number) =>
  `${value >= 0 ? '+' : ''}${value.toFixed(3)}`;
