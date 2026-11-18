import { applyAndCloseSettings } from './settings-actions';
import { expect, test, type Page } from './test';
import { readFileSync } from 'node:fs';

test.use({ viewport: { width: 1800, height: 1169 } });

const initialSettings = {
  version: 3,
  player_name: 'Appearance Player',
  analysis_engine: 'gnubg',
  analysis_plies: 3,
  play_engine: 'gnubg',
  play_plies: 2,
  gnubg_path: '/opt/bin/gnubg',
};

async function setup(page: Page, boardTheme = 'walnut') {
  let saved = { ...initialSettings },
    conflict = false,
    loadFailure = false;
  const writes: unknown[] = [];
  await page.route('**/api/settings', async (route) => {
    if (route.request().method() === 'GET')
      return route.fulfill(
        loadFailure
          ? { status: 503, json: { error: 'Unavailable' } }
          : { json: saved },
      );
    writes.push(route.request().postDataJSON());
    if (conflict)
      return route.fulfill({ status: 409, json: { error: 'Version changed' } });
    saved = { ...route.request().postDataJSON(), version: saved.version + 1 };
    return route.fulfill({ json: saved });
  });
  await page.route('**/api/configuration', async (route) => {
    if (route.request().method() !== 'PUT') return route.fallback();
    const c = route.request().postDataJSON().configuration;
    const proposed = {
      ...saved,
      player_name: c.general.player_name,
      analysis_engine: c.analysis.engine,
      analysis_plies: c.analysis.plies,
      play_engine: c.play.engine,
      play_plies: c.play.plies,
    };
    writes.push(proposed);
    if (conflict)
      return route.fulfill({
        status: 409,
        json: {
          error: 'Version changed',
          code: 'revision_conflict',
          issues: [],
        },
      });
    saved = { ...proposed, version: saved.version + 1 };
    return route.fallback();
  });
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      json: [
        {
          id: 'gnubg',
          name: 'GNU Backgammon',
          available: true,
          version: 'GNUbg 1.08',
          plies: [1, 2, 3, 4, 5],
        },
      ],
    }),
  );
  await page.addInitScript((boardTheme) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.theme', boardTheme);
  }, boardTheme);
  return {
    writes,
    conflict: (value: boolean) => {
      conflict = value;
    },
    loadFailure: (value: boolean) => {
      loadFailure = value;
    },
  };
}

async function appearanceSettings(page: Page) {
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
  await dialog.getByRole('button', { name: 'Appearance', exact: true }).click();
  return dialog;
}

const desktopSizes = [
  { width: 1800, height: 1169 },
  { width: 2560, height: 1440 },
  { width: 3840, height: 2160 },
];

test('appearance and board place their previews alongside the choices on desktop', async ({
  page,
}, info) => {
  await setup(page);
  await page.goto('/');
  const dialog = await appearanceSettings(page);
  const theme = dialog.getByRole('combobox', {
    name: 'Interface theme',
    exact: true,
  });
  const gallery = dialog.getByRole('group', {
    name: 'Available board themes',
    exact: true,
  });

  for (const size of desktopSizes) {
    await page.setViewportSize(size);
    await dialog
      .getByRole('button', { name: 'Appearance', exact: true })
      .click();
    const appearance = dialog.getByRole('figure', {
      name: 'Interface preview',
    });
    const themeBox = (await theme.boundingBox())!;
    const appearanceBox = (await appearance.boundingBox())!;
    expect(appearanceBox.x).toBeGreaterThan(themeBox.x + themeBox.width);
    expect(Math.abs(appearanceBox.y - themeBox.y)).toBeLessThan(45);
    await expect(appearance).toBeInViewport({ ratio: 1 });
    await dialog.screenshot({
      path: info.outputPath(`appearance-${size.width}.png`),
    });

    await dialog.getByRole('button', { name: 'Board', exact: true }).click();
    const preview = dialog.getByRole('region', { name: /^Preview: / });
    const galleryBox = (await gallery.boundingBox())!;
    const previewBox = (await preview.boundingBox())!;
    expect(previewBox.x).toBeGreaterThan(galleryBox.x + galleryBox.width);
    expect(Math.abs(previewBox.y - galleryBox.y)).toBeLessThan(8);
    await expect(preview).toBeInViewport({ ratio: 1 });
    await dialog.screenshot({
      path: info.outputPath(`board-${size.width}.png`),
    });
  }
});

test('scrolling and choosing from the board gallery keeps the preview in view', async ({
  page,
}) => {
  const archives = JSON.parse(
    readFileSync(
      new URL('../src/generated-themes.json', import.meta.url),
      'utf8',
    ),
  ) as { id: string; name: string }[];
  test.skip(
    archives.length < 7,
    'Requires enough local themes to scroll the gallery',
  );
  const savedTheme = archives.at(-1)!;
  await setup(page, savedTheme.id);
  await page.goto('/');
  const dialog = await appearanceSettings(page);
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  const gallery = dialog.getByRole('group', {
    name: 'Available board themes',
    exact: true,
  });
  const preview = dialog.getByRole('region', { name: /^Preview: / });
  const selected = gallery.getByText(/^✓ Selected$/);
  const initialChoice = gallery.getByRole('button', { pressed: true });
  await expect(initialChoice).toHaveAccessibleName(savedTheme.name);
  await expect(initialChoice).toBeInViewport({ ratio: 1 });
  await expect(
    dialog.getByRole('button', { name: 'Board', exact: true }),
  ).toBeFocused();
  const before = await preview.boundingBox();
  await gallery.press('Home');
  await expect
    .poll(() => gallery.evaluate((element) => element.scrollTop))
    .toBe(0);
  await expect(initialChoice).not.toBeInViewport();
  await dialog.getByRole('button', { name: 'Appearance', exact: true }).click();
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(initialChoice).toBeInViewport({ ratio: 1 });
  await gallery.press('Home');
  await expect
    .poll(() => gallery.evaluate((element) => element.scrollTop))
    .toBe(0);
  await gallery.hover();
  await page.mouse.wheel(0, 1100);
  await expect
    .poll(() => gallery.evaluate((element) => element.scrollTop))
    .toBeGreaterThan(0);
  expect(await preview.boundingBox()).toEqual(before);
  await expect(preview).toBeInViewport({ ratio: 1 });
  const choice = gallery.getByRole('button').nth(-2);
  const name = await choice.getAttribute('aria-label');
  await choice.press('Space');
  await expect(choice).toBeFocused();
  await expect(choice).toHaveAttribute('aria-pressed', 'true');
  await expect(selected).toHaveCount(1);
  await expect(choice.getByText(/^✓ Selected$/)).toBeInViewport();
  await expect(preview).toHaveAccessibleName(`Preview: ${name}`);
  await expect(preview).toBeInViewport({ ratio: 1 });
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeInViewport();
});

const backgrounds = {
  graphite: { light: 'rgb(238, 241, 245)', dark: 'rgb(21, 25, 31)' },
  club: { light: 'rgb(237, 242, 238)', dark: 'rgb(21, 30, 26)' },
  paper: { light: 'rgb(242, 238, 231)', dark: 'rgb(33, 29, 29)' },
};
const surfaces = {
  graphite: { light: 'rgb(255, 255, 255)', dark: 'rgb(30, 36, 45)' },
  club: { light: 'rgb(251, 253, 249)', dark: 'rgb(29, 42, 35)' },
  paper: { light: 'rgb(255, 252, 245)', dark: 'rgb(43, 37, 37)' },
};

for (const theme of ['graphite', 'club', 'paper'] as const) {
  for (const mode of ['light', 'dark'] as const) {
    test(`${theme} ${mode} previews, saves and survives reload independently of board artwork`, async ({
      page,
    }) => {
      await setup(page);
      await page.emulateMedia({ colorScheme: 'light' });
      const initialized = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname ===
            '/api/configuration/initialize' && response.ok(),
      );
      await page.goto('/');
      await initialized;
      const dialog = await appearanceSettings(page);
      await dialog
        .getByRole('combobox', { name: 'Interface theme', exact: true })
        .selectOption(theme);
      await dialog
        .getByRole('combobox', { name: 'Display mode', exact: true })
        .selectOption(mode);
      const preview = dialog.locator('.ui-appearance-preview');
      await expect(preview).toHaveAttribute('data-ui-theme', theme);
      await expect(preview).toHaveCSS(
        'background-color',
        backgrounds[theme][mode],
      );
      await expect(page.locator('html')).toHaveAttribute(
        'data-ui-theme',
        'graphite',
      );
      await expect(page.locator('html')).toHaveAttribute(
        'data-ui-mode',
        'light',
      );
      await dialog.getByRole('button', { name: 'Board', exact: true }).click();
      await expect(
        dialog.locator('.settings-preview .theme-walnut'),
      ).toBeVisible();
      await applyAndCloseSettings(dialog);
      await expect(page.locator('html')).toHaveAttribute(
        'data-ui-theme',
        theme,
      );
      await expect(page.locator('html')).toHaveAttribute('data-ui-mode', mode);
      await expect(page.locator('.app-shell')).toHaveCSS(
        'background-color',
        backgrounds[theme][mode],
      );
      expect(
        await page.evaluate(() => localStorage.getItem('bgtutor.theme')),
      ).toBe('walnut');
      await page.reload();
      await expect(page.locator('html')).toHaveAttribute(
        'data-ui-theme',
        theme,
      );
      await expect(page.locator('html')).toHaveAttribute('data-ui-mode', mode);
      const savedDialog = await appearanceSettings(page);
      await expect(savedDialog).toHaveCSS(
        'background-color',
        surfaces[theme][mode],
      );
      await expect(
        savedDialog.getByRole('combobox', {
          name: 'Display mode',
          exact: true,
        }),
      ).toHaveCSS('background-color', surfaces[theme][mode]);
      await savedDialog
        .getByRole('button', { name: 'Close', exact: true })
        .click();
      for (const tab of ['Match database', 'Positions', 'Study']) {
        await page.getByRole('tab', { name: tab, exact: true }).click();
        await expect(page.locator('.app-shell')).toHaveCSS(
          'background-color',
          backgrounds[theme][mode],
        );
      }
    });
  }
}

test('interface appearance preserves selected original ZIP artwork', async ({
  page,
}) => {
  const archive = JSON.parse(
    readFileSync(
      new URL('../src/generated-themes.json', import.meta.url),
      'utf8',
    ),
  )[0] as { id: string; name: string } | undefined;
  test.skip(!archive, 'No local board archives are installed');
  await setup(page);
  await page.goto('/');
  const dialog = await appearanceSettings(page);
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await dialog
    .getByRole('button', { name: archive!.name, exact: true })
    .click();
  const originalImage = await dialog
    .locator('.settings-preview .bgblitz-board image')
    .first()
    .getAttribute('href');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await dialog.getByRole('button', { name: 'Appearance', exact: true }).click();
  await dialog
    .getByRole('combobox', { name: 'Interface theme', exact: true })
    .selectOption('paper');
  await dialog
    .getByRole('combobox', { name: 'Display mode', exact: true })
    .selectOption('dark');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(
    dialog.locator('.settings-preview .bgblitz-board'),
  ).toHaveAttribute('data-theme', archive!.id);
  await expect(
    dialog.locator('.settings-preview .bgblitz-board image').first(),
  ).toHaveAttribute('href', originalImage!);
  expect(await page.evaluate(() => localStorage.getItem('bgtutor.theme'))).toBe(
    archive!.id,
  );
});

test('cancel and save conflicts retain the applied appearance and board preference', async ({
  page,
}) => {
  const backend = await setup(page);
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/');
  let dialog = await appearanceSettings(page);
  await dialog
    .getByRole('combobox', { name: 'Interface theme', exact: true })
    .selectOption('paper');
  await dialog
    .getByRole('combobox', { name: 'Display mode', exact: true })
    .selectOption('dark');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Unapplied settings' })
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  expect(backend.writes).toHaveLength(0);
  expect(
    JSON.parse(
      (await page.evaluate(() =>
        localStorage.getItem('bgtutor.ui-appearance'),
      ))!,
    ),
  ).toEqual({ version: 1, theme: 'graphite', mode: 'system' });
  dialog = await appearanceSettings(page);
  await expect(
    dialog.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('graphite');
  await dialog
    .getByRole('combobox', { name: 'Interface theme', exact: true })
    .selectOption('club');
  await dialog
    .getByRole('combobox', { name: 'Display mode', exact: true })
    .selectOption('dark');
  backend.conflict(true);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText(
    'Settings changed elsewhere',
  );
  await expect(
    dialog.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('club');
  await expect(page.locator('html')).toHaveAttribute(
    'data-ui-theme',
    'graphite',
  );
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'light');
  await dialog
    .getByRole('button', { name: 'Load latest and discard draft', exact: true })
    .click();
  await expect(
    dialog.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('graphite');
  await expect(
    dialog.getByRole('combobox', { name: 'Display mode', exact: true }),
  ).toHaveValue('system');
  expect(await page.evaluate(() => localStorage.getItem('bgtutor.theme'))).toBe(
    'walnut',
  );
});

test('System follows changes while explicit light remains fixed', async ({
  page,
}) => {
  await setup(page);
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'light');
  const dialog = await appearanceSettings(page);
  await dialog
    .getByRole('combobox', { name: 'Display mode', exact: true })
    .selectOption('light');
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'light');
});

test('loading errors prevent saving appearance before settings are available', async ({
  page,
}) => {
  const backend = await setup(page);
  backend.loadFailure(true);
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByRole('button', { name: 'Appearance', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText(
    'Settings could not be loaded',
  );
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await expect(
    dialog.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveCount(0);
  expect(
    await page.evaluate(() => localStorage.getItem('bgtutor.ui-appearance')),
  ).toBeNull();
});
