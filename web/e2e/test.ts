import { test as base } from '@playwright/test';
import { profileBackend } from './profile-backend';

export * from '@playwright/test';

// Existing functional suites exercise their real/mocked domain APIs while
// keeping the now installations-wide personal configuration isolated per test.
// Native profile integration uses the base fixture and the disposable backend.
export const test = base.extend({
  page: async ({ page }, use) => {
    await profileBackend(page, { initialized: false });
    await use(page);
  },
});
