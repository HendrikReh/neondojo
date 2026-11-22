import { applyAndCloseSettings } from './settings-actions';
import {
  calculateDiceEquity,
  openFullMatchAnalysis,
  openPositionCalculation,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { profileBackend } from './profile-backend';
import { test, expect } from './test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('neondojo.locale'))
      localStorage.setItem('neondojo.locale', 'de');
  });
});

test('real import, search, GNUbg analysis, distribution, themes and mobile layout', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByText('GNUbg verbunden')).toBeVisible();
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'tutorial.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Tutor Demo : 0                    GNUbg Demo : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n',
    ),
  });
  await expect(
    page.getByRole('img', { name: /Backgammon-Position/ }),
  ).toBeVisible();
  await (await openPositionCalculation(page)).getByRole('button', { name: 'Position analysieren' }).click();
  const bestCandidate = page.locator(
    '.analysis-panel .candidates tbody tr[data-candidate-best="true"]',
  );
  await expect(bestCandidate).toBeVisible({ timeout: 30_000 });
  await expect(bestCandidate).toContainText('8/5 6/5');
  await calculateDiceEquity(page);
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
  await applyAndCloseSettings(settings, 'de');
  await expect(page.locator('.analysis-workspace .theme-walnut')).toBeVisible();
  await page.getByRole('button', { name: 'Nächster Zug' }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(
    page.locator('.analysis-workspace .point-number').first(),
  ).toHaveText('12');
  await page.getByRole('button', { name: 'Vorheriger Zug' }).click();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByRole('button', { name: 'Positions-ID', exact: true }).click();
  await page
    .getByRole('button', { name: 'Aktuelle Position übernehmen' })
    .click();
  await page.getByRole('button', { name: 'Exakt suchen' }).click();
  await expect(page.locator('.database-table tbody tr').first()).toBeVisible();
  await expect(page.locator('.applied-filter-chips').getByRole('button', { name: 'Positionsfilter entfernen' })).toBeVisible();
  await expect(page.locator('.position-results')).toHaveCount(0);
  await page.getByRole('tab', { name: 'Analyse', exact: true }).click();
  await page.getByRole('tab', { name: 'Spielwürfe' }).click();
  await expect(page.locator('.observed')).toContainText(
    '4 aufgezeichnete Würfe',
  );
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Matches durchsuchen').fill('Tutor Demo');
  await expect(page.locator('.match-card')).toHaveCount(1);
  await page.getByRole('tab', { name: 'Analyse', exact: true }).click();
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/neondojo-desktop.png',
    fullPage: true,
  });
  await page.reload();
  await expect(page.locator('.analysis-workspace .theme-walnut')).toBeVisible();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Matches durchsuchen').fill('Tutor Demo');
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
    path: 'test-results/neondojo-mobile.png',
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test('a late game loads directly and its full move list stops at its own boundary', async ({
  page,
}) => {
  await page.goto('/');
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('1');
  await expect(page.locator('.moves-list .move-history-row')).toHaveCount(8, {
    timeout: 15000,
  });
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('10');
  await expect(page.locator('.board-heading')).toContainText('Spiel 10');
  const moves = await page.locator('.moves-list .move-history-row').count();
  expect(moves).toBeGreaterThan(0);
  await page.locator('.moves-list .move-history-row').last().click();
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
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'match-analysis-demo.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Match Review Demo : 0             Opponent Review : 0\n 1) 31: 24/21 6/5                  42: 8/4 6/4\n\n Game 2\n Match Review Demo : 1             Opponent Review : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n',
    ),
  });
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const panel = page.getByRole('region', {
    name: 'Vollständige Matchanalyse',
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
  await submitFullMatchAnalysis(page, await openFullMatchAnalysis(page));
  const submitted = await analysisResponse;
  expect(submitted.status()).toBe(202);
  const created = await submitted.json();
  await expect(
    progress.getByText('Matchanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 45_000 });
  await expect(progress.getByRole('progressbar')).toHaveAttribute(
    'value',
    '100',
  );
  await expect(progress).toContainText('2 / 2 Spiele');
  const statistics = page.getByRole('tabpanel', {
    name: 'Statistik',
    exact: true,
  });
  await expect(statistics).toBeVisible();
  await expect(statistics).toHaveAttribute('data-job-id', created.id);
  await expect(statistics).toContainText('4 von 4 Checkerzügen analysiert');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await expect(panel).toHaveAttribute('data-job-id', created.id);
  await expect(panel.getByRole('checkbox', { name: /Nur Auffälligkeiten/ })).toBeChecked();
  await panel.getByLabel('Nur Auffälligkeiten').uncheck();
  await page.getByRole('button', { name: 'Nächste Entscheidung', exact: true }).click();
  const cubeReview = page.getByRole('region', { name: 'Cube-Bewertung', exact: true });
  await expect(cubeReview).toBeVisible();
  // Score as points still needed, cube state, all outcomes and the take point.
  await expect(cubeReview.locator('[data-cube-context]')).toContainText(
    /7-Punkte-Match · Match Review Demo [67]-away, Opponent Review [67]-away · Cube 1, in der Mitte/,
  );
  for (const outcome of ['Nicht doppeln', 'Doppeln / annehmen', 'Doppeln / passen'])
    await expect(cubeReview.locator('.cube-outcomes')).toContainText(outcome);
  await expect(cubeReview.locator('.cube-chances')).toContainText('Gammon');
  await expect(cubeReview.locator('[data-take-point]')).toContainText(
    /Take-Point bei diesem Stand: \d+,\d\u00a0% \(Kazaross-XG2/,
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await page.getByLabel('Matches durchsuchen').fill('Match Review Demo');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Match Review Demo' })
    .first()
    .click();
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await expect(
    progress.getByText('Matchanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  // Opening Cube selects the first matching decision.
  await expect(page.locator('.board-error-count')).toContainText(
    /Entscheidung 1 von \d+/,
  );
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

test('full match analysis starts and preserves conflicting local settings', async ({
  page,
}) => {
  const profile = await profileBackend(page);
  const local = structuredClone(profile.state.current_configuration);
  local.general.locale = 'de';
  local.general.player_name = 'Unsaved local name';
  const pending = {
    base_revision: profile.state.revision,
    configuration: local,
  };
  profile.updateRemote({
    ...profile.state.current_configuration,
    general: { ...local.general, player_name: 'Confirmed name' },
  });
  await page.addInitScript((draft) => {
    localStorage.setItem(
      'neondojo.configuration-pending.v1',
      JSON.stringify(draft),
    );
  }, pending);

  await page.goto('/');
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'analysis-profile-conflict.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Profile Conflict Demo : 0       Opponent : 0\n 1) 31: 8/5 6/5                  42: 8/4 6/4\n',
    ),
  });
  const dialog = await openFullMatchAnalysis(page);
  await dialog
    .getByLabel('Match-Analysetiefe', { exact: true })
    .selectOption('1');
  const submitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /\/api\/matches\/[^/]+\/analysis$/.test(new URL(response.url()).pathname),
    { timeout: 10_000 },
  );
  const [response] = await Promise.all([
    submitted,
    dialog
      .getByRole('button', { name: 'Gesamtes Match analysieren', exact: true })
      .click(),
  ]);
  expect(response.status()).toBe(202);
  expect(response.request().postDataJSON()).toMatchObject({
    engine_id: 'gnubg',
    plies: 1,
  });
  await expect(page.getByRole('tabpanel', { name: 'Statistik', exact: true }))
    .toHaveAttribute('data-job-id', (await response.json()).id, { timeout: 30_000 });
  expect(profile.calls.filter(({ method }) => method !== 'GET')).toEqual([]);
  expect(
    await page.evaluate(() =>
      JSON.parse(localStorage.getItem('neondojo.configuration-pending.v1')!),
    ),
  ).toEqual(pending);
});

test('manual checker selection wins during a delayed review response', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Matchdatei').setInputFiles('../backend/tests/fixtures/WC2014.mat');
  const dialog = await openFullMatchAnalysis(page);
  await dialog.getByLabel('Match-Analysetiefe', { exact: true }).selectOption('1');
  await submitFullMatchAnalysis(page, dialog);
  await expect(page.locator('.match-analysis-progress')).toContainText('Matchanalyse abgeschlossen');
  await page.getByRole('tab', { name: 'Statistik', exact: true }).click();
  await page.locator('.statistics-summary [data-statistics-metric="checker"]').first().click();
  const next = page.getByRole('button', { name: 'Nächste Entscheidung', exact: true });
  await expect(next).toBeEnabled();

  let release = () => {};
  let requested = () => {};
  let finished = () => {};
  const gate = new Promise<void>((resolve) => { release = resolve; });
  const started = new Promise<void>((resolve) => { requested = resolve; });
  const completed = new Promise<void>((resolve) => { finished = resolve; });
  await page.route('**/api/positions/*', async (route) => {
    if (!new URL(route.request().url()).pathname.match(/^\/api\/positions\/[^/]+$/))
      return route.continue();
    const response = await route.fetch();
    requested();
    await gate;
    await route.fulfill({ response }).catch(() => {});
    finished();
  }, { times: 1 });
  try {
    await next.click();
    await started;
    await page.locator('.analysis-workspace .move-history-row').first().click();
  } finally {
    release();
  }
  await completed;
  await expect(page.locator('.board-heading')).toContainText('Spiel 1');
  await expect(page.locator('.board-heading')).toContainText('Zug 1');
});
