import { test, expect } from '@playwright/test';

test('real import, search, GNUbg analysis, distribution, themes and mobile layout', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByText('GNUbg verbunden')).toBeVisible();
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'tutorial.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Tutor Demo : 0                    GNUbg Demo : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n',
    ),
  });
  await expect(
    page.getByRole('img', { name: /Backgammon-Position/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Position analysieren' }).click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.best-move')).toContainText('8/5 6/5');
  await page
    .getByRole('button', { name: 'Dice Distribution', exact: true })
    .click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  await expect(page.locator('.distribution-caption')).toContainText(
    '21 Kombinationen',
  );
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible();
  await page.getByLabel('Board-Theme').selectOption('walnut');
  await expect(page.locator('.theme-walnut')).toBeVisible();
  await page.getByRole('button', { name: 'Nächster Zug' }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(page.locator('.point-number').first()).toHaveText('12');
  await page.getByRole('button', { name: 'Vorheriger Zug' }).click();
  await page.locator('.position-search summary').click();
  await page
    .getByRole('button', { name: 'Aktuelle Position übernehmen' })
    .click();
  await page.getByRole('button', { name: 'Exakt suchen' }).click();
  await expect(page.locator('.position-results button').first()).toBeVisible();
  await page.getByRole('tab', { name: 'Partiewürfe' }).click();
  await expect(page.locator('.observed')).toContainText(
    '4 aufgezeichnete Würfe',
  );
  await page.getByLabel('Partien durchsuchen').fill('Tutor Demo');
  await expect(page.locator('.match-card')).toHaveCount(1);
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/bgtutor-desktop.png',
    fullPage: true,
  });
  await page.reload();
  await expect(page.getByLabel('Board-Theme')).toHaveValue('walnut');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Tutor Demo' })
    .first()
    .click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page.getByRole('img', { name: /Backgammon-Position/ }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: 'test-results/bgtutor-mobile.png',
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test('a long imported match loads the next move page', async ({ page }) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await expect(page.locator('.moves-list button')).toHaveCount(100);
  await page.getByRole('button', { name: 'Weiter im Verlauf' }).click();
  await expect(page.locator('.moves-list button')).toHaveCount(200);
  await expect(page.locator('.board-navigation')).toContainText('101 / 460');
});

test('full match analysis, errors, board navigation and persisted results', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'match-analysis-demo.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Match Review Demo : 0             Opponent Review : 0\n 1) 31: 24/21 6/5                  42: 8/4 6/4\n\n Game 2\n Match Review Demo : 1             Opponent Review : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n',
    ),
  });
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
  });
  await expect(panel).toBeVisible();
  const analysisResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(response.url()).pathname),
  );
  await panel
    .getByRole('button', {
      name: /Gesamte Partie analysieren|Partie erneut analysieren/,
    })
    .click();
  const submitted = await analysisResponse;
  expect(submitted.status()).toBe(202);
  const created = await submitted.json();
  await expect(panel).toHaveAttribute('data-job-id', created.id);
  await expect(
    panel.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 45_000 });
  await expect(panel.getByRole('progressbar')).toHaveAttribute('value', '100');
  await expect(panel).toContainText('2 / 2 Spiele');
  await expect(panel).toContainText('4 / 4 Züge ausgewertet');
  await panel.getByLabel('Nur Auffälligkeiten').check();
  await expect(panel.locator('tbody tr').first()).toContainText('24/21 6/5');
  await expect(panel.locator('tbody tr').first()).toContainText('8/5 6/5');
  await panel
    .getByRole('button', { name: 'Spiel 1, Zug 1 auf dem Brett öffnen' })
    .click();
  await expect(page.locator('.board-navigation')).toContainText('24/21 6/5');
  await panel
    .getByLabel('Analyse nach Entscheidung filtern')
    .selectOption('cube');
  await expect(panel.locator('tbody tr')).toHaveCount(0);
  await panel.getByLabel('Nur Auffälligkeiten').uncheck();
  await expect(panel.locator('tbody tr').first()).toContainText('Cube');
  await page.reload();
  await page.getByLabel('Partien durchsuchen').fill('Match Review Demo');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Match Review Demo' })
    .first()
    .click();
  await expect(
    panel.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  await expect(panel.locator('tbody tr').first()).toBeVisible();
  await panel.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: 'test-results/match-analysis-desktop.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: 'test-results/match-analysis-mobile.png',
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test('the latest clicked analysis row wins during position pagination', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
  });
  const analysisResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(response.url()).pathname),
  );
  await panel
    .getByRole('button', {
      name: /Gesamte Partie analysieren|Partie erneut analysieren/,
    })
    .click();
  const submitted = await analysisResponse;
  expect(submitted.status()).toBe(202);
  const created = await submitted.json();
  await expect(panel).toHaveAttribute('data-job-id', created.id);
  await expect(
    panel.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 45_000 });
  await panel
    .getByLabel('Analyse nach Entscheidung filtern')
    .selectOption('checker');
  await expect(panel.locator('tbody tr')).toHaveCount(50);
  await panel
    .getByRole('button', { name: 'Weitere Entscheidungen laden' })
    .click();
  await expect(panel.locator('tbody tr')).toHaveCount(100);
  await panel
    .getByRole('button', { name: 'Weitere Entscheidungen laden' })
    .click();
  await expect(panel.locator('tbody tr')).toHaveCount(150);
  let release: (() => void) | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  let requested: (() => void) | undefined;
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await page.route('**/api/positions?**', async (route) => {
    if (new URL(route.request().url()).searchParams.has('after')) {
      requested?.();
      await gate;
    }
    await route.continue();
  });
  const loaded = page.waitForResponse(
    (response) =>
      response.url().includes('/api/positions?') &&
      new URL(response.url()).searchParams.has('after'),
  );
  await panel.locator('tbody tr').nth(149).getByRole('button').click();
  await started;
  await panel.locator('tbody tr').first().getByRole('button').click();
  release?.();
  await (await loaded).finished();
  // Allow the delayed response's React update to render before checking its effect.
  await page.waitForTimeout(100);
  await expect(page.locator('.board-navigation')).toContainText('1 / 460');
});
