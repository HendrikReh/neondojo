import type { components } from '../../generated-api';
import { AppError } from '../i18n/i18n-core';

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

export type ErrorIssue = Schema['ConfigurationIssue'];

/** A request the server refused, with the details of its `ErrorBody`. */
export class HttpError extends AppError {
  constructor(
    readonly status: number,
    diagnostic = '',
    /** The stable identifier of the problem, or an empty string. */
    readonly code = '',
    readonly issues: readonly ErrorIssue[] = [],
  ) {
    const preparing = status === 503 && code === 'index_preparing';
    super(
      preparing ? 'error.indexPreparing' : 'error.request',
      { status: String(status) },
      preparing ? '' : diagnostic,
    );
    this.name = 'HttpError';
  }
}

/** A conflict the client found itself, before or after a request. */
export class ConflictError extends AppError {
  constructor(
    readonly code: string,
    diagnostic: string,
  ) {
    super('error.conflict', {}, diagnostic);
    this.name = 'ConflictError';
  }
}

/** Whether the data changed elsewhere, so a reload is needed. */
export const isConflict = (
  error: unknown,
): error is HttpError | ConflictError =>
  error instanceof ConflictError ||
  (error instanceof HttpError && error.status === 409);

function isIssue(value: unknown): value is ErrorIssue {
  if (!value || typeof value !== 'object') return false;
  const issue = value as Record<string, unknown>;
  return ['path', 'code', 'message'].every(
    (field) => typeof issue[field] === 'string',
  );
}

function httpError(status: number, text: string) {
  const data = parseResponse(text);
  if (!data || typeof data !== 'object' || !('error' in data))
    return new HttpError(status, text);
  const body = data as Record<string, unknown>;
  return new HttpError(
    status,
    String(body.error),
    typeof body.code === 'string' ? body.code : '',
    Array.isArray(body.issues) ? body.issues.filter(isIssue) : [],
  );
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

  throw httpError(response.status, await response.text());
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
  const header = response.headers.get('Content-Disposition') ?? '';
  const filename = /filename="([^"]+)"/.exec(header)?.[1] ?? fallbackName;
  saveBlob(await response.blob(), filename);
}

export function saveBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
