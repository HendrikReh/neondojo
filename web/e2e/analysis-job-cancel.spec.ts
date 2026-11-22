import type { Job } from '../src/shared/api/api';
import { expect, test, type Page } from './test';

const job = (id: string, overrides: Partial<Job> = {}): Job => ({
  id,
  kind: 'match',
  status: 'running',
  attempts: 1,
  created_at: '2026-11-10T11:59:00Z',
  updated_at: '2026-11-10T12:00:00Z',
  plies: 3,
  progress: 40,
  completed_games: 1,
  total_games: 3,
  ...overrides,
});

async function mount(page: Page, jobs: Job[]) {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.evaluate(async (jobs) => {
    const { React, createRoot } = await import('/e2e/react-fixture.ts');
    const { I18nProvider } = await import('/src/shared/i18n/i18n.tsx');
    const { default: RunningJobs } =
      await import('/src/AnalysisRunningJobs.tsx');
    const original = document.getElementById('root');
    if (original) original.hidden = true;
    const host = document.createElement('main');
    host.style.cssText = 'padding:24px;max-width:700px';
    document.body.append(host);
    createRoot(host).render(
      React.createElement(I18nProvider, {
        children: React.createElement(RunningJobs, { jobs }),
      }),
    );
  }, jobs);
}

test('background analyses can be cancelled individually', async ({ page }) => {
  const cancelled: string[] = [];
  await page.route('**/api/jobs/*/cancel', (route) => {
    const id = route.request().url().split('/').at(-2)!;
    cancelled.push(`${route.request().method()} ${id}`);
    if (id === 'finished')
      return route.fulfill({
        status: 409,
        json: { error: 'The job has already finished' },
      });
    return route.fulfill({
      json: job(id, { status: 'failed', error: 'Cancelled by user' }),
    });
  });
  await mount(page, [
    job('running'),
    job('finished', { kind: 'dice', status: 'queued', progress: 0 }),
  ]);
  const rows = page.locator('.analysis-running-job');
  await expect(rows).toHaveCount(2);
  const cancel = (index: number) =>
    rows.nth(index).getByRole('button', { name: 'Cancel analysis' });
  await cancel(0).click();
  await expect.poll(() => cancelled).toEqual(['POST running']);
  await expect(page.getByRole('alert')).toHaveCount(0);
  await cancel(1).click();
  await expect.poll(() => cancelled).toEqual(['POST running', 'POST finished']);
  await expect(page.getByRole('alert')).toContainText(
    'The job has already finished',
  );
});
