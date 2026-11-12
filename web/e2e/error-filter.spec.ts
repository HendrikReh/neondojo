import { expect, test } from './test';

test('error lists exclude good zero-loss decisions after reset and label explicit decision drilldowns', async ({
  page,
}) => {
  test.setTimeout(45000);
  page.setDefaultTimeout(10000);
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'error-filter.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Errors ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await page
    .getByLabel('Analysis scope', { exact: true })
    .selectOption('match');
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('0');
  await page
    .getByRole('button', { name: 'Analyze entire match', exact: true })
    .click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Match analysis complete',
  );
  const panel = page.locator('.match-analysis');
  const toggle = panel.locator('.match-analysis-filters input[type=checkbox]');
  const heading = panel.locator('.match-move-overview > summary');
  const rows = panel.locator('.match-moves-table tbody tr');
  const assertErrors = async () => {
    await expect(toggle).toBeChecked();
    await expect(heading).toHaveText('Errors by move');
    await expect(rows.first()).toBeVisible();
    await expect(panel.locator('.severity-none')).toHaveCount(0);
    for (const loss of await panel.locator('.equity-loss').allTextContents())
      expect(Number(loss)).toBeGreaterThanOrEqual(0.04);
  };
  await assertErrors();
  await panel
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await assertErrors();
  await toggle.uncheck();
  await expect(heading).toHaveText('Decisions by move');
  await expect(panel.locator('.severity-none').first()).toBeVisible();
  expect(await panel.locator('.equity-loss').allTextContents()).toContain(
    '0.000',
  );
  await panel
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await assertErrors();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await page
    .locator(
      '.match-statistics .statistics-summary [data-statistics-metric="checker"]',
    )
    .first()
    .click();
  await expect(heading).toHaveText('Decisions by move');
  await expect(toggle).not.toBeChecked();
  await expect(panel.locator('.severity-none').first()).toBeVisible();
  await panel
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await assertErrors();
});
