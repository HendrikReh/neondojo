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
