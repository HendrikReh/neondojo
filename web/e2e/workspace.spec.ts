import { expect, test } from '@playwright/test';

test('analysis-first tabs preserve the selected game, notes and database navigation', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  const analysisTab = page.getByRole('tab', { name: 'Analyse', exact: true });
  await expect(analysisTab).toHaveAttribute('aria-selected', 'true');
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'workspace.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Workspace ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
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
  await page.getByRole('tab', { name: 'Lernen & Anki', exact: true }).click();
  await expect(
    page.getByRole('region', { name: 'Lernen & Anki', exact: true }),
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
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  const original = await (await page.request.get('/api/settings')).json();
  let dialogOpening = false,
    release = () => {},
    requested = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await page.route('**/api/settings', async (route) => {
    if (route.request().method() === 'PUT')
      return route.fulfill({
        json: {
          ...route.request().postDataJSON(),
          version: original.version + 1,
        },
      });
    if (!dialogOpening) {
      requested();
      await gate;
    }
    await route.fulfill({ json: original });
  });
  await page.goto('/');
  await started;
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.board')).toBeVisible();
  await expect(
    page.locator('.workspace-analysis-command button.primary'),
  ).toBeDisabled();
  dialogOpening = true;
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Analyse', exact: true }).click();
  await dialog
    .getByLabel('Standardprofil für Analysen', { exact: true })
    .selectOption('2');
  await dialog.getByRole('button', { name: 'Speichern', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByLabel('Partie-Analysetiefe', { exact: true }),
  ).toHaveValue('2');
  release();
  await page.unrouteAll({ behavior: 'wait' });
  // Give a late response time to reach React before asserting retained state.
  await page.waitForTimeout(100);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByLabel('Partie-Analysetiefe', { exact: true }),
  ).toHaveValue('2');
});

test('inspector tabs support arrow, Home and End navigation', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  const notes = page.getByRole('tab', { name: 'Notizen', exact: true });
  await notes.click();
  await notes.focus();
  await page.keyboard.press('Home');
  await expect(
    page.getByRole('tab', { name: 'Bewertung', exact: true }),
  ).toBeFocused();
  await expect(
    page.getByRole('tab', { name: 'Bewertung', exact: true }),
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
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.board')).toBeVisible();
  await page.route('**/api/matches/*/analysis', async (route) =>
    route.request().method() === 'POST'
      ? route.fulfill({
          status: 422,
          json: { error: 'scope-analysis-failure' },
        })
      : route.continue(),
  );
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  await page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: /^Spiel (erneut )?analysieren$/ })
    .click();
  await expect(
    page.getByRole('alert').filter({ hasText: 'scope-analysis-failure' }),
  ).toHaveCount(1);
});

test('accepted position jobs remain visible after a position switch during submission', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.board')).toBeVisible();
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
  await page
    .getByRole('button', { name: 'Position analysieren', exact: true })
    .click();
  await started;
  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  release();
  await expect(
    page.getByRole('button', { name: 'Zur Analyse zurückkehren', exact: true }),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Zur Analyse zurückkehren', exact: true }),
  ).toBeVisible();
});

test('accepted game jobs remain tracked when the game changes during submission', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'job-switch.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Queued ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe])`,
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
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
  release();
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await expect(
    page.getByRole('button', { name: 'Zur Analyse zurückkehren', exact: true }),
  ).toBeVisible();
});

test('next position error respects the player filter', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'filtered-errors.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Filtered ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42aesu])`,
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  await page
    .locator('.workspace-analysis-command')
    .getByRole('button', { name: /^Spiel (erneut )?analysieren$/ })
    .click();
  await expect(
    page.getByText('Spielanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  await page
    .getByLabel('Analyse nach Spieler filtern', { exact: true })
    .selectOption('1');
  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  await page
    .getByRole('button', { name: 'Nächster Fehler', exact: false })
    .click();
  await expect(page.locator('.moves-panel [role=status]')).toBeVisible();
  await expect(page.locator('.board-heading')).toContainText('Zug 2');
  await page
    .getByLabel('Analyse nach Spieler filtern', { exact: true })
    .selectOption('0');
  await page
    .getByRole('button', { name: 'Nächster Fehler', exact: false })
    .click();
  await expect(page.locator('.board-heading')).toContainText('Zug 1');
});

test('the last game and exact move restore without starting an analysis', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('8');
  await expect(page.locator('.board-heading')).toContainText('Spiel 8');
  await page.locator('.moves-list button').nth(5).click();
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
