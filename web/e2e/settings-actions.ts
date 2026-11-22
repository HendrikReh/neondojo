import { expect, type Locator } from '@playwright/test';
import type { Locale } from '../src/shared/i18n/i18n-core';
import { profileText } from '../src/settings/profiles/profile-messages';
import { settingsText } from '../src/settings/settings-messages';

export async function closeSettings(dialog: Locator, locale: Locale = 'en') {
  await expect(
    dialog.getByRole('button', {
      name: profileText(locale, 'apply'),
      exact: true,
    }),
  ).toBeHidden();
  await dialog
    .getByRole('button', { name: settingsText(locale, 'done'), exact: true })
    .click();
  await expect(dialog).toBeHidden();
}

export async function applyAndCloseSettings(
  dialog: Locator,
  locale: Locale = 'en',
) {
  await dialog
    .getByRole('button', { name: profileText(locale, 'apply'), exact: true })
    .click();
  await closeSettings(dialog, locale);
}
