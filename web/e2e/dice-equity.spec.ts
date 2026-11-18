import { openPositionCalculation } from './analysis-controls';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/configuration-model';
import { expect, test, type Page } from './test';

// A native distribution evaluates 21 rolls. Some cases run three distributions.
const distributionTimeout = 100_000;
test.describe.configure({ timeout: 360_000 });

async function openPosition(page: Page) {
  const configuration = structuredClone(defaultConfiguration);
  configuration.analysis.plies = 1;
  const profile = await profileBackend(page, { configuration });
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await expect(
    page.getByText('GNUbg connected', { exact: true }),
  ).toBeVisible();
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'dice-equity.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `7 point match\n\n Game 1\n Dice ${crypto.randomUUID()} : 0       Opponent : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n`,
    ),
  });
  await expect(page.locator('.analysis-panel')).toBeVisible();
  return profile;
}

test('opening dice equity calculates missing results and reuses saved results across tabs, positions and reloads', async ({
  page,
}) => {
  const submissions: { position_id: string; kind: string; plies: number }[] =
    [];
  page.on('request', (request) => {
    if (
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/jobs'
    )
      submissions.push(request.postDataJSON());
  });
  const profile = await openPosition(page);
  expect(submissions).toHaveLength(0);
  await expect(
    page.getByRole('button', { name: 'Dice distribution', exact: true }),
  ).toHaveCount(0);
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(submissions).toHaveLength(1);
  expect(submissions[0]).toMatchObject({ kind: 'dice', plies: 1 });
  await page.getByRole('tab', { name: 'Candidate moves', exact: true }).click();
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36);
  expect(submissions).toHaveLength(1);
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(page.locator('.distribution-caption')).toBeVisible();
    await page.screenshot({
      path: test.info().outputPath(`dice-equity-${width}.png`),
    });
  }
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect.poll(() => submissions.length).toBe(2);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(submissions[1].position_id).not.toBe(submissions[0].position_id);
  let releaseRestore!: () => void;
  const restoreGate = new Promise<void>((resolve) => {
    releaseRestore = resolve;
  });
  let restoring = false;
  await page.route(
    `**/api/positions/${submissions[0].position_id}/jobs*`,
    async (route) => {
      restoring = true;
      await restoreGate;
      await route.continue();
    },
  );
  await page
    .getByRole('button', { name: 'Previous move', exact: true })
    .click();
  await expect.poll(() => restoring).toBe(true);
  await expect(page.locator('.dice-grid button')).toHaveCount(0);
  expect(submissions).toHaveLength(2);
  releaseRestore();
  await expect(page.locator('.dice-grid button')).toHaveCount(36);
  expect(submissions).toHaveLength(2);
  // A cached scope must become unready immediately on departure, even if
  // the next scope's restoration is interrupted before it finishes.
  let releaseSecondRestore!: () => void;
  const secondRestoreGate = new Promise<void>((resolve) => {
    releaseSecondRestore = resolve;
  });
  let secondRestoring = false;
  await page.route(
    `**/api/positions/${submissions[1].position_id}/jobs*`,
    async (route) => {
      secondRestoring = true;
      await secondRestoreGate;
      await route.continue();
    },
  );
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect.poll(() => secondRestoring).toBe(true);
  await page
    .getByRole('button', { name: 'Previous move', exact: true })
    .click();
  releaseSecondRestore();
  await expect(page.locator('.dice-grid button')).toHaveCount(36);
  expect(submissions).toHaveLength(2);
  const calculation = await openPositionCalculation(page, true);
  await calculation
    .getByLabel('Search depth', { exact: true })
    .selectOption('2');
  expect(submissions).toHaveLength(2);
  await calculation
    .getByRole('button', { name: 'Dice equity', exact: true })
    .click();
  await expect.poll(() => submissions.length).toBe(3);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(submissions[2]).toMatchObject({
    position_id: submissions[0].position_id,
    plies: 2,
  });
  const updatedProfile = structuredClone(profile.state.current_configuration);
  updatedProfile.analysis.plies = 2;
  profile.updateRemote(updatedProfile);
  await page.reload();
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36);
  expect(submissions).toHaveLength(3);
});

test('dice equity starts on the board roll and previews a selected roll on the board', async ({
  page,
}) => {
  await openPosition(page);
  await page.getByRole('tab', { name: 'Dice equity', exact: true }).click();
  const grid = page.locator('.analysis-panel .dice-grid');
  await expect(grid.getByRole('button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  // The recorded 3-1 is outlined and selected in both of its cells.
  await expect(grid.locator('button.rolled')).toHaveText([/^1 · 3/, /^3 · 1/]);
  await expect(grid.locator('button[aria-pressed="true"]')).toHaveText([
    /^1 · 3/,
    /^3 · 1/,
  ]);
  const detail = page.locator('.analysis-panel .roll-detail');
  await expect(detail).toContainText('1–3');
  await expect(detail).toContainText('Rolled');
  const board = page.locator('#analysis-board-panel svg.board');
  await expect(board).toHaveAttribute('aria-label', /dice 3 and 1/);

  await grid.getByRole('button', { name: /^5 · 5/ }).click();
  await expect(detail).toContainText('5–5');
  await expect(detail).not.toContainText('Rolled');
  const selection = page.locator(
    '#analysis-board-panel .board-review-selection',
  );
  await expect(selection).toContainText('Roll 5–5');
  await expect(board).toHaveAttribute('aria-label', /dice 5 and 5/);
  // The footer keeps the recorded move.
  await expect(page.locator('.board-navigation')).toContainText('8/5 6/5');

  await selection.getByRole('button', { name: 'Reset preview' }).click();
  await expect(selection).toHaveCount(0);
  await expect(board).toHaveAttribute('aria-label', /dice 3 and 1/);

  await grid.getByRole('button', { name: /^6 · 6/ }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(grid.getByRole('button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  await expect(selection).toHaveCount(0);
  await expect(grid.locator('button[aria-pressed="true"]')).toHaveText([
    /^2 · 4/,
    /^4 · 2/,
  ]);
  await expect(board).toHaveAttribute('aria-label', /dice 4 and 2/);
});

test('dice equity ignores a saved result from another engine', async ({
  page,
}) => {
  const submissions: { kind: string; engine_id?: string }[] = [];
  page.on('request', (request) => {
    if (
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/jobs'
    )
      submissions.push(request.postDataJSON());
  });
  await openPosition(page);
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  const grid = page.locator('.analysis-panel .dice-grid button');
  await expect(grid).toHaveCount(36, { timeout: distributionTimeout });
  expect(submissions).toEqual([
    expect.objectContaining({ kind: 'dice', engine_id: 'gnubg' }),
  ]);
  // Present the saved GNUbg result as Open Sage's once. GNUbg must not reuse it.
  const restored: string[] = [];
  page.on('request', (request) => {
    if (/\/api\/positions\/[^/]+\/jobs$/.test(new URL(request.url()).pathname))
      restored.push(request.url());
  });
  await page.route(
    '**/api/positions/*/jobs*',
    async (route) => {
      const response = await route.fetch();
      const jobs: { engine_snapshot?: Record<string, unknown> | null }[] =
        await response.json();
      await route.fulfill({
        response,
        json: jobs.map((job) => ({
          ...job,
          engine_snapshot: { ...job.engine_snapshot, engine_id: 'bgsage' },
        })),
      });
    },
    { times: 1 },
  );
  await page.reload();
  await dice.click();
  await expect.poll(() => submissions.length).toBe(2);
  expect(submissions[1]).toMatchObject({ kind: 'dice', engine_id: 'gnubg' });
  expect(restored.length).toBeGreaterThan(0);
  for (const url of restored)
    expect(new URL(url).searchParams.get('engine_id')).toBe('gnubg');
  await expect(grid).toHaveCount(36, { timeout: distributionTimeout });
});

test('reselecting dice equity retries a failed submission once', async ({
  page,
}) => {
  await openPosition(page);
  let attempts = 0;
  await page.route('**/api/jobs', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    attempts++;
    await route.fulfill({ status: 503, json: { error: 'Engine unavailable' } });
  });
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  await expect(
    page.locator('.position-inspector [role="alert"]').first(),
  ).toBeVisible();
  expect(attempts).toBe(1);
  await dice.click();
  await expect.poll(() => attempts).toBe(2);
  await expect(
    page.locator('.position-inspector [role="alert"]').first(),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Candidate moves', exact: true }).click();
  expect(attempts).toBe(2);
});

test('repeated selection while submission is delayed creates only one job', async ({
  page,
}) => {
  await openPosition(page);
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let attempts = 0;
  await page.route('**/api/jobs', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    attempts++;
    await gate;
    await route.continue();
  });
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  await expect.poll(() => attempts).toBe(1);
  await dice.click();
  await page.getByRole('tab', { name: 'Candidate moves', exact: true }).click();
  await dice.click();
  expect(attempts).toBe(1);
  release();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(attempts).toBe(1);
});

test('changing position in a hidden dice view does not submit a job', async ({
  page,
}) => {
  await openPosition(page);
  let attempts = 0;
  page.on('request', (request) => {
    if (
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/jobs'
    )
      attempts++;
  });
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  expect(attempts).toBe(1);
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  await expect(dice).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(attempts).toBe(2);
});

test('an engine failure stays visible until dice equity is reselected', async ({
  page,
}) => {
  await openPosition(page);
  let failed = true;
  let attempts = 0;
  page.on('request', (request) => {
    if (
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/jobs'
    )
      attempts++;
  });
  await page.route('**/api/jobs/*', async (route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const response = await route.fetch();
    const job = await response.json();
    if (failed && job.kind === 'dice') {
      await route.fulfill({
        response,
        json: {
          ...job,
          status: 'failed',
          result: null,
          error: 'Engine evaluation failed',
        },
      });
    } else await route.fulfill({ response });
  });
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await dice.click();
  await expect(page.locator('.analysis-panel [role="alert"]')).toContainText(
    'Analysis failed',
  );
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  await expect(page.locator('.analysis-panel [role="alert"]')).toBeVisible();
  expect(attempts).toBe(1);
  failed = false;
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: distributionTimeout,
  });
  expect(attempts).toBe(2);
});
