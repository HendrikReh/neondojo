import { seedLocale } from '../helpers/navigation';
import { mockSettings, settingsFixture } from '../helpers/api-fixtures';
import { applyAndCloseSettings } from '../helpers/settings-actions';
import {
  openMatchAnalysisOptions,
  openPositionCalculation,
} from '../helpers/analysis-controls';
import { expect, test } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';

test('workspace tabs preserve the selected game, notes and database navigation', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Startseite', exact: true }),
  ).toBeVisible();
  const analysisTab = page.getByRole('tab', { name: 'Analyse', exact: true });
  await analysisTab.click();
  await expect(analysisTab).toHaveAttribute('aria-selected', 'true');
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'workspace.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Workspace ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await expect(page.locator('.analysis-workspace .moves-list')).toBeVisible();
  await expect(page.locator('.analysis-workspace .match-list')).toHaveCount(0);
  await page.getByRole('tab', { name: 'Notizen', exact: true }).click();
  const comment = page
    .locator('.annotation-panel')
    .getByLabel('Kommentar', { exact: true });
  await expect(comment).toBeEnabled();
  await comment.fill('Retained workspace draft');
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await expect(page.locator('.match-database')).toBeVisible();
  await analysisTab.click();
  await expect(page.getByLabel('Spiel auswählen', { exact: true })).toHaveValue(
    '2',
  );
  await expect(comment).toHaveValue('Retained workspace draft');
  await page.getByRole('tab', { name: 'Lernen', exact: true }).click();
  await expect(
    page.getByRole('tabpanel', { name: 'Übersicht', exact: true }),
  ).toBeVisible();
  await analysisTab.click();
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await expect(comment).toHaveValue('Retained workspace draft');
});

test('a delayed initial settings response cannot replace a newer saved profile', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = 'de';
  configuration.analysis.plies = 2;
  const profiles = await profileBackend(page, { configuration });
  const original = settingsFixture(configuration);
  let dialogOpening = false,
    release = () => {},
    requested = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await mockSettings(page, async ({ respond }) => {
    if (!dialogOpening) {
      requested();
      await gate;
    }
    // The first startup read can be aborted and replayed by StrictMode.
    await respond(original).catch(() => {});
  });
  await page.goto('/');
  await started;
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  // Match analysis waits for the engine catalog, not for the held settings.
  await expect(
    page.locator('.workspace-analysis-command button.primary'),
  ).toBeEnabled();
  dialogOpening = true;
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Analyse', exact: true }).click();
  await dialog
    .getByLabel('Standardprofil für Analysen', { exact: true })
    .selectOption('3');
  await applyAndCloseSettings(dialog, 'de');
  await openMatchAnalysisOptions(page);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByLabel('Match-Analysetiefe', { exact: true }),
  ).toHaveValue('3');
  expect(profiles.state.current_configuration.analysis.plies).toBe(3);
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByLabel('Match-Analysetiefe', { exact: true }),
  ).toHaveValue('3');
});

test('inspector tabs support arrow, Home and End navigation', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  const notes = page.getByRole('tab', { name: 'Notizen', exact: true });
  await notes.click();
  await notes.focus();
  await page.keyboard.press('Home');
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toBeFocused();
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(
    page.getByRole('tab', { name: 'Lernkarten', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(notes).toBeFocused();
});

test('an analysis submission error appears once in the errors view', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.route('**/api/matches/*/analysis', async (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          status: 422,
          json: { error: 'scope-analysis-failure' },
        })
      : route.continue(),
  );
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: /^Spiel (erneut )?analysieren$/ })
    .click();
  await expect(
    page.getByRole('alert').filter({ hasText: 'scope-analysis-failure' }),
  ).toHaveCount(1);
});

test('accepted position jobs remain tracked without a duplicate banner after a position switch', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  let release = () => {},
    accepted = () => {},
    job: Record<string, unknown> | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    accepted = resolve;
  });
  await page.route('**/api/jobs', async (route) => {
    const response = await route.fetch();
    job = await response.json();
    accepted();
    await gate;
    await route.fulfill({ json: job });
  });
  await page.route(/\/api\/jobs\/[^/]+$/, async (route) => {
    if (job && route.request().url().endsWith(`/${job.id}`))
      return route.fulfill({
        json: { ...job, status: 'running', progress: 42 },
      });
    await route.continue();
  });
  await (await openPositionCalculation(page))
    .getByRole('button', { name: 'Position analysieren', exact: true })
    .click();
  await started;
  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  const tracked = () =>
    page.waitForRequest(
      (request) => new URL(request.url()).pathname === `/api/jobs/${job!.id}`,
    );
  const firstPoll = tracked();
  release();
  await firstPoll;
  await expect(page.locator('.background-job-indicator')).toHaveCount(0);
  const backgroundPoll = tracked();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await backgroundPoll;
  await expect(page.locator('.background-job-indicator')).toHaveCount(0);
});

test('accepted game jobs remain tracked when the game changes during submission', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'job-switch.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Queued ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  let release = () => {},
    accepted = () => {},
    job: Record<string, unknown> | undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    accepted = resolve;
  });
  await page.route('**/api/matches/*/analysis', async (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    const response = await route.fetch();
    job = await response.json();
    accepted();
    await gate;
    await route.fulfill({ json: job });
  });
  await page.route(/\/api\/jobs\/[^/]+$/, async (route) => {
    if (job && route.request().url().endsWith(`/${job.id}`))
      return route.fulfill({
        json: { ...job, status: 'running', progress: 42 },
      });
    await route.continue();
  });
  await page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: /^Spiel (erneut )?analysieren$/ })
    .click();
  await started;
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('2');
  const tracked = page.waitForRequest(
    (request) => new URL(request.url()).pathname === `/api/jobs/${job!.id}`,
  );
  release();
  await tracked;
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await expect(page.locator('.background-job-indicator')).toHaveCount(0);
});

test('next position error respects the player filter', async ({ page }) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'filtered-errors.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Filtered ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42aeac])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: /^Spiel (erneut )?analysieren$/ })
    .click();
  await expect(
    page.getByText('Spielanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Statistik', exact: true }).click();
  await page.locator('.statistics-summary [data-statistics-metric="inaccuracies"][data-statistics-kind="checker"]').first().click();
  const player = page.getByLabel('Analyse nach Spieler filtern', {
    exact: true,
  });
  const review = page.getByRole('group', {
    name: 'Fehler durchgehen',
    exact: true,
  });
  const nextError = review.getByRole('button', {
    name: 'Nächster Fehler',
    exact: true,
  });
  await expect(page.locator('.board-heading')).toContainText('Zug 1');
  await player.selectOption('1');
  await expect(review.getByRole('status')).toHaveText('Fehler: 1');
  await expect(nextError).toBeEnabled();
  await nextError.click();
  await expect(page.locator('.board-heading')).toContainText('Zug 2');
  await expect(review.getByRole('status')).toHaveText('Fehler 1 von 1');
  await player.selectOption('0');
  await expect(review.getByRole('status')).toHaveText('Fehler: 1');
  await expect(nextError).toBeEnabled();
  await nextError.click();
  await expect(page.locator('.board-heading')).toContainText('Zug 1');
});

test('the last game and exact move restore without starting an analysis', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('8');
  await expect(page.locator('.board-heading')).toContainText('Spiel 8');
  await page.locator('.moves-list .move-history-row').nth(5).click();
  const position = await page
    .locator('[data-annotation-kind=position]')
    .getAttribute('data-annotation-id');
  let submitted = 0;
  page.on('request', (r) => {
    if (
      r.method() === 'POST' &&
      (/\/api\/jobs$/.test(r.url()) || /\/analysis$/.test(r.url()))
    )
      submitted++;
  });
  await page.reload();
  await expect(page.locator('[data-annotation-kind=position]')).toHaveAttribute(
    'data-annotation-id',
    position!,
  );
  await expect(page.getByLabel('Spiel auswählen', { exact: true })).toHaveValue(
    '8',
  );
  await expect(page.locator('.board-navigation')).toContainText('6 / 83');
  expect(submitted).toBe(0);
});
