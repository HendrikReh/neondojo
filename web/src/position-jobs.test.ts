import { expect, it } from 'vitest';
import { positionJobsPath } from './useJobs';
it('discovers persisted jobs from the server even without browser storage', () => {
  expect(positionJobsPath('saved position')).toBe(
    '/positions/saved%20position/jobs',
  );
});
