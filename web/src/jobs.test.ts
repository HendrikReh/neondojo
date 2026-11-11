import { expect, it } from 'vitest';
import { mergeJob, type Job } from './api';

const running: Job = {
  id: 'job',
  position_uuid: 'position',
  completed_games: 0,
  total_games: 0,
  kind: 'analysis',
  plies: 0,
  status: 'running',
  progress: 20,
  attempts: 1,
  created_at: '2026-11-08T11:00:00Z',
  updated_at: '2026-11-08T11:00:02Z',
};

it('late HTTP responses cannot replace a terminal result after SSE closes', () => {
  const completed: Job = {
    ...running,
    status: 'completed',
    progress: 100,
    updated_at: '2026-11-08T11:00:03Z',
  };
  expect(mergeJob(completed, running)).toBe(completed);
  expect(
    mergeJob(running, {
      ...running,
      progress: 0,
      updated_at: '2026-11-08T11:00:01Z',
    }),
  ).toBe(running);
});

it.each(['completed', 'failed'] as const)(
  'accepts an immutable %s result even when its timestamp predates the last update',
  (status) => {
    const finished: Job = {
      ...running,
      status,
      updated_at: '2026-11-08T11:00:01Z',
    };
    expect(mergeJob(running, finished)).toBe(finished);
  },
);

it('accepts a later retry state and a newly submitted job', () => {
  const retry: Job = {
    ...running,
    attempts: 2,
    status: 'queued',
    progress: 0,
    updated_at: '2026-11-08T11:03:00Z',
  };
  expect(mergeJob(running, retry)).toBe(retry);
  const next: Job = { ...running, id: 'new-job' };
  expect(mergeJob({ ...running, status: 'completed' }, next)).toBe(next);
});
