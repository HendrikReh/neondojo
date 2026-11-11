import { test, expect } from '@playwright/test';
import { profileBackend } from './profile-backend';

test('applies settings without overwriting, restores and manages saved profiles', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  const selector = page.getByRole('button', { name: /Profile: Standard/ });
  await expect(selector).toBeVisible();
  await selector.focus();
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Player name', { exact: true }).fill('Training player');
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(selector).toContainText('modified');
  expect(backend.profiles[0].configuration.general.player_name).toBe('');
  await page.reload();
  await expect(selector).toContainText('modified');
  await selector.click();
  await page
    .getByRole('menuitem', { name: 'Manage profiles', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Discard changes', exact: true })
    .click();
  await page
    .getByRole('dialog', { name: 'Discard profile changes' })
    .getByRole('button', { name: 'Discard', exact: true })
    .click();
  await expect(selector).not.toContainText('modified');
  await page.getByLabel('Profile name', { exact: true }).fill('Training');
  await page
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /Profile: Training/ }),
  ).toBeVisible();
  await page
    .getByLabel('Profile name', { exact: true })
    .fill('Training renamed');
  await page.getByRole('button', { name: 'Rename', exact: true }).click();
  await expect(
    page.getByRole('button', { name: /Profile: Training renamed/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: /^Deep analysis/ }).click();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Delete profile', exact: true })
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /^Deep analysis/ }),
  ).toHaveCount(0);
  expect(
    backend.calls.filter((c) => c.path.endsWith('/snapshot')),
  ).toHaveLength(0);
});

test('supports cancel, save and discard decisions when switching changed profiles', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByLabel('Player name', { exact: true }).fill('Saved to source');
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
    .click();
  await page
    .getByRole('dialog', { name: 'Switch profile', exact: true })
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toContainText('modified');
  await page.getByRole('button', { name: /Profile: Standard/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Save and switch', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /Profile: Deep analysis/ }),
  ).toBeVisible();
  expect(backend.profiles[0].configuration.general.player_name).toBe(
    'Saved to source',
  );
  await page.getByRole('button', { name: /Profile: Deep analysis/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Standard', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).not.toContainText('modified');
});
