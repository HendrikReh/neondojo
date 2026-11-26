import { profileText } from '../src/settings/profiles/profile-messages';
import { test, expect, type Locator, type Page } from './test';
import { applyAndCloseSettings, closeSettings } from './settings-actions';
import { readFileSync } from 'node:fs';
import { catalogs } from '../src/shared/i18n/i18n-catalogs';
import { candidateMessages } from '../src/workspaces/analysis/candidates/candidate-messages';
import type { Locale } from '../src/shared/i18n/i18n-core';
import { learningMessages } from '../src/workspaces/study/flashcards/learning-messages';
import { settingsText } from '../src/settings/settings-messages';
import { workspaceText } from '../src/shared/i18n/workspace-messages';
import { openPositionCalculation } from './analysis-controls';

const themes = JSON.parse(
  readFileSync(
    new URL('../src/generated-themes.json', import.meta.url),
    'utf8',
  ),
) as {
  id: string;
  name: string;
  background: { width: number; height: number };
}[];

// These legacy flows use an explicit existing German preference. Preserve any
// saved choice on reload. settings.spec.ts covers a new installation in English.
test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (localStorage.getItem('neondojo.locale') === null)
      localStorage.setItem('neondojo.locale', 'de');
  });
});

const analysis = (page: Page) => page.locator('.analysis-workspace');
const currentLocale = async (page: Page) =>
  (await page.locator('html').getAttribute('lang')) as Locale;

async function upload(page: Page) {
  await page.goto('/');
  const locale = await currentLocale(page);
  await page
    .getByLabel(catalogs[locale]['app.matchFile'], { exact: true })
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(analysis(page).locator('.board')).toBeVisible();
}

async function openSettings(page: Page) {
  const locale = await currentLocale(page);
  await page
    .getByRole('button', {
      name: workspaceText(locale, 'settings'),
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog', {
    name: settingsText(locale, 'title'),
    exact: true,
  });
  await expect(
    dialog.getByLabel(settingsText(locale, 'playerName'), { exact: true }),
  ).toBeVisible();
  return { dialog, locale };
}

async function applyTheme(page: Page, theme: (typeof themes)[number]) {
  const { dialog, locale } = await openSettings(page);
  await dialog
    .getByRole('button', { name: settingsText(locale, 'board'), exact: true })
    .click();
  const choice = dialog.getByRole('button', { name: theme.name, exact: true });
  const alreadySelected =
    (await choice.getAttribute('aria-pressed')) === 'true';
  await choice.click();
  await expect(
    dialog.locator('.settings-preview .bgblitz-board'),
  ).toHaveAttribute('data-theme', theme.id);
  await assertArtwork(dialog.locator('.settings-preview .bgblitz-board'));
  if (alreadySelected) await closeSettings(dialog, locale);
  else await applyAndCloseSettings(dialog, locale);
}

async function applyLanguage(page: Page, next: Locale) {
  const { dialog, locale } = await openSettings(page);
  await dialog
    .getByRole('button', { name: settingsText(locale, 'general'), exact: true })
    .click();
  await dialog
    .getByLabel(settingsText(locale, 'language'), { exact: true })
    .selectOption(next);
  await expect(page.locator('html')).toHaveAttribute('lang', locale);
  await dialog
    .getByRole('button', { name: profileText(locale, 'apply'), exact: true })
    .click();
  await expect(page.locator('html')).toHaveAttribute('lang', next);
  const translatedSettings = page.getByRole('dialog', {
    name: settingsText(next, 'title'),
    exact: true,
  });
  await expect(
    translatedSettings.getByRole('button', {
      name: settingsText(next, 'general'),
      exact: true,
    }),
  ).toHaveAttribute('aria-current', 'page');
  await closeSettings(translatedSettings, next);
}

async function reopenTutor(page: Page) {
  const locale = await currentLocale(page);
  await page
    .getByRole('tab', { name: workspaceText(locale, 'database'), exact: true })
    .click();
  await page
    .getByLabel(catalogs[locale]['app.searchMatches'], { exact: true })
    .fill('Tutor Demo');
  await page
    .locator('.match-database .match-card')
    .filter({ hasText: 'Tutor Demo' })
    .first()
    .click();
  await expect(
    page.getByRole('tab', {
      name: workspaceText(locale, 'analysis'),
      exact: true,
    }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(analysis(page).locator('.board')).toBeVisible();
}

async function assertArtwork(svg: Locator) {
  const failures = await svg.evaluate(async (element) => {
    const urls = [
      ...new Set(
        [...element.querySelectorAll('image')].map((node) =>
          node.getAttribute('href')!,
        ),
      ),
    ];
    return (
      await Promise.all(
        urls.map(async (src) => {
          const image = new Image();
          image.src = src;
          try {
            await image.decode();
            return null;
          } catch {
            return src;
          }
        }),
      )
    ).filter(Boolean);
  });
  expect(failures).toEqual([]);
}

async function holdModule(page: Page, url: string) {
  const requested = Promise.withResolvers<void>();
  const gate = Promise.withResolvers<void>();
  await page.route(url, async (route) => {
    requested.resolve();
    await gate.promise;
    await route.continue();
  });
  return { requested: requested.promise, release: gate.resolve };
}

test('waits for the saved language and artwork before mounting, and keeps the current language while switching', async ({
  page,
}) => {
  const theme = themes.find(({ id }) => id === 'bgblitz:bg-quantum');
  test.skip(!theme, 'optional local Quantum artwork');
  await upload(page);
  const initialModules = await page.evaluate(() =>
    performance
      .getEntriesByType('resource')
      .map(({ name }) => new URL(name).pathname),
  );
  expect(
    initialModules.some((path) => path.endsWith('/generated-themes.json')),
  ).toBe(false);
  expect(
    initialModules.filter((path) => /\/locales\/[a-z]{2}\.ts$/.test(path)),
  ).toEqual(['/src/shared/i18n/locales/de.ts']);

  await applyTheme(page, theme!);

  const language = await holdModule(page, '**/locales/de.ts*');
  const artwork = await holdModule(page, '**/generated-themes.json*');
  await page.reload({ waitUntil: 'domcontentloaded' });
  await Promise.all([language.requested, artwork.requested]);
  await expect(page.locator('#root')).toBeEmpty();
  language.release();
  await expect(page.locator('#root')).toBeEmpty();
  artwork.release();
  await expect(
    page.getByRole('tab', { name: 'Analyse', exact: true }),
  ).toBeVisible();
  const board = analysis(page).locator('.bgblitz-board');
  await expect(board).toHaveAttribute('data-theme', theme!.id);

  const english = await holdModule(page, '**/locales/en.ts*');
  const { dialog, locale } = await openSettings(page);
  await dialog
    .getByLabel(settingsText(locale, 'language'), { exact: true })
    .selectOption('en');
  await dialog
    .getByRole('button', { name: profileText(locale, 'apply'), exact: true })
    .click();
  await english.requested;
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  await expect(
    dialog.getByLabel(settingsText('de', 'language'), { exact: true }),
  ).toBeVisible();
  english.release();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(
    page.getByRole('dialog', { name: settingsText('en', 'title') }),
  ).toBeVisible();
  await expect(board).toHaveAttribute('data-theme', theme!.id);
});

test('every local ZIP theme uses its original assets and survives reload on desktop/mobile', async ({
  page,
}) => {
  test.setTimeout(120_000);
  test.skip(!themes.length, 'optional local theme archives');
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await upload(page);
  const board = analysis(page).locator('.bgblitz-board');
  for (const theme of themes) {
    await applyTheme(page, theme);
    await expect(board).toHaveAttribute('data-theme', theme.id);
    await expect(board).toHaveAttribute(
      'viewBox',
      `0 0 ${theme.background.width} ${theme.background.height}`,
    );
    await assertArtwork(board);
    await expect(board.locator('[data-point="24"]')).toHaveAttribute(
      'data-count',
      '2',
    );
    await expect(board.locator('[data-point="1"]')).toHaveAttribute(
      'data-count',
      '-2',
    );
    await expect(board.locator('[data-number-orientation]')).toHaveAttribute(
      'data-number-orientation',
      'RLD',
    );
    await analysis(page).getByRole('button', { name: 'Nächster Zug' }).click();
    await expect(board.locator('[data-number-orientation]')).toHaveAttribute(
      'data-number-orientation',
      'RLU',
    );
    await analysis(page)
      .getByRole('button', { name: 'Vorheriger Zug' })
      .click();
  }
  const selected = themes.find((t) => t.id === 'bgblitz:cl_green') || themes[0];
  await applyTheme(page, selected);
  await analysis(page)
    .locator('.board-panel')
    .screenshot({ path: 'test-results/bgblitz-desktop.png' });
  await page.reload();
  const { dialog, locale } = await openSettings(page);
  await dialog
    .getByRole('button', { name: settingsText(locale, 'board'), exact: true })
    .click();
  await expect(
    dialog.getByRole('button', { name: selected.name, exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await dialog
    .getByRole('button', { name: settingsText(locale, 'done'), exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  await reopenTutor(page);
  await expect(board).toHaveAttribute('data-theme', selected.id);
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: 'test-results/bgblitz-mobile.png',
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test('bar, borne-off checkers and owned cube retain their position data in archive themes', async ({
  page,
}) => {
  const theme = themes.find((t) => t.id === 'bgblitz:cl_green');
  test.skip(!theme, 'optional local Rain artwork');
  await page.goto('/');
  await page
    .getByLabel(catalogs.de['app.matchFile'], { exact: true })
    .setInputFiles({
      name: 'theme-position.sgf',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        '(;GM[6]PW[Theme White]PB[Theme Black]MI[length:7][ws:2][bs:3]AE[a:y]AW[y][f]AB[y][s]PL[W]CV[4]CO[b];W[61ysfe])',
      ),
    });
  await expect(analysis(page).locator('.board')).toBeVisible();
  await applyTheme(page, theme!);
  const board = analysis(page).locator('.bgblitz-board');
  await expect(board.locator('[data-point="bar-white"]')).toHaveAttribute(
    'data-count',
    '1',
  );
  await expect(board.locator('[data-point="bar-black"]')).toHaveAttribute(
    'data-count',
    '-1',
  );
  await expect(board.locator('[data-point="off-white"]')).toHaveAttribute(
    'data-count',
    '13',
  );
  await expect(board.locator('[data-point="off-black"]')).toHaveAttribute(
    'data-count',
    '-13',
  );
  await expect(board.locator('[data-cube]')).toHaveAttribute('data-cube', '4');
  await expect(board.locator('[data-cube]')).toHaveAttribute(
    'data-cube-owner',
    '1',
  );
  await expect(board.locator('[data-cube] image')).toHaveAttribute(
    'href',
    /cube4\.png$/,
  );
  await expect(board.locator('[data-cube] image')).toHaveAttribute('x', '57');
  await assertArtwork(board);
});

test('both languages switch complete views on save and persist with the theme', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await upload(page);
  for (const locale of ['en', 'de'] as const) {
    await applyLanguage(page, locale);
    const navigation = page.locator('.app-tabs');
    await expect(
      navigation.getByRole('tab', {
        name: workspaceText(locale, 'analysis'),
        exact: true,
      }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(
      navigation.getByRole('tab', {
        name: workspaceText(locale, 'database'),
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      navigation.getByRole('tab', {
        name: learningMessages[locale].study,
        exact: true,
      }),
    ).toBeVisible();
    await expect(analysis(page).locator('.analysis-heading h1')).toContainText(
      'Tutor Demo',
    );
    await expect(analysis(page).locator('.analysis-heading h1')).toContainText(
      'GNUbg Demo',
    );
    const matchOptions = analysis(page)
      .locator('.workspace-analysis-command')
      .getByRole('button', {
        name: catalogs[locale]['analysis.matchOptions'],
        exact: true,
      });
    await expect(matchOptions).toBeVisible();
    // Position recalculation lives under Analysis options.
    if ((await matchOptions.getAttribute('aria-expanded')) === 'false')
      await matchOptions.click();
    await expect(
      analysis(page).getByRole('button', {
        name: `${catalogs[locale]['analysis.recalculate']} …`,
        exact: true,
      }),
    ).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(
      analysis(page).getByRole('img', {
        name: new RegExp(
          catalogs[locale]['board.label']
            .split('{')[0]
            .replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
        ),
      }),
    ).toBeVisible();
    await page.setViewportSize({ width: 390, height: 844 });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.setViewportSize({ width: 1440, height: 1100 });
  }
  await applyLanguage(page, 'en');
  await (
    await openPositionCalculation(page)
  )
    .getByRole('button', { name: catalogs.en['analysis.analyze'], exact: true })
    .click();
  const bestMarker = analysis(page).locator(
    '.analysis-panel [data-candidate-best="true"] .candidate-marker-best',
  );
  await expect(bestMarker).toBeVisible({ timeout: 30_000 });
  await expect(bestMarker).toHaveText(candidateMessages.en.best);
  await applyLanguage(page, 'de');
  await expect(bestMarker).toHaveText(candidateMessages.de.best);
  await page.screenshot({
    path: test.info().outputPath('german-ui.png'),
    fullPage: true,
  });
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  const { dialog, locale } = await openSettings(page);
  await expect(
    dialog.getByLabel(settingsText(locale, 'language'), { exact: true }),
  ).toHaveValue('de');
  await dialog
    .getByRole('button', { name: settingsText(locale, 'done'), exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  await reopenTutor(page);
  await expect(bestMarker).toBeVisible();
  await expect(bestMarker).toHaveText(candidateMessages.de.best);
  expect(errors).toEqual([]);
});

test('a missing theme image shows a localized preview error and a working fallback after save', async ({
  page,
}) => {
  const theme = themes.find((t) => t.id === 'bgblitz:bg-earthboard');
  test.skip(!theme, 'optional local BG Earth Board artwork');
  await page.route('**/themes/bg-earthboard/background.jpg', (route) =>
    route.abort(),
  );
  await upload(page);
  const { dialog, locale } = await openSettings(page);
  await dialog
    .getByRole('button', { name: settingsText(locale, 'board'), exact: true })
    .click();
  await dialog.getByRole('button', { name: theme!.name, exact: true }).click();
  await expect(
    dialog.locator('.settings-preview .theme-load-error'),
  ).toHaveText(catalogs.de['board.assetError']);
  await expect(
    dialog.locator('.settings-preview .theme-forest .board'),
  ).toBeVisible();
  await expect(analysis(page).locator('.theme-load-error')).toHaveCount(0);
  await applyAndCloseSettings(dialog, locale);
  await expect(analysis(page).locator('.theme-load-error')).toHaveText(
    catalogs.de['board.assetError'],
  );
  await expect(analysis(page).locator('.theme-forest .board')).toBeVisible();
  await applyLanguage(page, 'en');
  await expect(analysis(page).locator('.theme-load-error')).toHaveText(
    catalogs.en['board.assetError'],
  );
  await expect(analysis(page).locator('.theme-forest .board')).toBeVisible();
});
