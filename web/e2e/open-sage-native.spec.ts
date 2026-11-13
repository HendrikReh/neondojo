import { test, expect } from '@playwright/test';
test.skip(
  !process.env.BGTUTOR_SAGE_PYTHON,
  'Requires locked local Open Sage and GNUbg',
);

test('independent native engines, immutable sessions and desktop settings', async ({
  page,
}, info) => {
  test.setTimeout(120000);
  await page.goto('/');
  await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible();
  const installation = await page.request.get('/api/engines/bgsage');
  const saved = await installation.json();
  const verified = await page.request.put('/api/engines/bgsage', {
    data: { version: saved.version, path: process.env.BGTUTOR_SAGE_PYTHON },
  });
  expect(verified.status(), await verified.text()).toBe(200);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
  await expect(dialog.getByLabel('Python path', { exact: true })).toHaveValue(
    process.env.BGTUTOR_SAGE_PYTHON!,
  );
  const latest = await (await page.request.get('/api/engines/bgsage')).json();
  expect(
    (
      await page.request.put('/api/engines/bgsage', {
        data: {
          version: latest.version,
          path: process.env.BGTUTOR_SAGE_PYTHON,
        },
      })
    ).status(),
  ).toBe(200);
  await dialog
    .getByRole('button', { name: 'Verify and save', exact: true })
    .click();
  await expect(
    dialog.getByRole('button', {
      name: 'Load latest and discard draft',
      exact: true,
    }),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Load latest and discard draft', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toHaveCount(0);

  await dialog
    .getByLabel('Analysis engine', { exact: true })
    .selectOption('bgsage');
  await expect(
    dialog.getByLabel('Default analysis profile', { exact: true }),
  ).toHaveValue('2');
  await expect(
    dialog
      .getByLabel('Default analysis profile', { exact: true })
      .locator('option[value="1"]'),
  ).toBeDisabled();
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(
      dialog.getByLabel('Analysis engine', { exact: true }),
    ).toBeVisible();
    await page.screenshot({ path: info.outputPath(`settings-${width}.png`) });
  }
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await expect(
    dialog.getByLabel('Engine for future games', { exact: true }),
  ).toHaveValue('gnubg');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const firstResponse = await page.request.post('/api/play/sessions', {
    data: { human_player: 0, match_length: 1, play_mode: 'competition' },
  });
  expect(firstResponse.status(), await firstResponse.text()).toBe(201);
  const first = await firstResponse.json();
  expect(first.engine.id).toBe('gnubg');
  expect(first.engine_snapshots.analysis.engine_id).toBe('bgsage');
  const state = await (await page.request.get('/api/configuration')).json();
  state.current_configuration.analysis.engine = 'gnubg';
  state.current_configuration.analysis.plies = 0;
  state.current_configuration.play.engine = 'bgsage';
  state.current_configuration.play.plies = 2;
  const changed = await page.request.put('/api/configuration', {
    data: {
      expected_revision: state.revision,
      configuration: state.current_configuration,
    },
  });
  expect(changed.status(), await changed.text()).toBe(200);
  const secondResponse = await page.request.post('/api/play/sessions', {
    data: { human_player: 1, match_length: 0, play_mode: 'competition' },
  });
  expect(secondResponse.status(), await secondResponse.text()).toBe(201);
  const second = await secondResponse.json();
  expect(second.engine.id).toBe('bgsage');
  expect(second.engine_snapshots.analysis.engine_id).toBe('gnubg');
  const resumed = await (
    await page.request.get(`/api/play/sessions/${first.id}`)
  ).json();
  expect(resumed.engine_snapshots).toEqual(first.engine_snapshots);
});

test('completes Open Sage 4-ply for one game and the entire match', async ({
  page,
}) => {
  test.setTimeout(120_000);
  const installation = await page.request.get('/api/engines/bgsage');
  const { version } = await installation.json();
  const verified = await page.request.put('/api/engines/bgsage', {
    data: { version, path: process.env.BGTUTOR_SAGE_PYTHON },
  });
  expect(verified.status(), await verified.text()).toBe(200);
  await page.goto('/');
  const imported = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'open-sage-four-ply.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '(;GM[6]FF[4]PW[Sage Four Ply]PB[Opponent]MI[length:5][ws:0][bs:0];W[31hefe])' +
        '(;GM[6]FF[4]PW[Sage Four Ply]PB[Opponent]MI[length:5][ws:1][bs:0];W[31hefe])',
    ),
  });
  const { match } = await (await imported).json();
  const engine = page.getByLabel('Analysis engine', { exact: true });
  const depth = page.getByLabel('Match analysis depth', { exact: true });
  await engine.selectOption('bgsage');
  await expect(depth.locator('option[value="4"]')).toBeEnabled();
  await depth.selectOption('4');
  for (const [scope, gameNo, button, games] of [
    ['game', 1, 'Analyze game', 1],
    ['match', null, 'Analyze entire match', 2],
  ] as const) {
    if (scope === 'match')
      await page
        .getByLabel('Analysis scope', { exact: true })
        .selectOption(scope);
    const accepted = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname ===
          `/api/matches/${match.id}/analysis` &&
        response.request().method() === 'POST',
    );
    await page.getByRole('button', { name: button, exact: true }).click();
    const response = await accepted;
    expect(response.status(), await response.text()).toBe(202);
    const job = await response.json();
    expect(job.game_no ?? null).toBe(gameNo);
    expect(job).toMatchObject({
      plies: 4,
      total_games: games,
      engine_snapshot: { engine_id: 'bgsage', open_sage: { plies: 4 } },
    });
    await expect
      .poll(
        async () => {
          const current = await page.request.get(`/api/jobs/${job.id}`);
          return (await current.json()).status;
        },
        { timeout: 45_000 },
      )
      .toBe('completed');
    const completed = await (
      await page.request.get(`/api/jobs/${job.id}`)
    ).json();
    expect(completed).toMatchObject({
      completed_games: games,
      result: { analyzed_moves: games, plies: 4 },
    });
  }
});

test.afterEach(async ({ request }) => {
  if (!process.env.BGTUTOR_SAGE_PYTHON) return;
  const state = await (await request.get('/api/configuration')).json();
  if (!state.current_configuration) return;
  state.current_configuration.analysis.engine = 'gnubg';
  state.current_configuration.analysis.plies = 2;
  state.current_configuration.play.engine = 'gnubg';
  state.current_configuration.play.plies = 0;
  const response = await request.put('/api/configuration', {
    data: {
      expected_revision: state.revision,
      configuration: state.current_configuration,
    },
  });
  expect(response.status(), await response.text()).toBe(200);
});
