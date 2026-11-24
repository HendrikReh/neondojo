import { backendTest as test, expect, type Page } from './test';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/settings/configuration/configuration-model';

test('restores dice and range filters as explicit pairs without producing invalid intermediate values', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  Object.assign(configuration.positions.definition, {
    die1: 5,
    die2: 6,
    match_points_min: 5,
    match_points_max: 7,
    min_loss: 0.5,
    max_loss: 1,
  });
  Object.assign(configuration.library.filters, {
    match_points_min: '5',
    match_points_max: '7',
    date_from: '2026-06-01',
    date_to: '2026-06-30',
  });
  const backend = await profileBackend(page, { configuration });
  const changed = structuredClone(backend.state.current_configuration);
  Object.assign(changed.positions.definition, {
    die1: 1,
    die2: 2,
    match_points_min: 1,
    match_points_max: 3,
    min_loss: 0.1,
    max_loss: 0.2,
  });
  Object.assign(changed.library.filters, {
    match_points_min: '1',
    match_points_max: '3',
    date_from: '2026-01-01',
    date_to: '2026-01-31',
  });
  backend.updateRemote(changed);
  const dialog = await openChanges(page);
  await expect(dialog.locator('tbody tr')).toHaveCount(5);
  for (const [index, label] of [
    'Position browser · Dice',
    'Position browser · Match points range',
    'Position browser · Equity loss range',
    'Match library · Match points range',
    'Match library · Date range',
  ].entries()) {
    await reset(page, label).click();
    await expect(dialog.locator('tbody tr')).toHaveCount(4 - index);
  }
  expect(backend.state.current_configuration.positions.definition).toEqual(
    configuration.positions.definition,
  );
  expect(backend.state.current_configuration.library.filters).toEqual(
    configuration.library.filters,
  );
  await expect(dialog).toContainText('Matches the current configuration');
});

test.beforeEach(({ page }) => page.setDefaultTimeout(10_000));
async function openChanges(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitem', { name: 'Show changes', exact: true })
    .click();
  return page.getByRole('dialog', {
    name: 'Profile changes: Standard',
    exact: true,
  });
}
function reset(page: Page, field: string) {
  return page.getByRole('button', {
    name: `Reset ${field} to the saved profile value`,
    exact: true,
  });
}
for (const width of [1440, 390]) {
  test(`resets one field at a time and retains other changes and the saved profile at ${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    const backend = await profileBackend(page);
    const saved = structuredClone(backend.profiles[0]);
    const changed = structuredClone(backend.state.current_configuration);
    changed.analysis.plies = 4;
    changed.appearance.board_theme = 'walnut';
    changed.general.play_sounds = false;
    backend.updateRemote(changed);
    const dialog = await openChanges(page);
    const depth = reset(page, 'Analysis · Search depth (ply)');
    await expect(depth).toBeVisible();
    await depth.click();
    await expect(depth).toHaveCount(0);
    await expect(dialog.locator('tbody tr')).toHaveCount(2);
    expect(backend.state.current_configuration.analysis.plies).toBe(3);
    expect(backend.state.current_configuration.appearance.board_theme).toBe(
      'walnut',
    );
    expect(backend.state.current_configuration.general.play_sounds).toBe(false);
    expect(backend.profiles[0]).toEqual(saved);
    await expect(dialog).toBeFocused();
    await page.screenshot({
      path: test.info().outputPath('one-field-restored.png'),
      fullPage: true,
    });
    await reset(page, 'Appearance · Board theme').click();
    await reset(page, 'General · Game sounds').click();
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText('Matches the current configuration');
    await expect(dialog.locator('tbody tr')).toHaveCount(0);
    await page.keyboard.press('Escape');
    await expect(
      page.getByRole('button', { name: /Profile: Standard/ }),
    ).not.toContainText('modified');
    await page.reload();
    await expect(
      page.getByRole('button', { name: /Profile: Standard/ }),
    ).not.toContainText('modified');
    expect(backend.profiles[0]).toEqual(saved);
    expect(
      backend.calls
        .filter((call) => call.method !== 'GET')
        .every(
          (call) => call.path === '/configuration' && call.method === 'PUT',
        ),
    ).toBe(true);
  });
}

test('failed field reset retains all changes and can be retried explicitly', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const changed = structuredClone(backend.state.current_configuration);
  changed.analysis.plies = 4;
  changed.appearance.board_theme = 'walnut';
  backend.updateRemote(changed);
  const dialog = await openChanges(page);
  backend.failNext(
    422,
    'unavailable_resources',
    'Saved engine is unavailable',
    '/configuration',
  );
  await reset(page, 'Analysis · Search depth (ply)').click();
  await expect(dialog.getByRole('alert')).toContainText(
    'Some of these settings are not available on this computer.',
  );
  expect(backend.state.current_configuration).toEqual(changed);
  await expect(dialog.locator('tbody tr')).toHaveCount(2);
  await reset(page, 'Analysis · Search depth (ply)').click();
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await expect(dialog.locator('tbody tr')).toHaveCount(1);
  expect(backend.state.current_configuration.analysis.plies).toBe(3);
  expect(backend.state.current_configuration.appearance.board_theme).toBe(
    'walnut',
  );
});

test('stale field resets never overwrite newer remote settings', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const local = structuredClone(backend.state.current_configuration);
  local.analysis.plies = 4;
  backend.updateRemote(local);
  const dialog = await openChanges(page);
  const remote = structuredClone(local);
  remote.general.player_name = 'Another window';
  backend.updateRemote(remote);
  await reset(page, 'Analysis · Search depth (ply)').click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(reset(page, 'Analysis · Search depth (ply)')).toBeDisabled();
  expect(backend.state.current_configuration).toEqual(remote);
  const writes = backend.calls.filter((call) => call.method === 'PUT');
  expect(writes).toHaveLength(1);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('restores optional values and column membership without discarding a separate column order change', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const changed = structuredClone(backend.state.current_configuration);
  changed.positions.definition.tag = 'extra';
  changed.library.column_order.reverse();
  changed.library.visible_columns = changed.library.visible_columns.filter(
    (id) => id !== 'white',
  );
  backend.updateRemote(changed);
  const dialog = await openChanges(page);
  await reset(page, 'Position browser · Tag').click();
  expect(
    backend.state.current_configuration.positions.definition,
  ).not.toHaveProperty('tag');
  const order = [...changed.library.column_order];
  await reset(page, 'Match library · Visible columns').click();
  await expect(reset(page, 'Match library · Visible columns')).toHaveCount(0);
  expect(backend.state.current_configuration.library.column_order).toEqual(
    order,
  );
  expect(
    new Set(backend.state.current_configuration.library.visible_columns),
  ).toEqual(new Set(defaultConfiguration.library.visible_columns));
  await expect(dialog.locator('tbody tr')).toHaveCount(1);
  await reset(page, 'Match library · Column order').click();
  await expect(dialog).toContainText('Matches the current configuration');
});

test('serializes resets and keeps the dialog open while the save is pending', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const changed = structuredClone(backend.state.current_configuration);
  changed.analysis.plies = 4;
  changed.general.play_sounds = false;
  backend.updateRemote(changed);
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/configuration', async (route) => {
    if (route.request().method() === 'PUT') await gate;
    await route.fallback();
  });
  const dialog = await openChanges(page);
  const writing = page.waitForRequest(
    (request) =>
      request.url().endsWith('/api/configuration') &&
      request.method() === 'PUT',
  );
  await reset(page, 'Analysis · Search depth (ply)').click();
  await writing;
  try {
    await expect(reset(page, 'General · Game sounds')).toBeDisabled();
    await expect(
      dialog.getByRole('button', { name: 'Close', exact: true }),
    ).toBeDisabled();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeVisible();
  } finally {
    release();
  }
  await expect(reset(page, 'Analysis · Search depth (ply)')).toHaveCount(0);
  await expect(reset(page, 'General · Game sounds')).toBeEnabled();
  expect(backend.calls.filter((call) => call.method === 'PUT')).toHaveLength(1);
});

test('restoring the language from German immediately applies the saved profile language', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const changed = structuredClone(backend.state.current_configuration);
  changed.general.locale = 'de';
  changed.appearance.board_theme = 'walnut';
  backend.updateRemote(changed);
  await page.goto('/');
  await page.getByRole('button', { name: /Profil: Standard/ }).click();
  await page
    .getByRole('menuitem', { name: 'Änderungen anzeigen', exact: true })
    .click();
  await page
    .getByRole('button', {
      name: 'Allgemein · Sprache auf den gespeicherten Profilwert zurücksetzen',
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Profile changes: Standard',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('tbody tr')).toHaveCount(1);
  expect(backend.state.current_configuration.general.locale).toBe('en');
  expect(backend.state.current_configuration.appearance.board_theme).toBe(
    'walnut',
  );
});

test('distinguishes width overrides and card-generation defaults from similarly named filters', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.library.column_widths = { comment: 300 };
  const backend = await profileBackend(page, { configuration });
  const changed = structuredClone(backend.state.current_configuration);
  changed.library.column_widths = { comment: 340 };
  changed.library.filters.comment = 'Keep comment filter';
  changed.learning.filters.player = '0';
  changed.learning.generation.player = '1';
  backend.updateRemote(changed);
  const dialog = await openChanges(page);
  await reset(page, 'Match library · Column widths · Comment').click();
  expect(
    backend.state.current_configuration.library.column_widths?.comment,
  ).toBe(300);
  expect(backend.state.current_configuration.library.filters.comment).toBe(
    'Keep comment filter',
  );
  await reset(page, 'Learning cards · Card generation · Player').click();
  expect(backend.state.current_configuration.learning.generation.player).toBe(
    '',
  );
  expect(backend.state.current_configuration.learning.filters.player).toBe('0');
  await expect(dialog.locator('tbody tr')).toHaveCount(2);
});
