import { DESKTOP_VIEWPORTS } from '../helpers/navigation';
import { installedBoardThemes } from '../helpers/board-themes';
import { applyAndCloseSettings } from '../helpers/settings-actions';
import { backendTest as test, expect } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';

test('leads with the active profile and compares the selected profile before saved details', async ({
  page,
}, info) => {
  const backend = await profileBackend(page);
  const current = structuredClone(backend.state.current_configuration);
  current.analysis.plies = 4;
  current.library.column_widths = { comment: 340 };
  current.library.filters.q = 'VeryLongSearchValue'.repeat(12);
  backend.updateRemote(current);
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Profiles', exact: true }).click();
  const active = settings.getByRole('region', { name: 'Active profile' });
  const overview = settings.getByRole('region', {
    name: 'Differences from current configuration',
  });
  const save = settings.getByRole('button', {
    name: 'Save in profile',
    exact: true,
  });
  const create = settings.getByRole('button', {
    name: 'Save as new profile',
    exact: true,
  });
  await expect(active).toContainText('Standard');
  await expect(active).toContainText('Changes from saved profile · 3');
  await expect(overview).toContainText('Analysis');
  await expect(overview).toContainText('Comment: 340');
  await expect(save).toHaveAccessibleDescription('Save destination: Standard');
  const columns = await overview.getByRole('table').evaluate((table) => {
    const width = table.getBoundingClientRect().width;
    return [...table.querySelectorAll('thead th')].map(
      (cell) => cell.getBoundingClientRect().width / width,
    );
  });
  [0.4, 0.3, 0.3].forEach((ratio, index) =>
    expect(columns[index]).toBeCloseTo(ratio, 2),
  );
  const currentValue = overview.getByText(current.library.filters.q, {
    exact: true,
  });
  await expect(currentValue).toBeVisible();
  expect(
    await currentValue.evaluate((value) => {
      const range = value.ownerDocument.createRange();
      range.selectNodeContents(value);
      return range.getClientRects().length;
    }),
  ).toBeGreaterThan(1);
  await expect(
    settings.getByLabel('Profile name', { exact: true }),
  ).toBeHidden();
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(overview).toBeInViewport();
    await expect(save).toBeInViewport();
    await expect(create).toBeInViewport();
    await settings.screenshot({
      path: info.outputPath(`profiles-${width}.png`),
    });
  }
  await settings.getByRole('button', { name: /^Deep analysis/ }).click();
  await expect(save).toHaveAccessibleDescription(
    'Save destination: Deep analysis',
  );
  await expect(active).toContainText('Standard');
  await expect(overview).toContainText('Dark');
  await expect(
    settings.getByRole('button', { name: 'Rename', exact: true }),
  ).toBeHidden();
  await expect(
    settings.getByRole('button', { name: 'Delete', exact: true }),
  ).toBeHidden();
  await settings.getByRole('button', { name: /^Standard/ }).click();
  await save.click();
  const confirm = page.getByRole('dialog', {
    name: 'Overwrite profile',
    exact: true,
  });
  await expect(confirm).toContainText('Standard');
  await confirm
    .getByRole('button', { name: 'Save in profile', exact: true })
    .click();
  await expect(active).toContainText('Changes from saved profile · 0');
  expect(backend.profiles[0].configuration.analysis.plies).toBe(4);
});

test('profile timestamps follow App language independently of browser locale', async ({
  browser,
}) => {
  const context = await browser.newContext({
    locale: 'en-US',
    timezoneId: 'Europe/Berlin',
  });
  try {
    const page = await context.newPage();
    const config = structuredClone(defaultConfiguration);
    config.general.locale = 'de';
    await profileBackend(page, { configuration: config });
    await page.goto('/');
    await page
      .getByRole('button', { name: 'Einstellungen', exact: true })
      .click();
    const dialog = page.getByRole('dialog', {
      name: 'Einstellungen',
      exact: true,
    });
    await dialog.getByRole('button', { name: 'Profile', exact: true }).click();
    const profile = dialog
      .getByRole('list', { name: 'Profile' })
      .getByRole('button', { name: /^Standard/ });
    await expect(profile).toContainText('10.11.2026, 13:00');
    await expect(profile.locator('time')).toHaveAttribute(
      'datetime',
      '2026-11-10T12:00:00Z',
    );
  } finally {
    await context.close();
  }
});

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
  await applyAndCloseSettings(
    page.getByRole('dialog', { name: 'Settings', exact: true }),
  );
  await expect(selector).toContainText('modified');
  expect(backend.profiles[0].configuration.general.player_name).toBe('');
  await page.reload();
  await expect(selector).toContainText('modified');
  await selector.click();
  await page
    .getByRole('menuitem', { name: 'Manage profiles', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Restore saved profile', exact: true })
    .click();
  await page
    .getByRole('dialog', { name: 'Restore saved profile' })
    .getByRole('button', { name: 'Restore saved profile', exact: true })
    .click();
  await expect(selector).not.toContainText('modified');
  await page
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  const create = page.getByRole('dialog', {
    name: 'Save as new profile',
    exact: true,
  });
  await create.getByLabel('Profile name', { exact: true }).fill('Training');
  await create
    .getByRole('button', { name: 'Save as new profile', exact: true })
    .click();
  await expect(create).toBeHidden();
  await expect(
    page.getByRole('button', { name: /Profile: Training/ }),
  ).toBeVisible();
  const menu = page.getByRole('button', {
    name: 'Profile actions',
    exact: true,
  });
  await menu.focus();
  await expect(menu).toBeFocused();
  await page.keyboard.press('Enter');
  const renameAction = page.getByRole('button', {
    name: 'Rename',
    exact: true,
  });
  await expect(renameAction).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(renameAction).toBeHidden();
  await expect(menu).toBeFocused();
  await menu.click();
  await renameAction.click();
  const rename = page.getByRole('dialog', {
    name: 'Rename profile',
    exact: true,
  });
  await expect(
    rename.getByLabel('Profile name', { exact: true }),
  ).toBeFocused();
  await rename
    .getByLabel('Profile name', { exact: true })
    .fill('Training renamed');
  await rename.getByRole('button', { name: 'Rename', exact: true }).click();
  await expect(rename).toBeHidden();
  await expect(menu).toBeFocused();
  await expect(
    page.getByRole('button', { name: /Profile: Training renamed/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: /^Deep analysis/ }).click();
  await menu.click();
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
  await applyAndCloseSettings(
    page.getByRole('dialog', { name: 'Settings', exact: true }),
  );
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
  const switchDialog = page.getByRole('dialog', {
    name: 'Switch profile',
    exact: true,
  });
  await expect(
    switchDialog.getByRole('button', { name: 'Save and switch', exact: true }),
  ).toHaveClass(/primary/);
  await switchDialog
    .getByRole('button', { name: 'Manage profiles', exact: true })
    .click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: /^Deep analysis/ }).click();
  await settings.getByRole('button', { name: 'Activate', exact: true }).click();
  await page
    .getByRole('button', { name: 'Save and switch', exact: true })
    .click();
  await settings
    .getByRole('button', { name: 'Close settings', exact: true })
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

test('a failed archive preview preserves the profile name draft and retries inside Settings', async ({
  page,
}) => {
  const archive = installedBoardThemes.find((theme) =>
    theme.id.startsWith('bgblitz:'),
  );
  test.skip(!archive, 'requires local BGBlitz artwork');
  const backend = await profileBackend(page);
  backend.profiles[1].configuration.appearance.board_theme = archive!.id;
  const pending = Promise.withResolvers<void>();
  let requests = 0;
  await page.route('**/archive-themes.ts*', async (route) => {
    if (++requests === 1) {
      await pending.promise;
      return route.abort('failed');
    }
    return route.continue();
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Profiles', exact: true }).click();
  const selected = settings.getByRole('button', { name: /^Deep analysis/ });
  await selected.click();
  const profiles = settings.getByRole('region', {
    name: 'Profiles',
    exact: true,
  });
  await expect(profiles.getByRole('status')).toHaveText('Loading …');
  await expect(selected).toHaveAttribute('aria-pressed', 'true');

  await settings
    .getByRole('button', { name: 'Save in profile', exact: true })
    .click();
  const overwrite = page.getByRole('dialog', {
    name: 'Overwrite profile',
    exact: true,
  });
  await expect(overwrite.getByRole('status')).toHaveText('Loading …');
  await overwrite.getByRole('button', { name: 'Cancel' }).click();

  await settings.getByRole('button', { name: 'Save as new profile' }).click();
  const create = page.getByRole('dialog', { name: 'Save as new profile' });
  const draft = create.getByLabel('Profile name');
  await draft.fill('Unfinished profile');
  pending.resolve();
  // The native child dialog makes its parent inert until Cancel.
  await expect(
    page
      .getByRole('dialog', {
        name: 'Settings',
        exact: true,
        includeHidden: true,
      })
      .locator('[role="alert"]'),
  ).toContainText('could not be loaded');
  await expect(draft).toHaveValue('Unfinished profile');
  await create.getByRole('button', { name: 'Cancel' }).click();
  await expect(selected).toHaveAttribute('aria-pressed', 'true');
  expect(requests).toBe(1);
  await profiles.getByRole('button', { name: 'Try again' }).click();
  await expect(
    profiles.getByRole('cell', { name: archive!.name, exact: true }),
  ).toBeVisible();
  expect(requests).toBe(2);
});

test('retrying an artwork load preserves four independent choices and the unsaved settings draft', async ({
  page,
}) => {
  const archive = installedBoardThemes.find((theme) =>
    theme.id.startsWith('bgblitz:'),
  );
  test.skip(!archive, 'requires local BGBlitz artwork');
  const backend = await profileBackend(page);
  const open = page.getByRole('button', { name: 'Settings', exact: true });
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  const board = dialog.getByRole('button', { name: 'Board', exact: true });
  const walnut = dialog.getByRole('button', { name: 'Walnut', exact: true });
  const checkers = dialog.getByRole('combobox', {
    name: 'Checker set',
    exact: true,
  });
  const cube = dialog.getByRole('combobox', {
    name: 'Doubling cube',
    exact: true,
  });
  const dice = dialog.getByRole('combobox', { name: 'Dice', exact: true });

  await page.goto('/');
  await open.click();
  await board.click();
  await walnut.click();
  await checkers.selectOption(archive!.id);
  await cube.selectOption('ion');
  await dice.selectOption('forest');
  await applyAndCloseSettings(dialog);

  // A new page load gives the archive module a cold cache. The active profile
  // still has artwork changes that have not been saved to the profile.
  await page.route('**/archive-themes.ts*', (route) => route.abort('failed'), {
    times: 1,
  });
  await page.reload();
  await open.click();
  const player = dialog.getByLabel('Player name', { exact: true });
  await player.fill('Unsaved player');
  await board.click();
  await expect(dialog.getByRole('alert')).toContainText('could not be loaded');
  await dialog.getByRole('button', { name: 'Try again' }).click();
  await expect(walnut).toHaveAttribute('aria-pressed', 'true');
  await expect(checkers).toHaveValue(archive!.id);
  await expect(cube).toHaveValue('ion');
  await expect(dice).toHaveValue('forest');
  await dialog.getByRole('button', { name: 'General', exact: true }).click();
  await expect(player).toHaveValue('Unsaved player');
  expect(backend.state.current_configuration.general.player_name).not.toBe(
    'Unsaved player',
  );
  expect(backend.state.dirty).toBe(true);
});
