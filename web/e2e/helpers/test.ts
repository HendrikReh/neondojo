import '../../src/shared/i18n/test-locales';
import { test as playwright } from '@playwright/test';
import { profileBackend } from './profile-backend';
import { startStack, type SpecStack, type Stack } from './stack';

export * from '@playwright/test';

// Every spec file owns a disposable database and backend. Its tests run in
// order on one worker and share that database, so a test may see data of an
// earlier test in the same file. After a failed test Playwright continues the
// file on a new worker with an empty database, so do not rely on that data.
export const backendTest = playwright.extend<
  { specStack: SpecStack; databaseUrl: string },
  { workerStack: Stack }
>({
  workerStack: [
    async ({}, use, workerInfo) => {
      const stack = await startStack(workerInfo.parallelIndex);
      try {
        await use(stack);
      } finally {
        await stack.stop();
      }
    },
    { scope: 'worker', timeout: 120_000 },
  ],
  specStack: [
    async ({ workerStack }, use, testInfo) => {
      const spec = await workerStack.forSpec(testInfo.file);
      // Specs read the database URL for direct SQL checks and fixtures.
      process.env.NEONDOJO_E2E_DATABASE_URL = spec.databaseUrl;
      await use(spec);
    },
    { auto: true, timeout: 120_000 },
  ],
  baseURL: async ({ specStack }, use) => use(specStack.url),
  databaseUrl: async ({ specStack }, use) => use(specStack.databaseUrl),
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
