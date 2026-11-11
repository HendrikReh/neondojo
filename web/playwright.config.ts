import { defineConfig } from '@playwright/test';

if (process.env.BGTUTOR_TEST_URL)
  throw new Error(
    'Browser tests start their own isolated BG Tutor. Unset BGTUTOR_TEST_URL.',
  );

export default defineConfig({
  testDir: './e2e',
  globalSetup: './playwright.setup.ts',
  timeout: 60_000,
  workers: 1,
  use: {
    headless: true,
    viewport: { width: 1440, height: 1100 },
    channel: 'chrome',
  },
});
