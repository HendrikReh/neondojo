import { openMatchAnalysisOptions } from './analysis-controls';
import { expect, test } from './test';

test.use({ actionTimeout: 10000 });
test('single-game statistics pin errors to their source game across views, reset and navigation', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: `statistics-game-scope-${crypto.randomUUID()}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Scope ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31xufe];B[42qusu])`,
    ),
  });
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Game 2');
  await openMatchAnalysisOptions(page);
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('1');
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Game analysis complete',
  );
  await page.locator('.moves-list .move-history-row').nth(1).click();
  const before = await page.locator('.position-code code').textContent();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  const moveRequest = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname.endsWith('/review-decisions') &&
      new URL(r.url()).searchParams.get('severity') === 'very_bad',
  );
  const veryBad = page
    .locator(
      '.match-statistics .statistics-summary [data-statistics-kind="checker"][data-statistics-metric="very_bad"]',
    )
    .first();
  const expected = Number(await veryBad.locator('strong').textContent());
  expect(expected).toBeGreaterThan(0);
  await veryBad.click();
  const response = await moveRequest;
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '2',
  );
  expect(new URL(response.url()).searchParams.get('game_no')).toBe('2');
  expect(response.status()).toBe(200);
  const decisions = (await response.json()).items;
  expect(decisions.length).toBe(expected);
  expect(decisions.every((row: { game_no: number }) => row.game_no === 2)).toBe(
    true,
  );
  await expect(page.locator('.position-code code')).toHaveText(before!);
  await expect(
    page.getByLabel('Filter by severity', { exact: true }),
  ).toHaveValue('very_bad');
  await page
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(
    page.getByLabel('Filter by game', { exact: true }).locator('option'),
  ).toHaveCount(1);
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await page
    .locator(
      '.match-statistics .statistics-summary [data-statistics-metric="checker"]',
    )
    .first()
    .click();
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(page.locator('.position-code code')).toHaveText(before!);
  await page.getByLabel('Select game', { exact: true }).selectOption('1');
  await expect(page.getByRole('button', { name: 'Next decision', exact: true })).toBeDisabled();
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(
    page.getByLabel('Filter analysis by player', { exact: true }),
  ).toHaveValue('0');
  expect(
    await page.evaluate(
      async () =>
        (await (await fetch('/api/configuration')).json()).current_configuration
          .review.player,
    ),
  ).toBe('');
  await expect(page.locator('.board-heading')).toContainText('Game 2');
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          JSON.parse(localStorage.getItem('bgtutor.workspace') ?? '{}').gameNo,
      ),
    )
    .toBe(2);
  await page.reload();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '2',
  );
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await page.locator('.match-statistics .statistics-summary [data-statistics-metric="checker"]').first().click();
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue('2');
  await expect(page.getByLabel('Filter analysis by player', { exact: true })).toHaveValue('0');
});
