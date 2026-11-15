import {
  calculateDiceEquity,
  openPositionCalculation,
} from './analysis-controls';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/configuration-model';
import { expect, test, type Page } from './test';

async function openPosition(page: Page) {
  const configuration = structuredClone(defaultConfiguration);
  configuration.analysis.plies = 0;
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

test('dice equity starts from the header and reuses results across tabs, positions and reloads', async ({
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
  await expect(page.locator('.analysis-panel')).toContainText(
    'No dice equity is saved',
  );
  expect(submissions).toHaveLength(0);
  await calculateDiceEquity(page);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  expect(submissions).toHaveLength(1);
  expect(submissions[0]).toMatchObject({ kind: 'dice', plies: 0 });
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
  await expect(page.locator('.analysis-panel')).toContainText(
    'No dice equity is saved',
  );
  expect(submissions).toHaveLength(1);
  await calculateDiceEquity(page);
  await expect.poll(() => submissions.length).toBe(2);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  expect(submissions[1].position_id).not.toBe(submissions[0].position_id);
  let releaseRestore!: () => void;
  const restoreGate = new Promise<void>((resolve) => {
    releaseRestore = resolve;
  });
  let restoring = false;
  await page.route(`**/api/positions/${submissions[0].position_id}/jobs`, async (route) => {
    restoring = true;
    await restoreGate;
    await route.continue();
  });
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
  await page.route(`**/api/positions/${submissions[1].position_id}/jobs`, async (route) => {
    secondRestoring = true;
    await secondRestoreGate;
    await route.continue();
  });
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
    .selectOption('1');
  expect(submissions).toHaveLength(2);
  await calculation
    .getByRole('button', { name: 'Dice equity', exact: true })
    .click();
  await expect.poll(() => submissions.length).toBe(3);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  expect(submissions[2]).toMatchObject({
    position_id: submissions[0].position_id,
    plies: 1,
  });
  const updatedProfile = structuredClone(profile.state.current_configuration);
  updatedProfile.analysis.plies = 1;
  profile.updateRemote(updatedProfile);
  await page.reload();
  await dice.click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36);
  expect(submissions).toHaveLength(3);
});

test('failed dice submissions retry only from the header', async ({ page }) => {
  await openPosition(page);
  let attempts = 0;
  await page.route('**/api/jobs', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    attempts++;
    await route.fulfill({ status: 503, json: { error: 'Engine unavailable' } });
  });
  const dice = page.getByRole('tab', { name: 'Dice equity', exact: true });
  await calculateDiceEquity(page);
  await expect(
    page.locator('.position-inspector [role="alert"]').first(),
  ).toBeVisible();
  expect(attempts).toBe(1);
  await dice.click();
  expect(attempts).toBe(1);
  await calculateDiceEquity(page);
  await expect.poll(() => attempts).toBe(2);
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
  await calculateDiceEquity(page);
  await expect.poll(() => attempts).toBe(1);
  await dice.click();
  await page.getByRole('tab', { name: 'Candidate moves', exact: true }).click();
  await dice.click();
  expect(attempts).toBe(1);
  release();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
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
  await calculateDiceEquity(page);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  expect(attempts).toBe(1);
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  await expect(page.locator('.analysis-panel')).toContainText(
    'No dice equity is saved',
  );
  expect(attempts).toBe(1);
});

test('an engine failure stays visible until a new explicit dice calculation', async ({
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
  await calculateDiceEquity(page);
  await expect(page.locator('.analysis-panel [role="alert"]')).toContainText(
    'Analysis failed',
  );
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  await expect(page.locator('.analysis-panel [role="alert"]')).toBeVisible();
  expect(attempts).toBe(1);
  failed = false;
  await calculateDiceEquity(page);
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  expect(attempts).toBe(2);
});
