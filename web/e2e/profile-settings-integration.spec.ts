import { test, expect } from '@playwright/test';
import { profileBackend } from './profile-backend';

test('explicit conflict recovery rebases the entire appearance draft', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings
    .getByRole('button', { name: 'Appearance', exact: true })
    .click();
  await settings
    .getByRole('combobox', { name: 'Interface theme', exact: true })
    .selectOption('club');
  const remote = structuredClone(backend.state.current_configuration);
  remote.appearance.ui_theme = 'paper';
  remote.appearance.mode = 'dark';
  backend.updateRemote(remote);
  await settings.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(settings.getByRole('alert')).toContainText(
    'Settings changed elsewhere',
  );
  await settings
    .getByRole('button', { name: 'Load latest and discard draft', exact: true })
    .click();
  await expect(
    settings.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('paper');
  await expect(
    settings.getByRole('combobox', { name: 'Display mode', exact: true }),
  ).toHaveValue('dark');
});

test('settings drafts follow a profile activated inside the settings dialog', async ({
  page,
}) => {
  await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Profiles', exact: true }).click();
  await settings
    .getByRole('list', { name: 'Profiles', exact: true })
    .getByRole('button', { name: /Deep analysis/ })
    .click();
  await settings.getByRole('button', { name: 'Activate', exact: true }).click();
  await expect(
    page.getByRole('button', { name: /Profile: Deep analysis/ }),
  ).toBeVisible();
  await settings.getByRole('button', { name: 'Analysis', exact: true }).click();
  await expect(
    settings.getByLabel('Default analysis profile', { exact: true }),
  ).toHaveValue('4');
  await settings.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(settings).not.toBeVisible();
  await expect(
    page.getByRole('button', { name: /Profile: Deep analysis/ }),
  ).not.toContainText('modified');
});

test('opening book conflicts preserve the draft and offer explicit recovery', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Settings', exact: true })
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Opening books', exact: true })
    .click();
  const book = page.getByRole('dialog', { name: 'Opening books', exact: true });
  await book
    .getByLabel('Opening-book source', { exact: true })
    .selectOption('off');
  backend.failNext(
    409,
    'revision_conflict',
    'Configuration changed',
    '/configuration',
  );
  await book.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    book.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeDisabled();
  await expect(
    book.getByLabel('Opening-book source', { exact: true }),
  ).toHaveValue('off');
  await expect(
    book.getByRole('button', { name: /Reload|Retry|Try again|latest/i }),
  ).toBeVisible();
});
