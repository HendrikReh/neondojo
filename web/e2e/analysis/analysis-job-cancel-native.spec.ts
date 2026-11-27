import { seedLocale } from '../helpers/navigation';
import { readFileSync } from 'node:fs';
import {
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
} from '../helpers/analysis-controls';
import { expect, backendTest as test } from '../helpers/test';

test('a running match analysis can be cancelled from its progress', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await seedLocale(page);
  await page.goto('/');
  const imported = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/imports' &&
      response.request().method() === 'POST',
  );
  // A complete game keeps a 5-ply GNUbg analysis busy long enough.
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'cancel.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Cancel ${crypto.randomUUID()}]`),
    ),
  });
  await imported;
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  const dialog = await openFullMatchAnalysis(page);
  await dialog.getByLabel('Match analysis depth').selectOption('5');
  const submitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /^\/api\/matches\/[^/]+\/analysis$/.test(
        new URL(response.url()).pathname,
      ),
  );
  await dialog
    .getByRole('button', { name: 'Analyze entire match', exact: true })
    .click();
  const job = await (await submitted).json();
  // The dialog shows progress while the analysis runs.
  const cancel = dialog.getByRole('button', { name: 'Cancel analysis' });
  await expect(cancel).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('running-dialog.png') });
  await cancel.click();
  await expect(dialog).toContainText('Match analysis cancelled');
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  const stored = await (await page.request.get(`/api/jobs/${job.id}`)).json();
  expect(stored.status).toBe('failed');
  expect(stored.error).toBe('Cancelled by user');
  await page.screenshot({ path: testInfo.outputPath('cancelled-dialog.png') });
});

test('a running game analysis can be cancelled from the Analysis header', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'cancel-game.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Header ${crypto.randomUUID()}]`),
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await openMatchAnalysisOptions(page);
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('5');
  const header = page.locator('.analysis-commandbar');
  await header
    .getByRole('button', { name: 'Analyze game', exact: true })
    .click();
  // The displayed analysis stays the imported one, so the new game
  // analysis runs in the header's background job list.
  const running = header.locator('.analysis-running-job');
  const cancel = running.getByRole('button', { name: 'Cancel analysis' });
  await expect(cancel).toBeVisible({ timeout: 30_000 });
  await page.screenshot({ path: testInfo.outputPath('running-header.png') });
  const cancelled = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /^\/api\/jobs\/[^/]+\/cancel$/.test(new URL(response.url()).pathname),
  );
  await cancel.click();
  const job = await (await cancelled).json();
  expect(job.error).toBe('Cancelled by user');
  await expect(running).toHaveCount(0);
  await expect(header.getByRole('alert')).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('cancelled-header.png') });
});
