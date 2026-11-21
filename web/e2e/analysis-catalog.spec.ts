import { expect, test } from './test';
import { profileBackend } from './profile-backend';
import { openFullMatchAnalysis } from './analysis-controls';

// The catalog assertions use persisted jobs and wait for the native engine.
test.describe.configure({ timeout: 240_000 });
const analysisTimeout = 100_000;

test('a selected analysis can be deleted while a deeper result remains available', async ({
  page,
}, info) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  const imported = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'delete-analysis.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]FF[4]PW[Delete analysis ${crypto.randomUUID()}]PB[Opponent]MI[length:21];B[63lolr])`,
    ),
  });
  const { match } = await (await imported).json();
  const saved: string[] = [];
  for (const plies of [3, 5]) {
    const response = await page.request.post(
      `/api/matches/${match.id}/analysis`,
      {
        data: { engine_id: 'gnubg', game_no: 1, plies },
      },
    );
    expect(response.status(), await response.text()).toBe(202);
    const { id } = await response.json();
    await expect
      .poll(
        async () => {
          const job = await (await page.request.get(`/api/jobs/${id}`)).json();
          return job.error ?? job.status;
        },
        { timeout: analysisTimeout },
      )
      .toBe('completed');
    saved.push(id);
  }
  const [old, deeper] = saved;
  await page.reload();
  const selection = page.getByLabel('Displayed analysis');
  await selection.selectOption(`job:${old}`);
  await expect(selection.locator('option:checked')).toContainText(
    /GNU Backgammon.*3-ply.*Game 1/,
  );
  await expect(selection.locator('option:checked')).not.toContainText(
    'Calculated',
  );
  const remove = page.getByRole('button', {
    name: 'Delete analysis',
    exact: true,
  });
  await remove.click();
  const dialog = page.getByRole('dialog', {
    name: 'Delete analysis',
    exact: true,
  });
  await expect(dialog).toContainText(/GNU Backgammon.*3-ply.*Game 1/);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(selection).toHaveValue(`job:${old}`);
  expect((await page.request.get(`/api/jobs/${old}`)).ok()).toBeTruthy();
  await remove.click();
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.screenshot({
    path: info.outputPath('confirm-analysis-deletion.png'),
  });
  await dialog
    .getByRole('button', { name: 'Delete analysis', exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(selection.locator(`option[value="job:${old}"]`)).toHaveCount(0);
  await expect(selection.locator('option:checked')).toContainText(
    /Automatic.*GNU Backgammon.*5-ply/,
  );
  expect((await page.request.get(`/api/jobs/${old}`)).status()).toBe(404);
  expect((await page.request.get(`/api/jobs/${deeper}`)).status()).toBe(200);
});

// Real persisted jobs and real catalog endpoints: selection must survive both
// navigation and a changed calculation profile without borrowing other results.
test('one saved choice drives every view and remains strict across games and reloads', async ({
  page,
}) => {
  const profile = await profileBackend(page);
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'catalog-scope.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Catalog ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1];W[31hefe];B[42qusu])`,
    ),
  });
  const response = await imported;
  expect(response.ok(), await response.text()).toBeTruthy();
  const { match } = await response.json();
  const start = async (game_no?: number) => {
    const response = await page.request.post(
      `/api/matches/${match.id}/analysis`,
      { data: { engine_id: 'gnubg', plies: 1, game_no } },
    );
    expect(response.ok(), await response.text()).toBeTruthy();
    const job = await response.json();
    await expect
      .poll(
        async () =>
          (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
        { timeout: analysisTimeout },
      )
      .toBe('completed');
    return job;
  };
  const whole = await start();
  const game = await start(1);
  await page.reload();
  const resultDialog = await openFullMatchAnalysis(page);
  await resultDialog
    .getByRole('button', { name: 'Show results', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  const selection = page.getByLabel('Displayed analysis', { exact: true });
  await expect(selection).toBeVisible();
  await expect(selection.locator('option')).toHaveCount(3);
  await selection.selectOption(`job:${game.id}`);
  await expect(
    page.locator(
      '.analysis-panel .candidate-comparison-table tr[data-candidate-best="true"]',
    ),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect(page.locator('.statistics-summary')).toBeVisible();
  const selectedStats = await (
    await page.request.get(`/api/matches/${match.id}/analysis-statistics`)
  ).json();
  expect(selectedStats.analysis_id).toBe(`job:${game.id}`);
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(selection).toHaveValue(`job:${game.id}`);
  await expect(page.locator('.analysis-panel')).toContainText(
    'This position has no result in the selected analysis.',
  );
  const marks = await (
    await page.request.get(
      `/api/matches/${match.id}/move-assessments?game_no=2`,
    )
  ).json();
  expect(marks.analysis_id).toBe(`job:${game.id}`);
  expect(marks.items).toEqual([]);
  profile.state.current_configuration.analysis.engine = 'bgsage';
  profile.state.current_configuration.analysis.plies = 2;
  await page.reload();
  await expect(selection).toHaveValue(`job:${game.id}`);
  await expect(page.locator('.analysis-panel')).toContainText(
    'This position has no result in the selected analysis.',
  );
  await selection.selectOption(`job:${whole.id}`);
  await expect(
    page.locator(
      '.analysis-panel .candidate-comparison-table tr[data-candidate-best="true"]',
    ),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await page.locator('.statistics-comparison summary').click();
  await expect(page.getByLabel('Comparison analysis 1')).toBeVisible();
  await page.getByLabel('Comparison analysis 1').selectOption(`job:${game.id}`);
  await page
    .getByLabel('Comparison analysis 2')
    .selectOption(`job:${whole.id}`);
  await expect(selection).toHaveValue(`job:${whole.id}`);
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(selection).toBeInViewport();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`catalog-${width}.png`),
    });
  }
  const catalog = await (
    await page.request.get(`/api/matches/${match.id}/analyses`)
  ).json();
  expect(catalog.items.map((entry: { id: string }) => entry.id).sort()).toEqual(
    [`job:${whole.id}`, `job:${game.id}`].sort(),
  );
});
