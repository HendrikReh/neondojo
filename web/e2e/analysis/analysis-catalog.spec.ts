import {
  DESKTOP_VIEWPORTS,
  openWorkspace,
  seedLocale,
} from '../helpers/navigation';
import { expect, test, type Route } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { openFullMatchAnalysis } from '../helpers/analysis-controls';
import { sql } from '../helpers/stack';

// The catalog assertions use persisted jobs and wait for the native engine.
test.describe.configure({ timeout: 240_000 });
const analysisTimeout = 100_000;

test('a selected analysis can be deleted while a deeper result remains available', async ({
  page,
}, info) => {
  await seedLocale(page);
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
  databaseUrl,
}) => {
  const profile = await profileBackend(page);
  await seedLocale(page);
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
  await openWorkspace(page, 'library');
  await openWorkspace(page, 'analysis');
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
  for (const { width, height } of DESKTOP_VIEWPORTS) {
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

  const workspace = page.locator('#workspace-panel-analysis');
  const picker = workspace.getByLabel('Select game', { exact: true });
  const colored = workspace.locator(
    '.moves-list .move-history-row[class*="move-assessment-"]',
  );
  const assessmentsPath = `/api/matches/${match.id}/move-assessments`;
  const assessmentsPattern = `**${assessmentsPath}?*`;
  let requested = () => {};
  let release = () => {};
  let delivered = () => {};
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const settled = new Promise<void>((resolve) => {
    delivered = resolve;
  });
  const delayPreviousGame = async (route: Route) => {
    const params = new URL(route.request().url()).searchParams;
    if (
      params.get('analysis_id') !== `job:${game.id}` ||
      params.get('game_no') !== '1'
    )
      return route.continue();
    const response = await route.fetch();
    requested();
    await gate;
    try {
      await route.fulfill({ response });
    } finally {
      delivered();
    }
  };
  await page.route(assessmentsPattern, delayPreviousGame);
  const statistics = workspace.getByRole('tabpanel', { name: 'Statistics' });
  try {
    await picker.selectOption('1');
    await expect(workspace.locator('.board-heading')).toContainText('Game 1');
    await selection.selectOption(`job:${game.id}`);
    await started;
    const updated = page.waitForResponse((response) => {
      const url = new URL(response.url());
      return (
        url.pathname === assessmentsPath &&
        url.searchParams.get('analysis_id') === `job:${whole.id}` &&
        url.searchParams.get('game_no') === '2'
      );
    });
    await selection.selectOption(`job:${whole.id}`);
    await picker.selectOption('2');
    const assessments = (await (await updated).json()).items;
    expect(assessments.length).toBeGreaterThan(0);
    await expect(colored).toHaveCount(assessments.length);
    await expect(
      statistics.locator('.statistics-coverage').first(),
    ).toContainText('2 of 2 games');
    release();
    await settled;
    await page.unroute(assessmentsPattern, delayPreviousGame);
    await expect(selection).toHaveValue(`job:${whole.id}`);
    await expect(colored).toHaveCount(assessments.length);
  } finally {
    release();
  }

  // Replay a committed checkpoint on the disposable database using the
  // original evaluation rows. The selected job and position stay the same.
  const db = new URL(databaseUrl);
  const stored = await (await page.request.get(`/api/jobs/${whole.id}`)).json();
  const checkpoint = await sql(
    db,
    `SELECT summary FROM match_analysis_games WHERE job_id='${whole.id}' AND game_no=2`,
  );
  expect(checkpoint).toBeTruthy();
  const json = (value: string) => `'${value.replaceAll("'", "''")}'::jsonb`;
  await sql(
    db,
    `BEGIN;
    DELETE FROM match_analysis_games WHERE job_id='${whole.id}' AND game_no=2;
    UPDATE jobs j SET status='running', completed_games=1, progress=50, lease_until=now()+interval '1 hour',
      result=(SELECT summary || jsonb_build_object('total_games',2,'total_moves',j.result->'total_moves')
              FROM match_analysis_games WHERE job_id=j.id AND game_no=1)
    WHERE j.id='${whole.id}'; COMMIT;`,
  );
  await page.reload();
  await workspace.getByRole('tab', { name: 'Statistics', exact: true }).click();
  const gameTwo = statistics.getByRole('row').filter({
    has: page.getByRole('button', { name: 'Open game 2', exact: true }),
  });
  await expect(selection).toHaveValue(`job:${whole.id}`);
  await expect(picker).toHaveValue('2');
  await expect(
    gameTwo.getByText('Not analyzed', { exact: true }),
  ).toBeVisible();
  await expect(colored).toHaveCount(0);
  const updated = page.waitForResponse(async (response) => {
    const url = new URL(response.url());
    return (
      url.pathname === `/api/matches/${match.id}/analysis-statistics` &&
      url.searchParams.get('analysis_id') === `job:${whole.id}` &&
      (await response.json()).summary?.completed_games === 2
    );
  });
  await sql(
    db,
    `BEGIN;
    INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES('${whole.id}',2,${json(checkpoint)});
    UPDATE jobs SET status='completed', completed_games=2, progress=100, lease_until=NULL,
      result=${json(JSON.stringify(stored.result))}, updated_at=now()
    WHERE id='${whole.id}'; COMMIT;`,
  );
  await updated;
  await expect(gameTwo.getByText('Analyzed', { exact: true })).toBeVisible();
  await expect(
    statistics.locator('.statistics-coverage').first(),
  ).toContainText('2 of 2 games');
  await expect(colored.first()).toBeVisible();
});
