import { mergeJob, request, terminal, type Job } from '../http/api';
import type { components } from '../../generated-api';
import { AppError } from '../i18n/i18n-core';
import { requestBackgroundJob } from './job-status-api';

type JobProgress = components['schemas']['JobProgress'];

/** Full reads while only the global display follows a job. */
export const JOB_POLL_MS = 3_000;
/** Full reads while an event stream reports progress, in case it stalls. */
export const JOB_SAFETY_READ_MS = 15_000;

interface Subscriber {
  onJob: (job: Job) => void;
  onError: (error: unknown) => void;
  /** Visible views follow progress over the job's event stream. */
  stream: boolean;
}

interface Watch {
  subscribers: Set<Subscriber>;
  job?: Job;
  stream?: EventSource;
  /** Attempt of the latest `running` event; progress events carry none. */
  streamAttempt?: number;
  timer?: ReturnType<typeof setInterval>;
  timerMs?: number;
  reading?: AbortController;
  readAgain?: boolean;
  /** The job no longer exists; nothing is read or streamed again. */
  missing?: boolean;
  /** The server closed the stream; full reads replace it from now on. */
  streamBlocked?: boolean;
}

// One watch per job ID serves every view that follows the job. Browsers allow
// only a few connections per server, so only visible views open a stream;
// jobs that only the global display follows are read periodically instead.
const watches = new Map<string, Watch>();
const background = new Map<string, Watch>();
let backgroundTimer: ReturnType<typeof setInterval> | undefined;

function hasStreamSubscriber(watch: Watch) {
  return [...watch.subscribers].some((subscriber) => subscriber.stream);
}

function pollInBackground(id: string, watch: Watch, enabled: boolean) {
  if (enabled) background.set(id, watch);
  else background.delete(id);
  if (background.size && backgroundTimer === undefined) {
    backgroundTimer = setInterval(() => {
      for (const [jobId, current] of background) void read(jobId, current);
    }, JOB_POLL_MS);
  } else if (!background.size) {
    clearInterval(backgroundTimer);
    backgroundTimer = undefined;
  }
}

function publish(watch: Watch, job: Job) {
  watch.job = mergeJob(watch.job, job);
  for (const subscriber of [...watch.subscribers]) subscriber.onJob(watch.job);
}

function stop(id: string, watch: Watch) {
  watch.readAgain = false;
  watch.stream?.close();
  watch.stream = undefined;
  clearInterval(watch.timer);
  watch.timer = undefined;
  watch.timerMs = undefined;
  pollInBackground(id, watch, false);
}

/** Open or close the stream and pace full reads for the current subscribers. */
function transport(id: string, watch: Watch) {
  if (watches.get(id) !== watch || !watch.subscribers.size) return;
  if (watch.missing || (watch.job && terminal(watch.job))) {
    stop(id, watch);
    return;
  }
  const local = hasStreamSubscriber(watch);
  const streaming = local && !watch.streamBlocked;
  if (streaming && !watch.stream) open(id, watch);
  if (!streaming && watch.stream) {
    watch.stream.close();
    watch.stream = undefined;
  }
  pollInBackground(id, watch, !local);
  const interval = local
    ? streaming
      ? JOB_SAFETY_READ_MS
      : JOB_POLL_MS
    : undefined;
  if (watch.timerMs !== interval) {
    clearInterval(watch.timer);
    watch.timer = interval
      ? setInterval(() => void read(id, watch), interval)
      : undefined;
    watch.timerMs = interval;
  }
}

/** One full read at a time; a request during a read repeats it once afterwards. */
async function read(id: string, watch: Watch): Promise<void> {
  if (watch.reading) {
    watch.readAgain = true;
    return;
  }
  const abort = new AbortController();
  watch.reading = abort;
  watch.readAgain = false;
  try {
    const job = await (hasStreamSubscriber(watch)
      ? request<Job>(`/jobs/${encodeURIComponent(id)}`, {
          signal: abort.signal,
        })
      : requestBackgroundJob(id, abort.signal));
    if (abort.signal.aborted || watches.get(id) !== watch) return;
    publish(watch, job);
    transport(id, watch);
  } catch (error) {
    if (abort.signal.aborted || watches.get(id) !== watch) return;
    if (error instanceof AppError && error.params.status === '404')
      watch.missing = true;
    for (const subscriber of [...watch.subscribers]) subscriber.onError(error);
    // Other failures are retried at the current reading interval.
    transport(id, watch);
  } finally {
    if (watch.reading === abort) watch.reading = undefined;
  }
  if (watch.readAgain && watches.get(id) === watch) await read(id, watch);
}

/** Apply a progress message without a full read. Replayed history never moves the job back. */
export function applyProgress(
  job: Job,
  event: string,
  progress: JobProgress,
  streamAttempt: number | undefined,
): Job | undefined {
  if (terminal(job)) return undefined;
  if (event === 'running') {
    const attempt = progress.attempt ?? job.attempts;
    if (attempt < job.attempts) return undefined;
    if (attempt === job.attempts && job.status === 'running') return undefined;
    return {
      ...job,
      attempts: attempt,
      status: 'running',
      progress: attempt > job.attempts ? progress.progress : job.progress,
    };
  }
  if (event !== 'progress' || streamAttempt !== job.attempts) return undefined;
  if (progress.progress < job.progress) return undefined;
  if (progress.progress === job.progress && job.status === progress.status)
    return undefined;
  return {
    ...job,
    status: progress.status,
    progress: progress.progress,
    completed_games: progress.completed_games ?? job.completed_games,
    total_games: progress.total_games ?? job.total_games,
  };
}

function open(id: string, watch: Watch) {
  const stream = new EventSource(`/api/jobs/${encodeURIComponent(id)}/events`);
  watch.stream = stream;
  watch.streamAttempt = undefined;
  const receive = (event: string) => (message: MessageEvent) => {
    if (watch.stream !== stream) return;
    if (event === 'completed' || event === 'failed') {
      // The result is only part of the full job.
      void read(id, watch);
      return;
    }
    let progress: JobProgress;
    try {
      progress = JSON.parse(message.data) as JobProgress;
    } catch {
      void read(id, watch);
      return;
    }
    if (event === 'running' && progress.attempt != null)
      watch.streamAttempt = progress.attempt;
    if (!watch.job) return;
    const next = applyProgress(watch.job, event, progress, watch.streamAttempt);
    if (next) publish(watch, next);
  };
  for (const event of ['queued', 'running', 'progress', 'completed', 'failed'])
    stream.addEventListener(event, receive(event));
  // The browser reconnects with Last-Event-ID; a closed stream or a lost
  // connection is synchronised with one full read.
  stream.onerror = () => {
    if (watch.stream !== stream) return;
    if (stream.readyState === EventSource.CLOSED) {
      watch.stream = undefined;
      watch.streamBlocked = true;
    }
    void read(id, watch);
  };
}

/**
 * Follow a job from every view through one shared watch. `onJob` receives the
 * job after the first full read and after every change; `stream` asks for
 * progress over the job's event stream instead of periodic reads.
 */
export function watchJob(
  id: string,
  onJob: (job: Job) => void,
  onError: (error: unknown) => void = () => {},
  stream = true,
): () => void {
  let watch = watches.get(id);
  const created = !watch;
  if (!watch) {
    watch = { subscribers: new Set() };
    watches.set(id, watch);
  }
  const subscriber: Subscriber = { onJob, onError, stream };
  watch.subscribers.add(subscriber);
  if (created) void read(id, watch);
  else {
    const known = watch.job;
    if (known)
      queueMicrotask(() => {
        if (watch.subscribers.has(subscriber)) onJob(watch.job ?? known);
      });
    transport(id, watch);
  }
  return () => {
    watch.subscribers.delete(subscriber);
    if (watch.subscribers.size) {
      transport(id, watch);
      return;
    }
    stop(id, watch);
    watch.reading?.abort();
    watches.delete(id);
  };
}

export type WatchedJobState = Pick<Job, 'id' | 'status' | 'progress'>;

/**
 * Follow the jobs a list shows and report each change of their status or
 * progress, or `undefined` once a job no longer exists. Running jobs follow
 * the event stream; queued ones are read with the global display's periodic
 * batch, so a long queue never opens one connection per job.
 */
export function watchJobChanges(
  jobs: readonly WatchedJobState[],
  onChange: (job: Job | undefined, id: string) => void,
): () => void {
  const stops = jobs.map(({ id, status, progress }) => {
    let last = { status, progress };
    return watchJob(
      id,
      (job) => {
        if (job.status === last.status && job.progress === last.progress)
          return;
        last = { status: job.status, progress: job.progress };
        onChange(job, id);
      },
      (error) => {
        if (error instanceof AppError && error.params.status === '404')
          onChange(undefined, id);
      },
      status === 'running',
    );
  });
  return () => stops.forEach((stop) => stop());
}
