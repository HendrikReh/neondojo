import { defineConfig } from '@playwright/test';

if (process.env.NEONDOJO_TEST_URL)
  throw new Error(
    'Browser tests start their own isolated NeonDojo. Unset NEONDOJO_TEST_URL.',
  );

// Each worker starts its own database, backend, and GNUbg processes, so the
// default stays below the core count. Override with --workers or
// NEONDOJO_E2E_WORKERS.
const workers = Number(process.env.NEONDOJO_E2E_WORKERS ?? 4);
if (!Number.isInteger(workers) || workers < 1)
  throw new Error('NEONDOJO_E2E_WORKERS must be a positive integer.');

export default defineConfig({
  testDir: './e2e',
  globalSetup: './playwright.setup.ts',
  timeout: 60_000,
  // Real engine work from an earlier test can still occupy a worker's backend.
  // The GNUbg health probe alone may take the five seconds Playwright waits by
  // default, so assertions get twice that.
  expect: { timeout: 10_000 },
  workers,
  forbidOnly: !!process.env.CI,
  use: {
    headless: true,
    viewport: { width: 1440, height: 1100 },
    channel: 'chrome',
  },
});
