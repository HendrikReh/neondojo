import { defineConfig } from '@playwright/test';

if (process.env.BGTUTOR_TEST_URL)
  throw new Error(
    'Browser tests start their own isolated BG Tutor. Unset BGTUTOR_TEST_URL.',
  );

export default defineConfig({
  testDir: './e2e',
  globalSetup: './playwright.setup.ts',
  timeout: 60_000,
  // One backend serves the whole run, and real engine work from an earlier
  // test can still occupy it. The GNUbg health probe alone may take the five
  // seconds Playwright waits by default, so assertions get twice that.
  expect: { timeout: 10_000 },
  workers: 1,
  use: {
    headless: true,
    viewport: { width: 1440, height: 1100 },
    channel: 'chrome',
  },
});
