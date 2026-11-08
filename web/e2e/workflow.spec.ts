import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('bgtutor.locale'))
      localStorage.setItem('bgtutor.locale', 'de');
  });
});

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
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  const settings = page.getByRole('dialog');
  await settings.getByRole('button', { name: 'Brett', exact: true }).click();
  await settings.getByRole('button', { name: 'Walnut', exact: true }).click();
  await settings
    .getByRole('button', { name: 'Speichern', exact: true })
    .click();
  await expect(settings).not.toBeVisible();
  await expect(page.locator('.analysis-workspace .theme-walnut')).toBeVisible();
  await page.getByRole('button', { name: 'Nächster Zug' }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(
    page.locator('.analysis-workspace .point-number').first(),
  ).toHaveText('12');
  await page.getByRole('button', { name: 'Vorheriger Zug' }).click();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.locator('.position-search summary').click();
  await page
    .getByRole('button', { name: 'Aktuelle Position übernehmen' })
    .click();
  await page.getByRole('button', { name: 'Exakt suchen' }).click();
  await expect(page.locator('.position-results button').first()).toBeVisible();
  await page.getByRole('tab', { name: 'Analyse', exact: true }).click();
  await page.getByRole('tab', { name: 'Spielwürfe' }).click();
  await expect(page.locator('.observed')).toContainText(
    '4 aufgezeichnete Würfe',
  );
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Partien durchsuchen').fill('Tutor Demo');
  await expect(page.locator('.match-card')).toHaveCount(1);
  await page.getByRole('tab', { name: 'Analyse', exact: true }).click();
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/bgtutor-desktop.png',
    fullPage: true,
  });
  await page.reload();
  await expect(page.locator('.analysis-workspace .theme-walnut')).toBeVisible();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Partien durchsuchen').fill('Tutor Demo');
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

test('a late game loads directly and its full move list stops at its own boundary', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await expect(page.locator('.moves-list button')).toHaveCount(8);
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('10');
  await expect(page.locator('.board-heading')).toContainText('Spiel 10');
  const moves = await page.locator('.moves-list button').count();
  expect(moves).toBeGreaterThan(0);
  await page.locator('.moves-list button').last().click();
  await expect(page.locator('.board-navigation')).toContainText(
    `${moves} / ${moves}`,
  );
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeDisabled();
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
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
    exact: true,
  });
  await expect(panel).toBeVisible();
  const analysisResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(response.url()).pathname),
  );
  await page
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
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Partien durchsuchen').fill('Match Review Demo');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Match Review Demo' })
    .first()
    .click();
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
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

test('the latest clicked analysis row wins during a delayed game load', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
    exact: true,
  });
  const analysisResponse = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(response.url()).pathname),
  );
  await page
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
    if (new URL(route.request().url()).searchParams.get('game_no') !== '1') {
      requested?.();
      await gate;
    }
    await route.continue();
  });

  await panel.locator('tbody tr').nth(149).getByRole('button').click();
  await started;
  await panel.locator('tbody tr').first().getByRole('button').click();
  release?.();
  // Allow the delayed response's React update to render before checking its effect.
  await page.waitForTimeout(100);
  await expect(page.locator('.board-navigation')).toContainText('1 / 8');
});
