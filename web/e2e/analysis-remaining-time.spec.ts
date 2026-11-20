import { expect, test, type Page } from './test';
const timing = (overrides: Record<string, unknown> = {}) => ({
  job_id: 'fixture',
  status: 'running',
  state: 'estimated',
  basis: 'benchmark',
  remaining_ms: 6000,
  total_moves: 100,
  completed_moves: 20,
  timed_moves: 20,
  measured_elapsed_ms: 2000,
  current_game_elapsed_ms: 1000,
  sampled_at: '2026-11-10T12:00:00Z',
  attempt: 1,
  ...overrides,
});
const job = (overrides: Record<string, unknown> = {}) => ({
  id: 'fixture',
  kind: 'match',
  status: 'running',
  attempts: 1,
  updated_at: '2026-11-10T12:00:00Z',
  completed_games: 1,
  ...overrides,
});
async function mount(page: Page, value = job()) {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.evaluate(async (value) => {
    const { React, createRoot } = await import('/e2e/react-fixture.ts');
    const { I18nProvider } = await import('/src/i18n.tsx');
    const { default: Remaining } =
      await import('/src/AnalysisRemainingTime.tsx');
    const original = document.getElementById('root');
    if (original) original.hidden = true;
    const host = document.createElement('main');
    host.id = 'remaining-fixture';
    host.style.cssText = 'padding:24px;max-width:700px';
    document.body.append(host);
    const root = createRoot(host);
    const render = (next: unknown) =>
      root.render(
        React.createElement(I18nProvider, {
          children: React.createElement(Remaining, { job: next }),
        }),
      );
    (window as unknown as { setRemainingJob: typeof render }).setRemainingJob =
      render;
    render(value);
  }, value);
}
async function replace(page: Page, value: Record<string, unknown>) {
  await page.evaluate(
    (value) =>
      (
        window as unknown as { setRemainingJob: (v: unknown) => void }
      ).setRemainingJob(value),
    value,
  );
}
const view = (page: Page) => page.locator('.analysis-remaining-time');

test('counts down monotonically despite browser wall clock changes and shows overrun', async ({
  page,
}) => {
  await page.clock.install();
  let reads = 0;
  await page.route('**/api/jobs/fixture/remaining-time', (route) => {
    reads++;
    return route.fulfill({ json: timing() });
  });
  await mount(page);
  await expect(view(page)).toContainText('approx. 6s');
  await expect(view(page)).toContainText('saved benchmark');
  await page.clock.setSystemTime(new Date('2100-01-01T00:00:00Z'));
  await page.clock.runFor(2000);
  await expect(view(page)).toContainText('approx. 4s');
  await page.clock.fastForward(5000);
  await expect(view(page)).toHaveAttribute(
    'data-remaining-state',
    'reestimating',
  );
  await expect(view(page)).toContainText('Recalculating time remaining');
  await expect(view(page)).not.toContainText('0s');
  expect(reads).toBe(1);
  await expect(view(page)).toHaveAttribute('aria-live', 'off');
});

test('updates from measured speed on checkpoint and preserves a server snapshot after remount', async ({
  page,
}) => {
  let response = timing({ remaining_ms: 61_000 });
  await page.route('**/api/jobs/fixture/remaining-time', (route) =>
    route.fulfill({ json: response }),
  );
  await mount(page);
  await expect(view(page)).toContainText('approx. 1m 1s');
  response = timing({
    basis: 'observed',
    remaining_ms: 20_000,
    completed_moves: 80,
    timed_moves: 80,
    measured_elapsed_ms: 8000,
  });
  await replace(
    page,
    job({ completed_games: 2, updated_at: '2026-11-10T12:01:00Z' }),
  );
  await expect(view(page)).toContainText('approx. 20s');
  await expect(view(page)).toContainText('measured analysis speed');
  await page.reload();
  await mount(page);
  await expect(view(page)).toContainText('approx. 20s');
});

test('waiting freezes the clock and retries until actual engine execution starts', async ({
  page,
}) => {
  await page.clock.install();
  let response = timing({ state: 'waiting', remaining_ms: 60_000 });
  await page.route('**/api/jobs/fixture/remaining-time', (route) =>
    route.fulfill({ json: response }),
  );
  await mount(page);
  await expect(view(page)).toContainText('Waiting for engine availability');
  await page.clock.fastForward(10_000);
  await expect(view(page)).toContainText('Waiting for engine availability');
  response = timing({ remaining_ms: 60_000 });
  await page.clock.runFor(3000);
  await expect(view(page)).toContainText('approx. 1m');
});

test('queued jobs do not fetch or count queue time, terminal jobs hide the estimate', async ({
  page,
}) => {
  let reads = 0;
  await page.route('**/api/jobs/*/remaining-time', (route) => {
    reads++;
    return route.fulfill({ json: timing() });
  });
  await mount(page, job({ status: 'queued' }));
  await expect(view(page)).toContainText('Waiting for analysis to start');
  expect(reads).toBe(0);
  await replace(page, job({ status: 'completed' }));
  await expect(view(page)).toHaveCount(0);
  await replace(page, job({ status: 'failed' }));
  await expect(view(page)).toHaveCount(0);
});

test('ignores a delayed prior job and clears old timing when an attempt changes', async ({
  page,
}) => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/jobs/fixture/remaining-time', async (route) => {
    await gate;
    await route
      .fulfill({ json: timing({ remaining_ms: 99_000 }) })
      .catch(() => {});
  });
  let response = timing({ job_id: 'next', remaining_ms: 15_000 });
  await page.route('**/api/jobs/next/remaining-time', (route) =>
    route.fulfill({ json: response }),
  );
  await mount(page);
  await expect(view(page)).toContainText('Calculating');
  await replace(page, job({ id: 'next' }));
  await expect(view(page)).toContainText('approx. 15s');
  release();
  await expect(view(page)).not.toContainText('99');
  response = timing({ job_id: 'next', attempt: 2, remaining_ms: 30_000 });
  await replace(page, job({ id: 'next', attempts: 2 }));
  await expect(view(page)).toContainText('approx. 30s');
});

for (const mode of [
  'network',
  'malformed',
  'unavailable',
  'finishing',
] as const) {
  test(`handles ${mode} optional timing without a render crash`, async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.route('**/api/jobs/fixture/remaining-time', (route) => {
      if (mode === 'network') return route.abort();
      return route.fulfill({
        json:
          mode === 'malformed'
            ? timing({ state: 'nonsense' })
            : timing({ state: mode, remaining_ms: null, basis: null }),
      });
    });
    await mount(page);
    await expect(view(page)).toContainText(
      mode === 'finishing'
        ? 'Saving analysis results'
        : 'Remaining time is not yet available',
    );
    expect(errors).toEqual([]);
  });
}
