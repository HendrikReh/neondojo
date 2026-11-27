import { backendTest as test, expect } from '../helpers/test';

test('native sessions retain their snapshot across complete profile activation', async ({
  page,
}) => {
  test.skip(
    process.env.NEONDOJO_PROFILE_NATIVE !== '1',
    'Requires the isolated profiles test backend',
  );
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile:|Profil:/ }),
  ).toBeVisible();
  const request = page.request;
  const read = async () => {
    const response = await request.get('/api/configuration');
    expect(response.ok()).toBe(true);
    return response.json();
  };
  let state = await read();
  const configuration = structuredClone(state.current_configuration);
  configuration.opening_book.provider = 'off';
  configuration.analysis.plies = 1;
  configuration.play.plies = 1;
  configuration.play.default_mode = 'coach';
  configuration.appearance.bearoff_direction = 'right';
  configuration.appearance.board_theme = 'forest';
  const initial = await request.put('/api/configuration', {
    data: {
      expected_revision: state.revision,
      configuration,
    },
  });
  expect(initial.ok(), await initial.text()).toBe(true);
  state = await initial.json();
  const suffix = `${Date.now()}`;
  const savedA = await request.post('/api/profiles', {
    data: {
      name: `Native A ${suffix}`,
      expected_state_revision: state.revision,
      configuration: state.current_configuration,
    },
  });
  expect(savedA.ok(), await savedA.text()).toBe(true);
  state = await savedA.json();
  const profileA = state.active_profile;
  const startedA = await request.post('/api/play/sessions', {
    data: {
      human_player: 0,
      match_length: 1,
    },
  });
  expect(startedA.ok(), await startedA.text()).toBe(true);
  const sessionA = await startedA.json();
  expect(sessionA.engine.plies).toBe(1);
  expect(sessionA.play_mode).toBe('coach');
  expect(sessionA.bearoff_direction).toBe('right');
  const changed = structuredClone(state.current_configuration);
  changed.play.plies = 2;
  changed.play.default_mode = 'training';
  changed.play.tutor_threshold = 0.1;
  changed.appearance.bearoff_direction = 'left';
  changed.appearance.board_theme = 'walnut';
  const savedB = await request.post('/api/profiles', {
    data: {
      name: `Native B ${suffix}`,
      expected_state_revision: state.revision,
      configuration: changed,
    },
  });
  expect(savedB.ok(), await savedB.text()).toBe(true);
  state = await savedB.json();
  const profileB = state.active_profile;
  const existingResponse = await request.get(
    `/api/play/sessions/${sessionA.id}`,
  );
  expect(existingResponse.ok()).toBe(true);
  const existing = await existingResponse.json();
  expect(existing.engine).toEqual(sessionA.engine);
  expect(existing.play_mode).toBe('coach');
  expect(existing.bearoff_direction).toBe('right');
  const startedB = await request.post('/api/play/sessions', {
    data: {
      human_player: 0,
      match_length: 1,
    },
  });
  expect(startedB.ok(), await startedB.text()).toBe(true);
  const sessionB = await startedB.json();
  expect(sessionB.engine.plies).toBe(2);
  expect(sessionB.play_mode).toBe('training');
  expect(sessionB.bearoff_direction).toBe('left');
  const activatedA = await request.post(
    `/api/profiles/${profileA.id}/activate`,
    {
      data: {
        expected_state_revision: state.revision,
        expected_target_revision: profileA.revision,
        expected_source_revision: profileB.revision,
        source_action: 'keep',
        current_configuration: state.current_configuration,
      },
    },
  );
  expect(activatedA.ok(), await activatedA.text()).toBe(true);
  state = await activatedA.json();
  expect(state.current_configuration.play.plies).toBe(1);
  expect(state.dirty).toBe(false);
  const stillB = await (
    await request.get(`/api/play/sessions/${sessionB.id}`)
  ).json();
  expect(stillB.engine).toEqual(sessionB.engine);
  expect(stillB.play_mode).toBe('training');
  expect(stillB.bearoff_direction).toBe('left');
  await page.reload();
  await expect(
    page.getByRole('button', {
      name: new RegExp(`Profile: Native A ${suffix}`),
    }),
  ).toBeVisible();
  // A single-field reset persists through the actual configuration API while
  // independently captured native sessions retain their engine snapshots.
  state = await read();
  const modified = structuredClone(state.current_configuration);
  modified.play.plies = 3;
  modified.appearance.board_theme = 'walnut';
  const applied = await request.put('/api/configuration', {
    data: { expected_revision: state.revision, configuration: modified },
  });
  expect(applied.ok(), await applied.text()).toBe(true);
  await page.reload();
  await page
    .getByRole('button', { name: new RegExp(`Profile: Native A ${suffix}`) })
    .click();
  await page
    .getByRole('menuitem', { name: 'Show changes', exact: true })
    .click();
  const changes = page.getByRole('dialog', {
    name: `Profile changes: Native A ${suffix}`,
    exact: true,
  });
  await changes
    .getByRole('button', {
      name: 'Reset Playing and training · Search depth (ply) to the saved profile value',
      exact: true,
    })
    .click();
  await expect(changes.locator('tbody tr')).toHaveCount(1);
  state = await read();
  expect(state.current_configuration.play.plies).toBe(1);
  expect(state.current_configuration.appearance.board_theme).toBe('walnut');
  expect(state.active_profile).toEqual(profileA);
  expect(
    await (await request.get(`/api/play/sessions/${sessionA.id}`)).json(),
  ).toEqual(sessionA);
  expect(
    await (await request.get(`/api/play/sessions/${sessionB.id}`)).json(),
  ).toEqual(sessionB);
  await page.keyboard.press('Escape');
  await page.reload();
  await expect(
    page.getByRole('button', {
      name: new RegExp(`Profile: Native A ${suffix}`),
    }),
  ).toContainText('modified');
  expect((await read()).current_configuration.play.plies).toBe(1);
  await page
    .getByRole('button', { name: new RegExp(`Profile: Native A ${suffix}`) })
    .click();
  await page
    .getByRole('menuitem', { name: 'Show changes', exact: true })
    .click();
  await changes
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  await changes
    .getByRole('group', { name: 'Discard profile changes', exact: true })
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  await expect(changes.locator('tbody tr')).toHaveCount(0);
  state = await read();
  expect(state.current_configuration).toEqual(profileA.configuration);
  expect(state.active_profile).toEqual(profileA);
  expect(state.dirty).toBe(false);
  expect(
    await (await request.get(`/api/play/sessions/${sessionA.id}`)).json(),
  ).toEqual(sessionA);
  expect(
    await (await request.get(`/api/play/sessions/${sessionB.id}`)).json(),
  ).toEqual(sessionB);
  await page.keyboard.press('Escape');
  await page.reload();
  await expect(
    page.getByRole('button', {
      name: new RegExp(`Profile: Native A ${suffix}`),
    }),
  ).not.toContainText('modified');
});
