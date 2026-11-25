import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import type { Job } from '../http/api';
import {
  JOB_POLL_MS,
  JOB_SAFETY_READ_MS,
  watchJob,
  watchJobChanges,
} from './job-watch';

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

test.each([200, 404])(
  'stream closure reconciles final HTTP %s once and stops retrying',
  async (finalStatus) => {
    const seen = vi.fn();
    const errors = vi.fn();
    const stop = watchJob('job-1', seen, errors);
    try {
      await flush();
      const [stream] = FakeEventSource.instances;
      served = job({ status: 'completed', progress: 100 });
      status = finalStatus;
      // EOF can start a reconnect while the final status read is in flight.
      stream.fail(FakeEventSource.CONNECTING);
      stream.fail(FakeEventSource.CLOSED);
      await vi.advanceTimersByTimeAsync(JOB_SAFETY_READ_MS * 2);
      expect(reads).toBe(2);
      expect(FakeEventSource.instances).toHaveLength(1);
      if (finalStatus === 404) {
        expect(errors).toHaveBeenCalledTimes(1);
        expect(errors.mock.calls[0][0]).toMatchObject({
          params: { status: '404' },
        });
      } else {
        expect(seen).toHaveBeenCalledTimes(2);
        expect(seen).toHaveBeenLastCalledWith(served);
      }
    } finally {
      stop();
    }
  },
);

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

test('24 background jobs use one request per refresh', async () => {
  const ids = Array.from({ length: 24 }, (_, index) => `bulk-${index}`);
  const seen = new Map<string, number>();
  const requested: number[] = [];
  vi.stubGlobal('fetch', async (url: string, options?: RequestInit) => {
    const batch = url === '/api/jobs/status';
    const group = batch
      ? (JSON.parse(String(options?.body)).ids as string[])
      : [url.replace('/api/jobs/', '')];
    requested.push(group.length);
    const jobs = group.map((id) => job({ id }));
    return Response.json(
      batch ? { jobs, missing: [], restricted: [] } : jobs[0],
    );
  });
  const stops = ids.map((id) =>
    watchJob(id, () => seen.set(id, (seen.get(id) ?? 0) + 1), undefined, false),
  );
  try {
    await flush();
    await vi.advanceTimersByTimeAsync(JOB_POLL_MS * 5);
    console.log(
      `24 background jobs and 6 refreshes: ${requested.length} requests, ${requested.reduce((count, size) => count + size, 0)} job records requested`,
    );
    expect(FakeEventSource.instances).toHaveLength(0);
    expect([...seen.values()]).toEqual(ids.map(() => 6));
    expect(requested).toEqual([24, 24, 24, 24, 24, 24]);
  } finally {
    stops.forEach((stop) => stop());
  }
});

test('background batches keep missing, restricted and completed jobs independent', async () => {
  const requests: string[][] = [];
  const errors = new Map<string, unknown>();
  const seen = new Map<string, Job>();
  vi.stubGlobal('fetch', async (url: string, options?: RequestInit) => {
    expect(url).toBe('/api/jobs/status');
    const { ids } = JSON.parse(String(options?.body)) as { ids: string[] };
    requests.push(ids);
    return requests.length === 1
      ? Response.json({
          jobs: [
            job({ id: 'done', status: 'completed', progress: 100 }),
            job({ id: 'open' }),
          ],
          missing: ['missing'],
          restricted: ['restricted'],
        })
      : Response.json({
          jobs: ids.map((id) => job({ id, progress: 60 })),
          missing: [],
          restricted: [],
        });
  });
  const stops = ['done', 'open', 'missing', 'restricted'].map((id) =>
    watchJob(
      id,
      (value) => seen.set(id, value),
      (error) => errors.set(id, error),
      false,
    ),
  );
  try {
    await flush();
    expect(seen.get('done')).toMatchObject({
      status: 'completed',
      progress: 100,
    });
    expect(errors.get('missing')).toMatchObject({ params: { status: '404' } });
    expect(errors.get('restricted')).toMatchObject({
      params: { status: '422' },
    });
    await vi.advanceTimersByTimeAsync(JOB_POLL_MS);
    expect(requests[1]).toEqual(['open', 'restricted']);
    expect(seen.get('restricted')?.progress).toBe(60);
    expect(seen.has('missing')).toBe(false);
  } finally {
    stops.forEach((stop) => stop());
  }
});

test('a failed batch request does not remove its jobs from later refreshes', async () => {
  const seen: string[] = [];
  const ids = ['first', 'second'];
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      Response.json({ error: 'Not found' }, { status: 404 }),
    )
    .mockResolvedValueOnce(
      Response.json({
        jobs: ids.map((id) => job({ id })),
        missing: [],
        restricted: [],
      }),
    );
  vi.stubGlobal('fetch', fetch);
  const stops = ids.map((id) =>
    watchJob(id, () => seen.push(id), undefined, false),
  );
  try {
    await flush();
    expect(seen).toEqual([]);
    await vi.advanceTimersByTimeAsync(JOB_POLL_MS);
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(seen).toEqual(ids);
  } finally {
    stops.forEach((stop) => stop());
  }
});

test('a batch accepts canonical responses for mixed-case UUIDs', async () => {
  const id = 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee';
  const other = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
  const seen: string[] = [];
  const fetch = vi
    .fn()
    .mockResolvedValue(
      Response.json({
        jobs: [job({ id }), job({ id: other })],
        missing: [],
        restricted: [],
      }),
    );
  vi.stubGlobal('fetch', fetch);
  const stops = [id.toUpperCase(), other].map((watched) =>
    watchJob(watched, (incoming) => seen.push(incoming.id), undefined, false),
  );
  try {
    await flush();
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(seen).toEqual([id, other]);
  } finally {
    stops.forEach((stop) => stop());
  }
});

test('job changes report status and progress transitions, not repeated reads', async () => {
  const changes: (string | undefined)[] = [];
  const stop = watchJobChanges(
    [{ id: 'job-1', status: 'running', progress: 40 }],
    (current) =>
      changes.push(current && `${current.status}:${current.progress}`),
  );
  await flush();
  expect(reads).toBe(1);
  expect(changes).toEqual([]);
  const [stream] = FakeEventSource.instances;
  stream.emit('running', { status: 'running', progress: 40, attempt: 1 });
  stream.emit('progress', { status: 'running', progress: 60 });
  expect(changes).toEqual(['running:60']);
  served = job({ status: 'completed', progress: 100 });
  stream.emit('completed', {});
  await flush();
  expect(changes).toEqual(['running:60', 'completed:100']);
  stop();
});

test('job changes report a job that no longer exists', async () => {
  status = 404;
  const changes: (string | undefined)[] = [];
  const stop = watchJobChanges(
    [{ id: 'job-1', status: 'running', progress: 40 }],
    (current, id) => changes.push(current ? current.status : `missing ${id}`),
  );
  await flush();
  expect(changes).toEqual(['missing job-1']);
  stop();
});
