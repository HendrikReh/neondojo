import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { Job } from './api';
import { JOB_POLL_MS, JOB_SAFETY_READ_MS, watchJob } from './job-watch';

class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static instances: FakeEventSource[] = [];
  readyState = FakeEventSource.OPEN;
  onerror: (() => void) | null = null;
  private listeners = new Map<string, ((message: MessageEvent) => void)[]>();
  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }
  addEventListener(event: string, listener: (message: MessageEvent) => void) {
    this.listeners.set(event, [...(this.listeners.get(event) ?? []), listener]);
  }
  close() {
    this.readyState = FakeEventSource.CLOSED;
  }
  emit(event: string, data: object) {
    for (const listener of this.listeners.get(event) ?? [])
      listener({ data: JSON.stringify(data) } as MessageEvent);
  }
  fail(readyState: number) {
    this.readyState = readyState;
    this.onerror?.();
  }
}

function job(patch: Partial<Job> = {}): Job {
  return {
    id: 'job-1',
    kind: 'dice',
    status: 'running',
    progress: 40,
    attempts: 1,
    completed_games: 0,
    total_games: 0,
    plies: 3,
    created_at: '2026-11-20T10:00:00Z',
    updated_at: '2026-11-20T10:00:01Z',
    ...patch,
  } as Job;
}

let served: Job;
let reads: number;
let status: number;
const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  vi.stubGlobal('EventSource', FakeEventSource);
  served = job();
  reads = 0;
  status = 200;
  vi.stubGlobal('fetch', async (url: string) => {
    expect(url).toContain('/jobs/job-1');
    reads++;
    return status === 200
      ? Response.json(served)
      : Response.json({ error: 'Job not found' }, { status });
  });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

test('a replayed stream advances progress without full reads and never moves back', async () => {
  const seen: number[] = [];
  const stop = watchJob('job-1', (current) => seen.push(current.progress));
  await flush();
  expect(reads).toBe(1);
  const [stream] = FakeEventSource.instances;
  expect(FakeEventSource.instances).toHaveLength(1);
  // Without Last-Event-ID the server replays the whole history first.
  stream.emit('queued', { status: 'queued', progress: 0 });
  stream.emit('running', { status: 'running', progress: 0, attempt: 1 });
  for (let progress = 4; progress <= 100; progress += 5)
    stream.emit('progress', { status: 'running', progress });
  expect(reads).toBe(1);
  expect(seen[0]).toBe(40);
  expect(seen.slice(1)).toEqual([...seen.slice(1)].sort((a, b) => a - b));
  expect(seen.at(-1)).toBe(99);
  served = job({ status: 'completed', progress: 100 });
  stream.emit('completed', { status: 'completed', progress: 100 });
  await flush();
  expect(reads).toBe(2);
  expect(seen.at(-1)).toBe(100);
  expect(stream.readyState).toBe(FakeEventSource.CLOSED);
  stop();
});

test('progress of an earlier attempt is ignored and a new attempt restarts', async () => {
  served = job({ attempts: 2, progress: 30 });
  const seen: Job[] = [];
  const stop = watchJob('job-1', (current) => seen.push(current));
  await flush();
  const [stream] = FakeEventSource.instances;
  stream.emit('running', { status: 'running', progress: 0, attempt: 1 });
  stream.emit('progress', { status: 'running', progress: 90 });
  expect(seen.at(-1)?.progress).toBe(30);
  stream.emit('running', { status: 'running', progress: 0, attempt: 2 });
  stream.emit('progress', { status: 'running', progress: 50 });
  expect(seen.at(-1)?.progress).toBe(50);
  stream.emit('running', { status: 'running', progress: 0, attempt: 3 });
  expect(seen.at(-1)).toMatchObject({ attempts: 3, progress: 0 });
  stop();
});

test('views share one watch; only visible views open the stream', async () => {
  const global: number[] = [];
  const stopGlobal = watchJob(
    'job-1',
    (current) => global.push(current.progress),
    undefined,
    false,
  );
  await flush();
  expect(FakeEventSource.instances).toHaveLength(0);
  await vi.advanceTimersByTimeAsync(JOB_POLL_MS);
  expect(reads).toBe(2);
  const local: number[] = [];
  const stopLocal = watchJob('job-1', (current) =>
    local.push(current.progress),
  );
  await flush();
  expect(reads).toBe(2);
  expect(local).toEqual([40]);
  expect(FakeEventSource.instances).toHaveLength(1);
  FakeEventSource.instances[0].emit('running', {
    status: 'running',
    progress: 0,
    attempt: 1,
  });
  FakeEventSource.instances[0].emit('progress', {
    status: 'running',
    progress: 60,
  });
  expect(global.at(-1)).toBe(60);
  expect(local.at(-1)).toBe(60);
  // While the stream reports progress, full reads slow down.
  await vi.advanceTimersByTimeAsync(JOB_POLL_MS * 2);
  expect(reads).toBe(2);
  await vi.advanceTimersByTimeAsync(JOB_SAFETY_READ_MS);
  expect(reads).toBe(3);
  stopLocal();
  expect(FakeEventSource.instances[0].readyState).toBe(FakeEventSource.CLOSED);
  await vi.advanceTimersByTimeAsync(JOB_POLL_MS);
  expect(reads).toBe(4);
  stopGlobal();
  await vi.advanceTimersByTimeAsync(JOB_SAFETY_READ_MS);
  expect(reads).toBe(4);
});

test('a lost connection resynchronises once and a closed stream falls back to reads', async () => {
  const stop = watchJob('job-1', () => {});
  await flush();
  const [stream] = FakeEventSource.instances;
  stream.fail(FakeEventSource.CONNECTING);
  await flush();
  expect(reads).toBe(2);
  expect(FakeEventSource.instances).toHaveLength(1);
  stream.fail(FakeEventSource.CLOSED);
  await flush();
  expect(reads).toBe(3);
  // A closed stream is not reopened in a loop.
  expect(FakeEventSource.instances).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(JOB_POLL_MS);
  expect(reads).toBe(4);
  stop();
});

test('a deleted job reports its error once and stops watching', async () => {
  status = 404;
  const errors: unknown[] = [];
  const stop = watchJob(
    'job-1',
    () => {},
    (error) => errors.push(error),
    false,
  );
  await flush();
  await vi.advanceTimersByTimeAsync(JOB_POLL_MS * 3);
  expect(errors).toHaveLength(1);
  expect(errors[0]).toMatchObject({ params: { status: '404' } });
  expect(reads).toBe(1);
  expect(FakeEventSource.instances).toHaveLength(0);
  stop();
});

test('a finished job is read once and never streamed', async () => {
  served = job({ status: 'completed', progress: 100 });
  const seen: Job[] = [];
  const stop = watchJob('job-1', (current) => seen.push(current));
  await flush();
  await vi.advanceTimersByTimeAsync(JOB_SAFETY_READ_MS * 2);
  expect(reads).toBe(1);
  expect(seen).toHaveLength(1);
  expect(FakeEventSource.instances).toHaveLength(0);
  stop();
});
