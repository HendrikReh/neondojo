import { expect, test, type Page } from './test';
const reference = {
  id: 'wc2014-v1',
  version: 1,
  name: 'WC2014 – Yazawa / Mayfield',
  sha256: 'abc123',
  match_length: 11,
  total_games: 10,
  total_moves: 460,
  protocol: { id: 'gnubg-match-v2-unlimited', game_timeout_seconds: null },
};
const settings = {
  version: 1,
  player_name: 'Player',
  analysis_engine: 'gnubg',
  analysis_plies: 2,
  play_engine: 'gnubg',
  play_plies: 1,
  gnubg_path: '/usr/bin/gnubg',
};
const run = (id: string, status = 'completed', plies = 0) => ({
  id,
  status,
  plies,
  created_at: '2026-11-10T12:00:00Z',
  started_at: '2026-11-10T12:01:00Z',
  finished_at: status === 'completed' ? '2026-11-10T12:02:00Z' : null,
  machine: {
    id: 'local',
    name: 'Backend Mac',
    os: 'macOS',
    architecture: 'aarch64',
    cpu_model: 'Apple M4',
    logical_cpus: 10,
    memory_bytes: 17179869184,
  },
  engine_id: 'gnubg',
  engine_name: 'GNU Backgammon',
  engine_version: 'GNUbg 1.08',
  engine_path: '/usr/bin/gnubg',
  reference,
  protocol: reference.protocol,
  total_elapsed_ms: status === 'completed' ? 60000 : null,
  average_move_ms: status === 'completed' ? 60000 / 460 : null,
  completed_games: status === 'completed' ? 10 : 3,
  analyzed_moves: status === 'completed' ? 460 : 130,
  error: status === 'failed' ? 'Engine version changed' : null,
});
async function backend(page: Page) {
  page.on('pageerror', (error) => {
    throw error;
  });
  let runs = [run('completed'), run('failed', 'failed')];
  const starts: unknown[] = [],
    cancels: string[] = [],
    writes: unknown[] = [];
  let reads = 0;
  await page.addInitScript(() => {
    localStorage.clear();
    localStorage.setItem('bgtutor.locale', 'en');
  });
  await page.route('**/api/**', (route) =>
    /\/api\/(?:configuration|profiles|opening-book)(?:\/|$)/.test(
      new URL(route.request().url()).pathname,
    )
      ? route.fallback()
      : route.fulfill({ json: [] }),
  );
  await page.route('**/api/library/matches?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null, total: 0 } }),
  );
  await page.route('**/api/positions?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null, total: 0 } }),
  );
  await page.route('**/api/cards?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null, total: 0 } }),
  );
  await page.route('**/api/health', (route) =>
    route.fulfill({ json: { status: 'ok' } }),
  );
  await page.route('**/api/settings', (route) => {
    if (route.request().method() !== 'GET')
      writes.push(route.request().postDataJSON());
    return route.fulfill({ json: settings });
  });
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      json: [
        {
          id: 'gnubg',
          name: 'GNU Backgammon',
          available: true,
          version: 'GNUbg 1.08',
          plies: [0, 1, 2, 3, 4],
        },
        {
          id: 'bgsage',
          name: 'BGSage',
          available: false,
          version: null,
          plies: [],
        },
      ],
    }),
  );
  await page.route('**/api/benchmarks/status', (route) =>
    route.fulfill({
      json: {
        active: runs.some((r) => ['queued', 'running'].includes(r.status)),
      },
    }),
  );
  await page.route('**/api/benchmarks/reference', (route) =>
    route.fulfill({ json: reference }),
  );
  await page.route('**/api/benchmarks', (route) => {
    if (route.request().method() === 'POST') {
      const body = route.request().postDataJSON();
      starts.push(body);
      const created = body.plies.map((ply: number) =>
        run(`new-${ply}`, ply === body.plies[0] ? 'running' : 'queued', ply),
      );
      runs = [...created, ...runs];
      return route.fulfill({ status: 202, json: created });
    }
    reads++;
    return route.fulfill({
      json: {
        runs,
        active: runs.some((r) => ['queued', 'running'].includes(r.status)),
      },
    });
  });
  await page.route('**/api/benchmarks/*/cancel', (route) => {
    const id = route.request().url().split('/').at(-2)!;
    cancels.push(id);
    runs = runs.map((r) => (r.id === id ? { ...r, status: 'cancelled' } : r));
    return route.fulfill({ json: runs.find((r) => r.id === id) });
  });
  await page.goto('/');
  return { starts, cancels, writes, reads: () => reads };
}
async function open(page: Page) {
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Engine benchmark' }),
  ).toBeVisible();
  await expect(page.getByLabel('Benchmark engine')).toHaveValue('gnubg');
}
test('play-turn latency starts independently and has its own history metric', async ({
  page,
}, testInfo) => {
  const api = await backend(page);
  await page.setViewportSize({ width: 1800, height: 1169 });
  let playStarts = 0;
  const playRun = {
    ...run('play-completed', 'completed', settings.play_plies),
    reference: null,
    protocol: null,
    play_reference: {
      id: 'wc2014-checker-turns-v1',
      protocol_id: 'gnubg-checker-turn-v1',
      sha256: 'abc',
      total_turns: 11,
    },
    completed_turns: 11,
    completed_games: 0,
    analyzed_moves: 0,
    average_move_ms: null,
    total_elapsed_ms: 462,
    average_turn_ms: 42,
  };
  await page.route('**/api/benchmarks/play', (route) => {
    expect(route.request().method()).toBe('POST');
    expect(route.request().postDataJSON()).toEqual({
      engine_id: 'gnubg',
      plies: 1,
    });
    playStarts++;
    return route.fulfill({ status: 202, json: playRun });
  });
  await page.route('**/api/benchmarks', (route) =>
    route.request().method() === 'GET'
      ? route.fulfill({
          json: {
            active: false,
            runs: playStarts ? [playRun, run('analysis')] : [run('analysis')],
          },
        })
      : route.fallback(),
  );
  await open(page);
  await page.getByRole('checkbox', { name: '2-ply', exact: true }).uncheck();
  await page.getByRole('button', { name: 'Measure play-turn latency' }).click();
  const row = page
    .getByRole('row')
    .filter({ hasText: 'wc2014-checker-turns-v1' });
  await expect(row).toContainText('1-ply');
  await expect(row).toContainText('11 / 11 checker turns');
  await expect(row).toContainText('42 ms');
  await expect(row.locator('[data-benchmark-time]')).toHaveText(['—', '—']);
  expect(playStarts).toBe(1);
  expect(api.starts).toHaveLength(0);
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(row.locator('.benchmark-latency')).toBeInViewport();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: testInfo.outputPath(`play-latency-${width}.png`),
    });
  }
});
test('Extras shows reference, saved history and only the saved default ply', async ({
  page,
}) => {
  const api = await backend(page);
  await open(page);
  await expect(
    page.getByRole('checkbox', { name: '2-ply', exact: true }),
  ).toBeChecked();
  for (const ply of [0, 1, 3, 4])
    await expect(
      page.getByRole('checkbox', { name: `${ply}-ply`, exact: true }),
    ).not.toBeChecked();
  await expect(
    page.getByLabel('Benchmark engine').locator('option[value=bgsage]'),
  ).toBeDisabled();
  await expect(page.locator('.benchmark-reference')).toContainText(
    '11 points · 10 games · 460 checker moves',
  );
  await expect(page.locator('.benchmark-reference')).toContainText(
    'No time limit',
  );
  const rows = page.locator('.benchmark-history tbody tr');
  await expect(rows).toHaveCount(2);
  for (const text of [
    'Backend Mac',
    'Apple M4',
    '16 GiB',
    'GNUbg 1.08',
    '60,000 ms',
    '130.43 ms',
  ])
    await expect(rows.first()).toContainText(text);
  await expect(rows.nth(1)).toContainText('Engine version changed');
  await expect(rows.nth(1).locator('[data-benchmark-time]')).toHaveText([
    '—',
    '—',
  ]);
  expect(api.starts).toHaveLength(0);
  await page.screenshot({
    path: '/tmp/bgtutor-benchmark-desktop.png',
    fullPage: true,
  });
  await page.getByRole('checkbox', { name: '2-ply', exact: true }).uncheck();
  await expect(
    page.getByRole('button', { name: 'Start benchmark', exact: true }),
  ).toBeDisabled();
});
test('multiple plies queue, cancel explicitly and keep running across navigation', async ({
  page,
}) => {
  const api = await backend(page);
  await open(page);
  await page.getByRole('checkbox', { name: '0-ply', exact: true }).check();
  await page
    .getByRole('button', { name: 'Start benchmark', exact: true })
    .click();
  await expect
    .poll(() => api.starts)
    .toEqual([{ engine_id: 'gnubg', plies: [0, 2] }]);
  const active = page.locator('.benchmark-active');
  await expect(active).toContainText('3 / 10 games');
  await expect(active).toContainText('Waiting for engine availability');
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(
    page
      .getByRole('status')
      .filter({ hasText: 'Benchmark running or waiting.' }),
  ).toBeVisible();
  const hiddenReads = api.reads();
  await page.waitForTimeout(1300);
  expect(api.reads()).toBe(hiddenReads);
  expect(api.cancels).toEqual([]);
  await open(page);
  await active
    .getByRole('button', { name: 'Cancel run', exact: true })
    .first()
    .click();
  await expect.poll(() => api.cancels).toEqual(['new-0']);
  await expect(page.locator('.benchmark-history')).toContainText('Cancelled');
});
test('Settings benchmark link leaves drafts unapplied and opens the same view', async ({
  page,
}) => {
  const api = await backend(page);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByLabel('Player name', { exact: true }).fill('Unsaved');
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog
    .getByLabel('Default analysis profile', { exact: true })
    .selectOption('4');
  await dialog
    .getByRole('button', { name: 'Open benchmark', exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  await expect(
    page.getByRole('tab', { name: 'Extras', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.getByRole('checkbox', { name: '2-ply', exact: true }),
  ).toBeChecked();
  await expect(
    page.getByRole('checkbox', { name: '4-ply', exact: true }),
  ).not.toBeChecked();
  expect(api.writes).toEqual([]);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    'Player',
  );
});

test('active history stays usable while engine catalog is blocked, then refreshes engines', async ({
  page,
}) => {
  await backend(page);
  let active = true,
    catalogReads = 0;
  await page.route('**/api/engines', (route) => {
    catalogReads++;
    return route.fulfill({
      json: [
        {
          id: 'gnubg',
          name: 'GNU Backgammon',
          available: !active,
          version: active ? null : 'GNUbg 1.08',
          plies: [0, 1, 2, 3, 4],
        },
      ],
    });
  });
  await page.route('**/api/benchmarks', (route) =>
    route.fulfill({
      json: {
        active,
        runs: [
          {
            ...run('existing', active ? 'running' : 'completed'),
            started_at: active ? null : '2026-11-10T12:01:00Z',
          },
        ],
      },
    }),
  );
  await openWithoutEngine();
  async function openWithoutEngine() {
    await page.getByRole('tab', { name: 'Extras', exact: true }).click();
    await expect(page.locator('.benchmark-active')).toContainText(
      'Waiting for engine availability',
    );
  }
  await expect(
    page.getByRole('button', { name: 'Start benchmark', exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByText(
      'Other engine work waits until the benchmark queue finishes.',
      { exact: true },
    ),
  ).toBeVisible();
  active = false;
  await expect(
    page.getByRole('button', { name: 'Start benchmark', exact: true }),
  ).toBeEnabled();
  expect(catalogReads).toBeGreaterThan(1);
});

test('failed starts show backend diagnostics and narrow view keeps actions reachable', async ({
  page,
}) => {
  await backend(page);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.route('**/api/benchmarks', (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          status: 409,
          json: { error: 'Engine work is already queued' },
        })
      : route.fulfill({ json: { active: false, runs: [] } }),
  );
  await open(page);
  await page
    .getByRole('button', { name: 'Start benchmark', exact: true })
    .click();
  await expect(page.getByRole('alert')).toContainText(
    'Engine work is already queued',
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  const alertBox = await page.getByRole('alert').boundingBox();
  const retryBox = await page
    .getByRole('alert')
    .getByRole('button', { name: 'Try again' })
    .boundingBox();
  expect(retryBox!.y + retryBox!.height).toBeLessThanOrEqual(
    alertBox!.y + alertBox!.height,
  );
  await page.screenshot({
    path: '/tmp/bgtutor-benchmark-narrow.png',
    fullPage: true,
  });
});

test('Extras tabs support keyboard navigation and remember the last selection', async ({
  page,
}) => {
  await page.goto('/');
  const extras = page.getByRole('tab', { name: 'Extras', exact: true });
  await extras.click();
  const tools = page.getByRole('tablist', { name: 'Extra tools', exact: true });
  const benchmark = tools.getByRole('tab', { name: 'Benchmark', exact: true });
  const versions = tools.getByRole('tab', { name: 'Versions', exact: true });
  const backup = tools.getByRole('tab', {
    name: 'Database backup',
    exact: true,
  });
  await benchmark.focus();
  await benchmark.press('End');
  await expect(versions).toBeFocused();
  await versions.press('ArrowLeft');
  await expect(backup).toHaveAttribute('aria-selected', 'true');
  await page.reload();
  await extras.click();
  await expect(backup).toHaveAttribute('aria-selected', 'true');
  await backup.press('Home');
  await expect(benchmark).toBeFocused();
});
