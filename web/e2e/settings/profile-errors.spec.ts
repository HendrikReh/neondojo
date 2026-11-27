import { backendTest as test, expect } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { configurationJournalKey } from '../../src/settings/configuration/configuration-store';

test('profile recovery shows one localized conflict alert with both actions', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const remote = structuredClone(backend.state.current_configuration);
  remote.general.locale = 'de';
  const pending = structuredClone(remote);
  pending.general.player_name = 'Lokaler Entwurf';
  backend.updateRemote(remote);
  await page.addInitScript(
    ({ key, pending }) => {
      localStorage.setItem(
        key,
        JSON.stringify({ base_revision: 1, configuration: pending }),
      );
    },
    { key: configurationJournalKey, pending },
  );

  await page.goto('/');
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  const settings = page.getByRole('dialog', {
    name: 'Einstellungen',
    exact: true,
  });
  await settings.getByRole('button', { name: 'Profile', exact: true }).click();
  const alert = settings.getByRole('alert');
  await expect(alert).toHaveCount(1);
  await expect(alert).toContainText(
    'Die Einstellungen wurden in einem anderen Fenster geändert. Lokale Änderungen bleiben erhalten.',
  );
  await expect(
    alert.getByRole('button', { name: 'Serverstand laden', exact: true }),
  ).toBeEnabled();
  await expect(
    alert.getByRole('button', {
      name: 'Lokale Änderungen als neues Profil sichern',
      exact: true,
    }),
  ).toBeEnabled();
});

test('reports unavailable target resources without changing active appearance', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.route('**/api/profiles/*/activate', (route) =>
    route.fulfill({
      status: 422,
      json: {
        error: 'Profile resources are unavailable',
        code: 'unavailable_resources',
        issues: [
          {
            path: 'opening_book.path',
            code: 'missing_resource',
            message: 'Book file is missing',
          },
        ],
      },
    }),
  );
  await page.goto('/');
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
    .click();
  await expect(page.getByRole('alert')).toContainText('Book file is missing');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  expect(backend.state.active_profile.name).toBe('Standard');
});

test('rejects duplicate names at the name field and allows a corrected name', async ({
  page,
}) => {
  await profileBackend(page);
  await page.goto('/');
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitem', { name: 'Manage profiles', exact: true })
    .click();
  const actions = page.getByRole('button', {
    name: 'Profile actions',
    exact: true,
  });
  await actions.click();
  await expect(
    page.getByRole('button', { name: 'Delete', exact: true }),
  ).toBeDisabled();
  await page.keyboard.press('Escape');
  await page
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  const create = page.getByRole('dialog', {
    name: 'Save as new profile',
    exact: true,
  });
  const name = create.getByLabel('Profile name', { exact: true });
  const save = create.getByRole('button', {
    name: 'Save as new profile',
    exact: true,
  });
  await name.fill(' standard ');
  await save.click();
  await expect(name).toHaveAttribute('aria-invalid', 'true');
  await name.fill('Corrected');
  await save.click();
  await expect(
    page.getByRole('button', { name: /Profile: Corrected/ }),
  ).toBeVisible();
});

test('preserves the draft after an unresolved conflict and confirms reloading server settings', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Player name', { exact: true }).fill('Local draft');
  backend.failNext(
    409,
    'revision_conflict',
    'Configuration changed',
    '/configuration',
  );
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText(
    'Settings changed elsewhere',
  );
  await expect(
    page.getByRole('textbox', { name: 'Player name', exact: true }),
  ).toHaveValue('Local draft');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Unapplied settings' })
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitem', { name: 'Manage profiles', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Load server settings', exact: true })
    .click();
  await page
    .getByRole('dialog', { name: 'Load server settings', exact: true })
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
  await expect(
    page.getByText('Settings changed in another window.', { exact: false }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Load server settings', exact: true })
    .click();
  await page
    .getByRole('dialog', { name: 'Load server settings', exact: true })
    .getByRole('button', { name: 'Discard', exact: true })
    .click();
  await expect(
    page.getByText('Settings changed in another window.', { exact: false }),
  ).toHaveCount(0);
});

test('shows profile errors and unavailable engines in the interface language', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const german = structuredClone(backend.state.current_configuration);
  german.general.locale = 'de';
  backend.updateRemote(german);
  const message = 'English server text';
  const issue = {
    path: 'analysis.engine',
    code: 'engine_unavailable',
    message,
  };
  await page.route('**/api/configuration/availability', (route) =>
    route.fulfill({
      json: {
        revision: backend.state.revision,
        valid: false,
        issues: [{ ...issue, engine: 'gnubg' }],
      },
    }),
  );
  await page.route('**/api/profiles/*/activate', (route) =>
    route.fulfill({
      status: 422,
      json: {
        error: message,
        code: 'unavailable_resources',
        issues: [{ ...issue, path: 'play.engine', engine: 'bgsage' }],
      },
    }),
  );
  await page.goto('/');
  const selector = page.locator('.profile-selector');
  await selector.locator('summary').click();
  await expect(selector).toContainText(
    'Analyse · Engine: Die eingestellte GNUbg-Installation ist nicht verfügbar.',
  );
  await selector.getByRole('button', { name: /Profil: Standard/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
    .click();
  // The modified profile asks first; the answer shows the failure.
  const alert = page.getByRole('alert');
  const discard = page.getByRole('button', { name: /verwerfen/i });
  await expect(alert.or(discard).first()).toBeVisible();
  if (await discard.count()) await discard.first().click();
  await expect(alert.first()).toContainText(
    'Einige dieser Einstellungen sind auf diesem Computer nicht verfügbar.',
  );
  await expect(alert.first()).toContainText(
    'Engine: Die NeonGammon-Installation oder die gewählte Tiefe ist nicht verfügbar oder wurde geändert.',
  );
  await expect(page.getByText(message)).toHaveCount(0);
});
