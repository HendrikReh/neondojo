import { readFileSync } from 'node:fs';
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
  await expect(importedEngine).toHaveText('GNU Backgammon 1.05.002');
  await expect(importedEngine).toHaveAttribute('aria-description', /0-ply/);
  await expect(panel.getByLabel('Search depth', { exact: true })).toBeHidden();
  await expect(
    header.getByLabel('Analysis scope', { exact: true }),
  ).toBeHidden();

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

test('game and match calculation controls open separately from the job status', async ({
  page,
}) => {
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
  const scope = header.getByLabel('Analysis scope', { exact: true });
  const settings = header.getByRole('button', {
    name: 'Analyze game or match …',
    exact: true,
  });
  await expect(scope).toBeHidden();
  await settings.click();
  await expect(scope).toBeVisible();
  await header
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('0');
  await header
    .getByRole('button', { name: 'Analyze game', exact: true })
    .click();
  await expect(scope).toBeHidden();
  await expect(header.locator('.match-analysis-progress')).toContainText(
    'Game analysis completed',
  );
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect(header.locator('.match-analysis-progress')).toBeVisible();
});
