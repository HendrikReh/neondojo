import { request, type Job } from './api';
import type { components } from './generated-api';
import { AppError } from './i18n-core';

const MAX_GROUP = 100;

interface PendingRead {
  id: string;
  signal: AbortSignal;
  resolve: (job: Job) => void;
  reject: (error: unknown) => void;
}

const pending: PendingRead[] = [];

export function requestBackgroundJob(id: string, signal: AbortSignal) {
  return new Promise<Job>((resolve, reject) => {
    const empty = pending.length === 0;
    pending.push({ id, signal, resolve, reject });
    if (empty) queueMicrotask(flush);
  });
}

function flush() {
  const ready = pending.splice(0).filter((read) => {
    if (!read.signal.aborted) return true;
    read.reject(read.signal.reason);
    return false;
  });
  for (let start = 0; start < ready.length; start += MAX_GROUP)
    void readGroup(ready.slice(start, start + MAX_GROUP));
}

async function readGroup(reads: PendingRead[]) {
  const abort = new AbortController();
  const abortWhenUnused = () => {
    if (reads.every((read) => read.signal.aborted)) abort.abort();
  };
  for (const read of reads)
    read.signal.addEventListener('abort', abortWhenUnused);
  try {
    if (reads.length === 1) {
      const read = reads[0];
      read.resolve(
        await request<Job>(`/jobs/${encodeURIComponent(read.id)}`, {
          signal: abort.signal,
        }),
      );
      return;
    }
    const response = await request<components['schemas']['JobStatusBatch']>(
      '/jobs/status',
      {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ ids: reads.map(({ id }) => id) }),
        signal: abort.signal,
      },
    );
    const available = new Map(response.jobs.map((job) => [job.id, job]));
    const missing = new Set(response.missing);
    const restricted = new Set(response.restricted);
    for (const read of reads) {
      const id = read.id.toLowerCase();
      const job = available.get(id);
      if (job) read.resolve(job);
      else if (missing.has(id))
        read.reject(
          new AppError('error.request', { status: '404' }, 'Not found'),
        );
      else if (restricted.has(id))
        read.reject(
          new AppError(
            'error.request',
            { status: '422' },
            'Complete the competition match before requesting analysis',
          ),
        );
      else read.reject(new AppError('error.invalidResponse'));
    }
  } catch (error) {
    // Only an individual response or an explicit missing ID ends a watch.
    const failure =
      reads.length > 1 &&
      error instanceof AppError &&
      error.params.status === '404'
        ? new AppError('error.invalidResponse')
        : error;
    for (const read of reads) read.reject(failure);
  } finally {
    for (const read of reads)
      read.signal.removeEventListener('abort', abortWhenUnused);
  }
}
