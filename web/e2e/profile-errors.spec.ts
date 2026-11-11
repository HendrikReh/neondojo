import { test, expect } from '@playwright/test';
import { profileBackend } from './profile-backend';

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
  await expect(
    page.getByRole('button', { name: 'Delete', exact: true }),
  ).toBeDisabled();
  await page.getByLabel('Profile name', { exact: true }).fill(' standard ');
  await page
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  await expect(
    page.getByLabel('Profile name', { exact: true }),
  ).toHaveAttribute('aria-invalid', 'true');
  await page.getByLabel('Profile name', { exact: true }).fill('Corrected');
  await page
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /Profile: Corrected/ }),
  ).toBeVisible();
});

test('preserves local configuration on a stale revision and confirms reloading server settings', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Player name', { exact: true }).fill('Local draft');
  const changed = structuredClone(backend.state.current_configuration);
  changed.general.player_name = 'Remote';
  backend.updateRemote(changed);
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    page.getByRole('textbox', { name: 'Player name', exact: true }),
  ).toHaveValue('Local draft');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
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
