import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import type { Job } from '../src/api';
import { expect, test } from './test';

test('imported candidates come first and position progress stays in the workspace header', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'position-controls.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Controls ${crypto.randomUUID()}]`),
    ),
  });
  const header = page.locator('.analysis-commandbar');
  const panel = page.locator('.analysis-panel');
  const importedEngine = panel.locator(
    '.candidate-metadata .engine-label-details',
  );
  await expect(importedEngine).toHaveText('GNU Backgammon 1.05.002', {
    timeout: 30_000,
  });
  await expect(importedEngine).toHaveAttribute('aria-description', /0-ply/);
  await expect(panel.getByLabel('Search depth', { exact: true })).toBeHidden();
  await expect(
    page.getByLabel('Displayed analysis', { exact: true }),
  ).toBeVisible();

  const recalculate = panel.getByRole('button', {
    name: 'Recalculate position …',
    exact: true,
  });
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(recalculate).toBeInViewport();
    await expect(panel.locator('.candidate-metadata')).toBeInViewport();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: test.info().outputPath(`analysis-results-${width}.png`),
    });
  }
  await page.setViewportSize({ width: 1800, height: 1169 });
  await recalculate.click();
  const dialog = page.getByRole('dialog', {
    name: 'Recalculate position',
    exact: true,
  });
  await expect(
    dialog.getByLabel('Search depth', { exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(recalculate).toBeFocused();
  let submitted = 0;
  let job: Record<string, unknown>;
  await page.route('**/api/jobs', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    const input = route.request().postDataJSON();
    submitted++;
    job = {
      id: 'pending-position-result',
      kind: 'analysis',
      position_uuid: input.position_id,
      plies: input.plies,
      progress: 25,
      status: 'running',
      updated_at: '2026-11-13T10:00:00Z',
    };
    return route.fulfill({ status: 202, json: job });
  });
  await page.route('**/api/jobs/pending-position-result', (route) =>
    route.fulfill({ json: job }),
  );
  await page.route('**/api/jobs/pending-position-result/events', (route) =>
    route.fulfill({ contentType: 'text/event-stream', body: '' }),
  );

  await recalculate.click();
  expect(submitted).toBe(0);
  await dialog.getByLabel('Search depth', { exact: true }).selectOption('1');
  await dialog
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  await expect(dialog).toBeHidden();
  await expect(
    header.getByRole('progressbar', { name: 'Analysis progress' }),
  ).toHaveAttribute('value', '25');
  await expect(importedEngine).toHaveAttribute('aria-description', /0-ply/);
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(
    header.getByRole('progressbar', { name: 'Analysis progress' }),
  ).toHaveAttribute('value', '25');
  expect(submitted).toBe(1);
});

test('the previous engine result survives a pending rerun and position restoration', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'rerun-controls.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Rerun ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
    ),
  });
  const open = () =>
    page
      .getByRole('button', { name: 'Recalculate position …', exact: true })
      .click();
  const dialog = page.getByRole('dialog', {
    name: 'Recalculate position',
    exact: true,
  });
  const result = page.locator(
    '.analysis-panel .candidate-metadata .engine-label-details',
  );
  const start = () =>
    dialog
      .getByRole('button', { name: 'Analyze position', exact: true })
      .click();
  await open();
  await dialog.getByLabel('Search depth', { exact: true }).selectOption('0');
  await start();
  await expect(result).toHaveText(/^GNU Backgammon [\d.]+$/);
  await expect(result).toHaveAttribute('aria-description', /0-ply/);
  let nextJob: Record<string, unknown>;
  await page.route('**/api/jobs', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    const input = route.request().postDataJSON();
    nextJob = {
      id: 'pending-rerun',
      kind: 'analysis',
      position_uuid: input.position_id,
      plies: input.plies,
      progress: 20,
      status: 'running',
      updated_at: '2026-11-13T10:00:00Z',
    };
    return route.fulfill({ status: 202, json: nextJob });
  });
  await page.route('**/api/positions/*/jobs', async (route) => {
    const response = await route.fetch();
    const jobs = await response.json();
    await route.fulfill({ json: nextJob ? [nextJob, ...jobs] : jobs });
  });
  await page.route('**/api/jobs/pending-rerun', (route) =>
    route.fulfill({ json: nextJob }),
  );
  await page.route('**/api/jobs/pending-rerun/events', (route) =>
    route.fulfill({ contentType: 'text/event-stream', body: '' }),
  );
  await open();
  await dialog.getByLabel('Search depth', { exact: true }).selectOption('1');
  await start();
  const progress = page
    .locator('.analysis-commandbar')
    .getByRole('progressbar', { name: 'Analysis progress' });
  await expect(progress).toHaveAttribute('value', '20');
  await expect(result).toHaveAttribute('aria-description', /0-ply/);
  await page.reload();
  await expect(progress).toHaveAttribute('value', '20');
  await expect(result).toHaveAttribute('aria-description', /0-ply/);
});

test('game and match settings close on start and progress stays in the command bar', async ({
  page,
}) => {
  test.setTimeout(120_000);
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'match-controls.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Controls ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
    ),
  });
  const header = page.locator('.analysis-commandbar');
  const settings = header.getByRole('button', {
    name: 'Analyze game or match …',
    exact: true,
  });
  const scope = header.getByLabel('Analysis scope', { exact: true });
  const engine = header.getByLabel('Analysis engine', { exact: true });
  const depth = header.getByLabel('Match analysis depth', { exact: true });
  await expect(settings).toHaveAttribute('aria-expanded', 'false');
  await expect(scope).toBeHidden();

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(settings).toBeInViewport();
    await settings.click();
    await expect(scope).toBeInViewport();
    await expect(engine).toBeInViewport();
    await expect(depth).toBeInViewport();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await settings.click();
  }
  await page.setViewportSize({ width: 1800, height: 1169 });
  const progress = header.locator('.match-analysis-progress');
  for (const [value, label, completed] of [
    ['game', 'Analyze game', 'Game analysis completed'],
    ['match', 'Analyze entire match', 'Match analysis complete'],
  ]) {
    await settings.click();
    await scope.selectOption(value);
    await depth.selectOption('0');
    await header.getByRole('button', { name: label, exact: true }).click();
    await expect(settings).toBeFocused();
    await expect(settings).toHaveAttribute('aria-expanded', 'false');
    await expect(scope).toBeHidden();
    await expect(progress).toContainText(completed, { timeout: 25_000 });
  }
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect(progress).toBeVisible();
});

test('reload resumes native bulk match analysis without submitting new jobs', async ({
  page,
}) => {
  test.setTimeout(180_000);
  const testDatabase = process.env.BGTUTOR_E2E_DATABASE_URL;
  if (!testDatabase)
    throw new Error('Playwright did not provision a test database');

  const player = `Bulk ${crypto.randomUUID()}`;
  const plies = 2;
  const matchIds: string[] = [];
  const fixtures = [
    {
      name: 'bulk-short.sgf',
      buffer: Buffer.from(
        `(;GM[6]PW[${player}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
      ),
    },
    {
      name: 'bulk-WC2014.sgf',
      buffer: Buffer.from(
        readFileSync(
          '../backend/tests/fixtures/WC2014.gnubg.sgf',
          'utf8',
        ).replaceAll('PW[Douglas Mayfield]', `PW[${player}]`),
      ),
    },
  ];
  for (const fixture of fixtures) {
    const imported = await page.request.post('/api/imports', {
      multipart: {
        file: { ...fixture, mimeType: 'application/octet-stream' },
      },
    });
    expect(imported.status(), await imported.text()).toBe(201);
    matchIds.push((await imported.json()).match.id);
  }
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.getByRole('textbox', { name: 'Search matches' }).fill(player);
  const rows = page.locator('.database-table tbody tr');
  await expect(rows).toHaveCount(2);
  for (const id of matchIds) {
    await rows
      .and(page.locator(`[data-match-id="${id}"]`))
      .getByRole('checkbox')
      .check();
  }
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Analyze selected matches …', exact: true })
    .click();
  const dialog = page.getByRole('dialog', { name: 'Analyze selected matches' });
  await dialog
    .getByLabel('Analysis engine', { exact: true })
    .selectOption('gnubg');
  await dialog
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption(String(plies));
  let posts = 0;
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (
      request.method() === 'POST' &&
      /^\/api\/matches\/[^/]+\/analysis$/.test(path)
    )
      posts++;
  });
  const submitted = matchIds.map((id) =>
    page.waitForResponse(
      (response) =>
        response.request().method() === 'POST' &&
        new URL(response.url()).pathname === `/api/matches/${id}/analysis`,
    ),
  );
  await dialog
    .getByRole('button', { name: 'Analyze selected matches', exact: true })
    .click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Analysis jobs accepted (2)',
  );
  const jobs: Job[] = [];
  for (const response of await Promise.all(submitted)) {
    expect(response.status(), await response.text()).toBe(202);
    jobs.push(await response.json());
  }
  const [shortJob, longJob] = jobs;
  expect(jobs.map((job) => job.match_uuid)).toEqual(matchIds);
  for (const job of jobs) {
    expect(job.kind).toBe('match');
    expect(job.game_no).toBeNull();
    expect(job.plies).toBe(plies);
    expect(job.engine_snapshot?.engine_id).toBe('gnubg');
  }
  expect(longJob.total_games).toBe(10);
  const readJob = async (id: string): Promise<Job> => {
    const response = await page.request.get(`/api/jobs/${id}`);
    expect(response.ok(), await response.text()).toBe(true);
    return response.json();
  };

  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  const database = page.getByRole('tabpanel', { name: 'Match database' });
  const progress = database
    .getByRole('status')
    .filter({ hasText: /matches analyzed/ });
  await expect(progress).toHaveText('1 of 2 matches analyzed', {
    timeout: 30_000,
  });
  expect((await readJob(shortJob.id)).status).toBe('completed');
  await expect
    .poll(async () => (await readJob(longJob.id)).status)
    .toBe('running');

  await page.reload();
  await expect(database).toBeVisible();
  await expect(progress).toHaveText('1 of 2 matches analyzed');
  expect((await readJob(longJob.id)).status).toBe('running');
  expect(posts).toBe(2);

  await expect(progress).toHaveText('2 of 2 matches analyzed', {
    timeout: 120_000,
  });
  expect((await readJob(longJob.id)).completed_games).toBe(longJob.total_games);
  expect(posts).toBe(2);
  const persisted = spawnSync(
    'psql',
    [
      testDatabase,
      '-XAt',
      '-v',
      'ON_ERROR_STOP=1',
      '-v',
      `short_match=${matchIds[0]}`,
      '-v',
      `long_match=${matchIds[1]}`,
    ],
    {
      encoding: 'utf8',
      input:
        "SELECT id FROM jobs WHERE match_uuid IN (:'short_match', :'long_match');",
    },
  );
  expect(
    persisted.status,
    'Could not read the jobs from the test database',
  ).toBe(0);
  const persistedIds = persisted.stdout.trim().split('\n');
  expect(persistedIds.sort()).toEqual(jobs.map((job) => job.id).sort());
});
