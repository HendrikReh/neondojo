import { applyAndCloseSettings } from './settings-actions';
import { backendTest as test, expect } from './test';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/settings/configuration/configuration-model';

for (const { locale, width, labels } of [
  {
    locale: 'en' as const,
    width: 1440,
    labels: {
      profile: 'Profile: Standard',
      show: 'Show changes',
      title: 'Profile changes: Standard',
      columns: ['Setting', 'Saved', 'Current'],
      depth: 'Analysis · Search depth (ply)',
      sounds: 'General · Game sounds',
      board: 'Appearance · Board theme',
      summary: 'Saved configuration',
      on: 'On',
      off: 'Off',
    },
  },
  {
    locale: 'de' as const,
    width: 390,
    labels: {
      profile: 'Profil: Standard',
      show: 'Änderungen anzeigen',
      title: 'Profiländerungen: Standard',
      columns: ['Einstellung', 'Gespeichert', 'Aktuell'],
      depth: 'Analyse · Suchtiefe (Ply)',
      sounds: 'Allgemein · Spielgeräusche',
      board: 'Darstellung · Board-Design',
      summary: 'Gespeicherte Konfiguration',
      on: 'An',
      off: 'Aus',
    },
  },
]) {
  test(`shows only changed profile settings, saved/current values and restores focus ${locale}/${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    const configuration = structuredClone(defaultConfiguration);
    configuration.general.locale = locale;
    const backend = await profileBackend(page, { configuration });
    const changed = structuredClone(backend.state.current_configuration);
    changed.analysis.plies = 4;
    changed.general.play_sounds = false;
    changed.appearance.board_theme = 'walnut';
    backend.updateRemote(changed);
    await page.goto('/');
    const selector = page.getByRole('button', {
      name: new RegExp(labels.profile),
    });
    await selector.click();
    const show = page.getByRole('menuitem', { name: labels.show, exact: true });
    await expect(show).toBeVisible();
    if (locale === 'en') {
      await page.keyboard.press('End');
      await page.keyboard.press('ArrowUp');
      await expect(show).toBeFocused();
      await page.keyboard.press('Enter');
    } else await show.click();
    const dialog = page.getByRole('dialog', {
      name: labels.title,
      exact: true,
    });
    await expect(dialog).toBeVisible();
    await expect(dialog).toBeFocused();
    await expect(dialog.getByRole('columnheader')).toHaveText(labels.columns);
    await expect(dialog.locator('tbody tr')).toHaveCount(3);
    for (const [field, saved, current] of [
      [labels.depth, '3', '4'],
      [labels.sounds, labels.on, labels.off],
      [labels.board, 'Forest', 'Walnut'],
    ]) {
      const row = dialog.getByRole('row').filter({
        has: page.getByRole('rowheader', { name: field, exact: true }),
      });
      await expect(row.getByRole('cell').first()).toHaveText(saved);
      await expect(row.locator('[data-profile-current-value]')).toHaveText(
        current,
      );
    }
    await expect(
      dialog.getByRole('heading', { name: labels.summary, exact: true }),
    ).toHaveCount(0);
    const box = await dialog.boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: test.info().outputPath('profile-changes.png'),
      fullPage: true,
    });
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(selector).toBeFocused();
    expect(backend.calls.filter((call) => call.method !== 'GET')).toEqual([]);
  });
}

test('offers changes only while values differ from the saved active profile', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  const selector = page.getByRole('button', { name: /Profile: Standard/ });
  await selector.click();
  await expect(
    page.getByRole('menuitem', { name: 'Show changes', exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press('Escape');
  for (const name of ['Changed player', '']) {
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.getByLabel('Player name', { exact: true }).fill(name);
    await applyAndCloseSettings(
      page.getByRole('dialog', { name: 'Settings', exact: true }),
    );
    await selector.click();
    await expect(
      page.getByRole('menuitem', { name: 'Show changes', exact: true }),
    ).toHaveCount(name ? 1 : 0);
    await page.keyboard.press('Escape');
  }
  expect(backend.profiles[0].configuration.general.player_name).toBe('');
});

test('keeps long profile names and changed search values within the mobile dialog', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 1100 });
  const backend = await profileBackend(page);
  const name = 'Training' + 'x'.repeat(72);
  backend.profiles[0].name = name;
  const changed = structuredClone(backend.state.current_configuration);
  changed.library.filters.q = 'longsearchvalue'.repeat(30);
  backend.updateRemote(changed);
  await page.goto('/');
  const selector = page.getByRole('button', {
    name: new RegExp(`Profile: ${name}`),
  });
  const clippedName = selector.locator('.profile-name');
  await selector.focus();
  await expect(page.getByRole('tooltip')).toHaveText(name);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await expect(selector).toBeFocused();
  await clippedName.hover();
  await expect(page.getByRole('tooltip')).toHaveText(name);
  await selector.click();
  await page
    .getByRole('menuitem', { name: 'Show changes', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: `Profile changes: ${name}`,
    exact: true,
  });
  await expect(dialog).toBeVisible();
  const close = await dialog
    .getByRole('button', { name: 'Close', exact: true })
    .boundingBox();
  expect(close!.x + close!.width).toBeLessThanOrEqual(390);
  expect(await dialog.evaluate((el) => el.scrollWidth <= el.clientWidth)).toBe(
    true,
  );
  const savedColumn = await dialog
    .getByRole('columnheader', { name: 'Saved', exact: true })
    .boundingBox();
  expect(savedColumn!.width).toBeGreaterThan(60);
  await expect(dialog.locator('[data-profile-current-value]')).toHaveText(
    changed.library.filters.q,
  );
  await page.screenshot({
    path: test.info().outputPath('profile-changes-long-mobile.png'),
    fullPage: true,
  });
});
