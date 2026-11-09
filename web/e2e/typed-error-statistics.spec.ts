import { expect, test } from '@playwright/test';

test.use({ actionTimeout: 10000, viewport: { width: 1800, height: 1169 } });

for (const scope of ['match', 'game'] as const) {
  test(`typed ${scope} error cells preserve kind, severity, player and source game`, async ({
    page,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem('bgtutor.locale', 'en'),
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
    await page
      .getByLabel('Analysis scope', { exact: true })
      .selectOption(scope);
    await page
      .getByLabel('Match analysis depth', { exact: true })
      .selectOption('0');
    await page
      .getByRole('button', {
        name: scope === 'game' ? 'Analyze game' : 'Analyze entire match',
        exact: true,
      })
      .click();
    await expect(page.locator('.match-analysis-progress')).toContainText(
      scope === 'game' ? 'Game analysis complete' : 'Match analysis complete',
    );
    const stats = page.locator('.overview-statistics');
    await expect(stats.locator('.statistics-error-matrix')).toHaveCount(
      scope === 'game' ? 4 : 6,
    );
    const position = await page.locator('.position-code code').textContent();
    for (const kind of ['checker', 'cube'] as const) {
      const cell = stats
        .locator(
          `.statistics-summary [data-statistics-kind="${kind}"][data-statistics-metric="very_bad"]`,
        )
        .first();
      const expected = Number(await cell.locator('strong').textContent());
      const response = page.waitForResponse((r) => {
        const url = new URL(r.url());
        return (
          url.pathname.endsWith('/moves') &&
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
        page.getByLabel('Filter analysis by decision', { exact: true }),
      ).toHaveValue(kind);
      await expect(
        page.getByLabel('Filter by severity', { exact: true }),
      ).toHaveValue('very_bad');
      await expect(page.locator('.position-code code')).toHaveText(position!);
    }
    if (scope === 'match') {
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
          url.pathname.endsWith('/moves') &&
          url.searchParams.get('game_no') === '2' &&
          url.searchParams.get('kind') === 'checker'
        );
      });
      await cell.click();
      expect((await response).status()).toBe(200);
      await expect(
        page.getByLabel('Filter by game', { exact: true }),
      ).toHaveValue('2');
      await stats
        .locator(
          '.statistics-summary [data-statistics-metric="bad"]:not([data-statistics-kind])',
        )
        .first()
        .click();
      await expect(
        page.getByLabel('Filter analysis by decision', { exact: true }),
      ).toHaveValue('');
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
