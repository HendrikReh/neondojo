import { openMatchCalculation, openPositionCalculation } from './analysis-controls';
import { test, expect } from './test';

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
  await (
    await openPositionCalculation(page)
  )
    .getByRole('button', { name: 'Position analysieren' })
    .click();
  const bestCandidate = page.locator(
    '.analysis-panel .candidates tbody tr[data-candidate-best="true"]',
  );
  await expect(bestCandidate).toBeVisible({ timeout: 30_000 });
  await expect(bestCandidate).toContainText('8/5 6/5');
  await page.getByRole('tab', { name: 'Würfel-Equity', exact: true }).click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  await expect(page.locator('.distribution-caption')).toContainText(
    '21 Kombinationen',
  );
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await expect(bestCandidate).toBeVisible();
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  const settings = page.getByRole('dialog');
  await settings.getByRole('button', { name: 'Brett', exact: true }).click();
  await settings.getByRole('button', { name: 'Walnut', exact: true }).click();
  await settings.getByRole('button', { name: 'Anwenden', exact: true }).click();
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
  await expect(page.locator('.database-table tbody tr').first()).toBeVisible();
  await expect(page.locator('.library-position-filter')).toBeVisible();
  await expect(page.locator('.position-results')).toHaveCount(0);
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
  await expect(bestCandidate).toBeVisible();
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
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('1');
  await expect(page.locator('.moves-list button')).toHaveCount(8, {
    timeout: 15000,
  });
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
  await openMatchCalculation(page);
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
    exact: true,
  });
  const progress = page
    .getByRole('tabpanel', { name: 'Analyse', exact: true })
    .locator('.match-analysis-progress');
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
    progress.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 45_000 });
  await expect(progress.getByRole('progressbar')).toHaveAttribute(
    'value',
    '100',
  );
  await expect(progress).toContainText('2 / 2 Spiele');
  await page.getByRole('tab', { name: 'Statistik', exact: true }).click();
  const statistics = page.getByRole('tabpanel', {
    name: 'Statistik',
    exact: true,
  });
  await expect(statistics).toBeVisible();
  await expect(statistics).toHaveAttribute('data-job-id', created.id);
  await expect(statistics).toContainText('4 von 4 Checkerzügen analysiert');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
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
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  await expect(
    progress.getByText('Partieanalyse abgeschlossen', { exact: true }),
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

test('the latest clicked analysis row wins during a delayed position request', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  await openMatchCalculation(page);
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page
    .getByLabel('Partie-Analysetiefe', { exact: true })
    .selectOption('0');
  await openMatchCalculation(page);
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const panel = page.getByRole('region', {
    name: 'Vollständige Partieanalyse',
    exact: true,
  });
  const progress = page
    .getByRole('tabpanel', { name: 'Analyse', exact: true })
    .locator('.match-analysis-progress');
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
  await expect
    .poll(
      async () => (await (await page.request.get(`/api/jobs/${created.id}`)).json()).status,
      { timeout: 45_000 },
    )
    .toBe('completed');
  await page
    .getByLabel('Angezeigte Analyse', { exact: true })
    .selectOption(`job:${created.id}`);
  await expect(panel).toHaveAttribute('data-job-id', created.id);
  await expect(
    progress.getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 45_000 });
  await panel.getByLabel('Nur Auffälligkeiten').uncheck();
  await panel
    .getByLabel('Analyse nach Entscheidung filtern')
    .selectOption('checker');
  await expect(panel.locator('tbody tr')).toHaveCount(50);
  await panel.getByRole('button', { name: 'Nächste Seite' }).click();
  await expect(
    panel.getByRole('button', { name: 'Seite 2', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(panel.locator('tbody tr')).toHaveCount(50);
  await panel.getByRole('button', { name: 'Nächste Seite' }).click();
  await expect(
    panel.getByRole('button', { name: 'Seite 3', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(panel.locator('tbody tr')).toHaveCount(50);
  let release = () => {}, requested = () => {}, finished = () => {};
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const started = new Promise<void>((resolve) => { requested = resolve; });
  const completed = new Promise<void>((resolve) => { finished = resolve; });
  await page.route(/\/api\/positions\/[^/?]+$/, async (route) => {
    const response = await route.fetch();
    requested();
    await gate;
    await route.fulfill({ response }).catch(() => {});
    finished();
  }, { times: 1 });

  try {
    await panel.locator('tbody tr').last().getByRole('button').click();
    await started;
    await panel.getByRole('button', { name: 'Anfang', exact: true }).click();
    await expect(
      panel.getByRole('button', { name: 'Seite 1', exact: true }),
    ).toHaveAttribute('aria-current', 'page');
    await panel.locator('tbody tr').first().getByRole('button').click();
    await expect(page.locator('.board-navigation')).toContainText('1 / 8');
  } finally {
    release();
  }
  await completed;
  await page.evaluate(() => new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  await expect(page.locator('.board-navigation')).toContainText('1 / 8');
});
