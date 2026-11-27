import { seedLocale } from '../helpers/navigation';
import { readFileSync } from 'node:fs';
import { openMatchAnalysisOptions } from '../helpers/analysis-controls';
import { expect, backendTest as test } from '../helpers/test';

test('Analysis options start a whole-match analysis and show its statistics', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'whole-match.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Whole ${crypto.randomUUID()}]`),
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  const options = page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: 'Analysis options', exact: true });
  const openDialog = async () => {
    await openMatchAnalysisOptions(page);
    await page
      .locator('.match-analysis-popover')
      .getByRole('button', { name: 'Analyze entire match …', exact: true })
      .click();
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    // The options close behind the dialog.
    await expect(page.locator('.match-analysis-popover')).toBeHidden();
    return dialog;
  };

  // Closing the dialog returns to the options button it replaced.
  let dialog = await openDialog();
  await page.screenshot({ path: testInfo.outputPath('dialog.png') });
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(options).toBeFocused();

  dialog = await openDialog();
  await dialog.getByLabel('Match analysis depth').selectOption('1');
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
  expect(job.game_no ?? null).toBeNull();
  // A completed run opens its statistics in Analysis.
  await expect(dialog).toBeHidden({ timeout: 60_000 });
  await expect(
    page.getByRole('tab', { name: 'Statistics', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  const stored = await (await page.request.get(`/api/jobs/${job.id}`)).json();
  expect(stored.status).toBe('completed');
  await page.screenshot({ path: testInfo.outputPath('statistics.png') });
});
