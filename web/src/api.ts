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
  const response = await requestResponse(path, options);
  if (response.status === 204) return undefined as T;
  const data = parseResponse(await response.text());
  if (data === undefined) throw new AppError('error.invalidResponse');
  return data as T;
}

function parseResponse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

export async function requestResponse(
  path: string,
  options: RequestInit = {},
): Promise<Response> {
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
  if (response.ok) return response;

  const text = await response.text();
  const data = parseResponse(text);
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

export const terminal = (job: Job) =>
  job.status === 'completed' || job.status === 'failed';
// SSE and polling can finish out of order. A terminal state for the same job
// is authoritative even if its persisted timestamp moved backwards.
/** Jobs without an engine snapshot predate engine selection and ran with GNUbg. */
/** Error the backend gives a job the user cancelled (`jobs::CANCELLED_ERROR`). */
export const JOB_CANCELLED_ERROR = 'Cancelled by user';
export function isCancelledJob(job: Pick<Job, 'status' | 'error'>) {
  return job.status === 'failed' && job.error === JOB_CANCELLED_ERROR;
}
/** Cancel a queued or running job; it ends as a cancelled `failed` job. */
export function cancelJob(id: string) {
  return request<Job>(`/jobs/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
  });
}
export function jobEngine(job: Pick<Job, 'engine_snapshot'>) {
  return job.engine_snapshot?.engine_id ?? 'gnubg';
}
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

export async function downloadResponse(
  response: Response,
  fallbackName: string,
) {
  const blob = await response.blob();
  const header = response.headers.get('Content-Disposition') ?? '';
  const filename = /filename="([^"]+)"/.exec(header)?.[1] ?? fallbackName;
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
