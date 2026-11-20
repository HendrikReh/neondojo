import {
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { expect, test } from './test';

test.use({ actionTimeout: 10000, viewport: { width: 1800, height: 1169 } });

for (const scope of ['match', 'game'] as const) {
  test(`typed ${scope} error cells preserve kind, severity, player and source game`, async ({
    page,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem('neondojo.locale', 'en'),
    );
    await page.goto('/');
    await page.getByLabel('Match file', { exact: true }).setInputFiles({
      name: `typed-errors-${crypto.randomUUID()}.sgf`,
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        `(;GM[6]PW[Typed ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31xufe];B[42qusu])`,
      ),
    });
    if (scope === 'game')
      await page.getByLabel('Select game', { exact: true }).selectOption('2');
    await openMatchAnalysisOptions(page);
    if (scope === 'match') {
      const dialog = await openFullMatchAnalysis(page);
      await dialog.getByLabel('Match analysis depth', { exact: true }).selectOption('1');
      await submitFullMatchAnalysis(page, dialog);
    } else {
      await page.getByLabel('Match analysis depth', { exact: true }).selectOption('1');
      await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
    }
    await expect(page.locator('.match-analysis-progress')).toContainText(
      scope === 'game' ? 'Game analysis complete' : 'Match analysis complete',
      { timeout: 30000 },
    );
    const statisticsTab = page.getByRole('tab', {
      name: 'Statistics',
      exact: true,
    });
    await statisticsTab.click();
    const stats = page.locator('.match-statistics');
    await expect(stats.locator('.statistics-error-matrix')).toHaveCount(
      scope === 'game' ? 4 : 6,
    );
    const position = await page.locator('.position-code code').textContent();
    for (const kind of ['checker', 'cube'] as const) {
      await statisticsTab.click();
      const cell = stats
        .locator(
          `.statistics-summary [data-statistics-kind="${kind}"][data-statistics-metric="very_bad"]`,
        )
        .first();
      const expected = Number(await cell.locator('strong').textContent());
      const response = page.waitForResponse((r) => {
        const url = new URL(r.url());
        return (
          url.pathname.endsWith('/review-decisions') &&
          url.searchParams.get('kind') === kind &&
          url.searchParams.get('severity') === 'very_bad'
        );
      });
      await cell.click();
      const result = await response;
      const query = new URL(result.url()).searchParams;
      expect(query.get('player')).toBe('0');
      expect(query.get('errors_only')).toBe('true');
      expect(query.get('game_no')).toBe(scope === 'game' ? '2' : null);
      expect((await result.json()).items).toHaveLength(expected);
      await expect(
        page.getByRole('tab', { name: kind === 'cube' ? 'Cube' : 'Checker', exact: true }),
      ).toHaveAttribute('aria-selected', 'true');
      await expect(
        page.getByLabel('Filter by severity', { exact: true }),
      ).toHaveValue('very_bad');
      await expect(page.locator('.position-code code')).toHaveText(position!);
    }
    if (scope === 'match') {
      await statisticsTab.click();
      const cell = stats
        .locator('.statistics-games-table tbody tr')
        .nth(1)
        .locator(
          '[data-statistics-kind="checker"][data-statistics-metric="very_bad"]',
        )
        .first();
      const response = page.waitForResponse((r) => {
        const url = new URL(r.url());
        return (
          url.pathname.endsWith('/review-decisions') &&
          url.searchParams.get('game_no') === '2' &&
          url.searchParams.get('kind') === 'checker'
        );
      });
      await cell.click();
      expect((await response).status()).toBe(200);
      await expect(
        page.getByLabel('Filter by game', { exact: true }),
      ).toHaveValue('2');
      await statisticsTab.click();
      await stats
        .locator(
          '.statistics-summary [data-statistics-metric="bad"]:not([data-statistics-kind])',
        )
        .first()
        .click();
      await stats.locator('.statistics-destinations:popover-open').getByRole('button', { name: 'Checker', exact: true }).click();
      await expect(page.getByRole('tab', { name: 'Checker', exact: true })).toHaveAttribute('aria-selected', 'true');
      await expect(
        page.getByLabel('Filter by game', { exact: true }),
      ).toHaveValue('');
    }
    await page
      .getByLabel('Filter by severity', { exact: true })
      .selectOption('none');
    await expect(
      page.getByLabel('Filter by severity', { exact: true }),
    ).toHaveValue('none');
  });
}
