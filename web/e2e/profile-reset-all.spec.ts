import { backendTest as test, expect, type Page } from './test';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/configuration-model';

test.beforeEach(({ page }) => page.setDefaultTimeout(10_000));
async function setup(page: Page, locale: 'en' | 'de' = 'en') {
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = locale;
  const backend = await profileBackend(page, { configuration });
  const saved = structuredClone(backend.profiles[0]);
  const changed = structuredClone(backend.state.current_configuration);
  changed.analysis.plies = 4;
  changed.appearance.board_theme = 'walnut';
  changed.general.play_sounds = false;
  changed.library.filters.q = 'keep until confirmed';
  changed.positions.definition.tag = 'extra';
  backend.updateRemote(changed);
  await page.goto('/');
  const selector = page.getByRole('button', {
    name: locale === 'de' ? /Profil: Standard/ : /Profile: Standard/,
  });
  await selector.click();
  await page
    .getByRole('menuitem', {
      name: locale === 'de' ? 'Änderungen anzeigen' : 'Show changes',
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog', {
    name:
      locale === 'de'
        ? 'Profiländerungen: Standard'
        : 'Profile changes: Standard',
    exact: true,
  });
  return { backend, saved, changed, selector, dialog };
}
for (const { width, locale, action, confirmation, cancel } of [
  {
    width: 1440,
    locale: 'en' as const,
    action: 'Reset all changes',
    confirmation: 'Discard profile changes',
    cancel: 'Cancel',
  },
  {
    width: 390,
    locale: 'de' as const,
    action: 'Alle Änderungen zurücksetzen',
    confirmation: 'Profiländerungen verwerfen',
    cancel: 'Abbrechen',
  },
]) {
  test(`confirms and atomically resets every change without overwriting the profile ${locale}/${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    const { backend, saved, changed, selector, dialog } = await setup(
      page,
      locale,
    );
    const bulk = dialog.getByRole('button', { name: action, exact: true });
    await expect(bulk).toBeVisible();
    await bulk.click();
    const group = dialog.getByRole('group', {
      name: confirmation,
      exact: true,
    });
    await expect(group).toBeVisible();
    await expect(
      group.getByRole('button', { name: cancel, exact: true }),
    ).toBeFocused();
    await expect(dialog.locator('.profile-field-reset').first()).toBeDisabled();
    expect(backend.calls.filter((call) => call.method !== 'GET')).toEqual([]);
    expect(backend.state.current_configuration).toEqual(changed);
    await group.getByRole('button', { name: cancel, exact: true }).click();
    await expect(group).toHaveCount(0);
    await expect(bulk).toBeFocused();
    await bulk.click();
    await page.screenshot({
      path: test.info().outputPath('reset-all-confirmation.png'),
      fullPage: true,
    });
    await group.getByRole('button', { name: action, exact: true }).click();
    await expect(dialog.locator('tbody tr')).toHaveCount(0);
    await expect(group).toHaveCount(0);
    await expect(bulk).toHaveCount(0);
    await expect(dialog).toBeFocused();
    expect(backend.state.current_configuration).toEqual(saved.configuration);
    expect(backend.profiles[0]).toEqual(saved);
    const writes = backend.calls.filter((call) => call.method !== 'GET');
    expect(writes.map(({ method, path }) => ({ method, path }))).toEqual([
      { method: 'POST', path: '/configuration/restore-profile' },
    ]);
    await page.keyboard.press('Escape');
    await expect(selector).not.toContainText(
      locale === 'de' ? 'geändert' : 'modified',
    );
    await page.reload();
    await expect(selector).not.toContainText(
      locale === 'de' ? 'geändert' : 'modified',
    );
  });
}

test('Escape cancels only the confirmation before closing the overview', async ({
  page,
}) => {
  const { backend, changed, dialog, selector } = await setup(page);
  const bulk = dialog.getByRole('button', {
    name: 'Reset all changes',
    exact: true,
  });
  await bulk.click();
  await page.keyboard.press('Escape');
  await expect(dialog.getByRole('group')).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(bulk).toBeFocused();
  expect(backend.state.current_configuration).toEqual(changed);
  expect(backend.calls.filter((call) => call.method !== 'GET')).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(selector).toBeFocused();
});

test('failed bulk resets retain all changes and allow an explicit retry', async ({
  page,
}) => {
  const { backend, changed, saved, dialog } = await setup(page);
  await dialog
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  const confirm = dialog
    .getByRole('group')
    .getByRole('button', { name: 'Reset all changes', exact: true });
  backend.failNext(
    422,
    'unavailable_resources',
    'Saved book is unavailable',
    '/configuration/restore-profile',
  );
  await confirm.click();
  await expect(dialog.getByRole('alert')).toContainText(
    'Saved book is unavailable',
  );
  expect(backend.state.current_configuration).toEqual(changed);
  expect(backend.profiles[0]).toEqual(saved);
  await expect(dialog.locator('tbody tr')).toHaveCount(5);
  await confirm.click();
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await expect(dialog.locator('tbody tr')).toHaveCount(0);
  expect(backend.state.current_configuration).toEqual(saved.configuration);
});

test('a conflicting bulk reset never overwrites newer server settings', async ({
  page,
}) => {
  const { backend, changed, dialog } = await setup(page);
  await dialog
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  const remote = structuredClone(changed);
  remote.general.player_name = 'Another window';
  backend.updateRemote(remote);
  const confirm = dialog
    .getByRole('group')
    .getByRole('button', { name: 'Reset all changes', exact: true });
  await confirm.click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(confirm).toBeDisabled();
  expect(backend.state.current_configuration).toEqual(remote);
  expect(backend.calls.filter((call) => call.method !== 'GET')).toHaveLength(1);
});

test('blocks additional resets and closing while the atomic restore is pending', async ({
  page,
}) => {
  const { backend, dialog } = await setup(page);
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/configuration/restore-profile', async (route) => {
    await gate;
    await route.fallback();
  });
  await dialog
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  const group = dialog.getByRole('group');
  const pending = page.waitForRequest(
    (request) =>
      request.url().endsWith('/configuration/restore-profile') &&
      request.method() === 'POST',
  );
  await group
    .getByRole('button', { name: 'Reset all changes', exact: true })
    .click();
  await pending;
  try {
    await expect(
      group.getByRole('button', { name: 'Reset all changes', exact: true }),
    ).toBeDisabled();
    await expect(
      group.getByRole('button', { name: 'Cancel', exact: true }),
    ).toBeDisabled();
    await expect(
      dialog.getByRole('button', { name: 'Close', exact: true }),
    ).toBeDisabled();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeVisible();
  } finally {
    release();
  }
  await expect(dialog.locator('tbody tr')).toHaveCount(0);
  expect(backend.calls.filter((call) => call.method !== 'GET')).toHaveLength(1);
});
