import { test as playwright } from '@playwright/test';
import { profileBackend } from './profile-backend';
import { startStack, type Stack } from './stack';

export * from '@playwright/test';

// Every worker owns a disposable database, backend, and Vite server. Tests in
// one spec file run in order on the same worker and may build on each other's
// data; tests in different files never share persisted state.
export const backendTest = playwright.extend<
  { databaseUrl: string },
  { stack: Stack }
>({
  stack: [
    async ({}, use, workerInfo) => {
      const stack = await startStack(workerInfo.parallelIndex);
      // Specs read the database URL for direct SQL checks and fixtures.
      process.env.NEONDOJO_E2E_DATABASE_URL = stack.databaseUrl;
      try {
        await use(stack);
      } finally {
        await stack.stop();
      }
    },
    { scope: 'worker', auto: true, timeout: 120_000 },
  ],
  baseURL: async ({ stack }, use) => use(stack.url),
  databaseUrl: async ({ stack }, use) => use(stack.databaseUrl),
});

// Existing functional suites exercise their real/mocked domain APIs while
// keeping the now installations-wide personal configuration isolated per test.
// Native profile integration uses backendTest and the disposable backend.
export const test = backendTest.extend({
  page: async ({ page }, use) => {
    await profileBackend(page, { initialized: false });
    await use(page);
  },
});
