import { seedLocale } from '../helpers/navigation';
import {
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
  submitFullMatchAnalysis,
} from '../helpers/analysis-controls';
import { expect, test } from '../helpers/test';

test('error review restores the configured threshold and navigates all decisions on request', async ({
  page,
}) => {
  test.setTimeout(45000);
  page.setDefaultTimeout(10000);
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'error-filter.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Errors ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await openMatchAnalysisOptions(page);
  const dialog = await openFullMatchAnalysis(page);
  await dialog
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('1');
  await submitFullMatchAnalysis(page, dialog);
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Match analysis complete',
  );
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  const pending = (errorsOnly: string, kind: string) =>
    page.waitForResponse((response) => {
      const url = new URL(response.url());
      return (
        url.pathname.endsWith('/review-decisions') &&
        url.searchParams.has('page') &&
        url.searchParams.get('errors_only') === errorsOnly &&
        url.searchParams.get('kind') === kind
      );
    });

  const errors = pending('true', 'cube');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  expect((await errors).ok()).toBe(true);
  const panel = page.getByRole('tabpanel', { name: 'Cube', exact: true });
  const toggle = panel.getByRole('checkbox', { name: /Inaccuracies only/ });
  await expect(toggle).toBeChecked();
  await expect(panel.getByLabel('Filter analysis by decision')).toHaveCount(0);
  const all = pending('false', 'cube');
  await toggle.uncheck();
  const results = await (await all).json();
  expect(results.items.length).toBeGreaterThan(0);
  expect(
    results.items.every((row: { kind: string }) => row.kind === 'cube'),
  ).toBe(true);
  await page
    .getByRole('button', { name: 'Next decision', exact: true })
    .click();
  await expect(
    page.getByRole('region', { name: 'Cube evaluation', exact: true }),
  ).toBeVisible();

  const statistics = page.getByRole('tab', { name: 'Statistics', exact: true });
  await statistics.click();
  const checker = pending('false', 'checker');
  await page
    .locator(
      '.match-statistics .statistics-summary [data-statistics-metric="checker"]',
    )
    .first()
    .click();
  const checked = await checker;
  expect(new URL(checked.url()).searchParams.get('game_no')).toBeNull();
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.getByRole('group', { name: 'Review errors' }),
  ).toContainText('All games in this analysis');

  await statistics.click();
  const cube = pending('false', 'cube');
  await page
    .locator(
      '.match-statistics .statistics-summary [data-statistics-metric="cube"]',
    )
    .first()
    .click();
  expect((await cube).ok()).toBe(true);
  await expect(
    page.getByRole('tab', { name: 'Cube', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(toggle).not.toBeChecked();
  await panel
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await expect(toggle).toBeChecked();
});
