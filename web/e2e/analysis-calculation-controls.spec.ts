import { readFileSync } from 'node:fs';
import { expect, test } from './test';

test('imported candidates come first and position progress stays in the workspace header', async ({
  page,
}) => {
  test.setTimeout(150_000);
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
  const importedEngine = panel.locator('.candidate-metadata');
  await expect(importedEngine).toContainText(/GNU Backgammon.*0-ply/, {
    timeout: 20_000,
  });
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
  await expect(importedEngine).toContainText(/GNU Backgammon.*0-ply/);
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
  const result = page.locator('.analysis-panel .candidate-metadata');
  const start = () =>
    dialog
      .getByRole('button', { name: 'Analyze position', exact: true })
      .click();
  await open();
  await dialog.getByLabel('Search depth', { exact: true }).selectOption('0');
  await start();
  await expect(result).toContainText(/GNU Backgammon.*0-ply/);
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
  await page.route('**/api/positions/*/jobs?engine_id=*', async (route) => {
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
  await expect(result).toContainText(/GNU Backgammon.*0-ply/);
  await page.reload();
  await expect(progress).toHaveAttribute('value', '20');
  await expect(result).toContainText(/GNU Backgammon.*0-ply/);
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
