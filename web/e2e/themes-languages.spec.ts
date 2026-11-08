import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { catalogs } from '../src/i18n-catalogs';

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

async function upload(page: import('@playwright/test').Page) {
  await page.goto('/');
  await page
    .locator('input[type=file]')
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  await expect(page.locator('.board')).toBeVisible();
}

async function assertArtwork(page: import('@playwright/test').Page) {
  const failures = await page
    .locator('.bgblitz-board')
    .evaluate(async (svg) => {
      const urls = [
        ...new Set(
          [...svg.querySelectorAll('image')].map((node) =>
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

test('every local ZIP theme uses its original assets and survives reload on desktop/mobile', async ({
  page,
}) => {
  test.skip(!themes.length, 'optional local theme archives');
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await upload(page);
  for (const theme of themes) {
    await page.getByLabel('Board-Theme').selectOption(theme.id);
    await expect(page.locator('.bgblitz-board')).toHaveAttribute(
      'data-theme',
      theme.id,
    );
    await expect(page.locator('.bgblitz-board')).toHaveAttribute(
      'viewBox',
      `0 0 ${theme.background.width} ${theme.background.height}`,
    );
    await assertArtwork(page);
    await expect(page.locator('[data-point="24"]')).toHaveAttribute(
      'data-count',
      '2',
    );
    await expect(page.locator('[data-point="1"]')).toHaveAttribute(
      'data-count',
      '-2',
    );
    await expect(page.locator('[data-number-orientation]')).toHaveAttribute(
      'data-number-orientation',
      'RLD',
    );
    await page.getByRole('button', { name: 'Nächster Zug' }).click();
    await expect(page.locator('[data-number-orientation]')).toHaveAttribute(
      'data-number-orientation',
      'RLU',
    );
    await page.getByRole('button', { name: 'Vorheriger Zug' }).click();
  }
  const selected = themes.find((t) => t.id === 'bgblitz:cl_green') || themes[0];
  await page.getByLabel('Board-Theme').selectOption(selected.id);
  await page
    .locator('.board-panel')
    .screenshot({ path: 'test-results/bgblitz-desktop.png' });
  await page.reload();
  await expect(page.getByLabel('Board-Theme')).toHaveValue(selected.id);
  await page
    .locator('.match-card')
    .filter({ hasText: 'Tutor Demo' })
    .first()
    .click();
  await expect(page.locator('.bgblitz-board')).toHaveAttribute(
    'data-theme',
    selected.id,
  );
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
  test.skip(
    !themes.some((t) => t.id === 'bgblitz:cl_green'),
    'optional local Rain artwork',
  );
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({
    name: 'theme-position.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '(;GM[6]PW[Theme White]PB[Theme Black]MI[length:7][ws:2][bs:3]AE[a:y]AW[y][f]AB[y][s]PL[W]CV[4]CO[b];W[61ysfe])',
    ),
  });
  await expect(page.locator('.board')).toBeVisible();
  await page.getByLabel('Board-Theme').selectOption('bgblitz:cl_green');
  await expect(page.locator('[data-point="bar-white"]')).toHaveAttribute(
    'data-count',
    '1',
  );
  await expect(page.locator('[data-point="bar-black"]')).toHaveAttribute(
    'data-count',
    '-1',
  );
  await expect(page.locator('[data-point="off-white"]')).toHaveAttribute(
    'data-count',
    '13',
  );
  await expect(page.locator('[data-point="off-black"]')).toHaveAttribute(
    'data-count',
    '-13',
  );
  await expect(page.locator('[data-cube]')).toHaveAttribute('data-cube', '4');
  await expect(page.locator('[data-cube]')).toHaveAttribute(
    'data-cube-owner',
    '1',
  );
  await expect(page.locator('[data-cube] image')).toHaveAttribute(
    'href',
    /cube4\.png$/,
  );
  await expect(page.locator('[data-cube] image')).toHaveAttribute('x', '57');
  await assertArtwork(page);
});

test('all six languages switch complete views live and persist with the theme', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await upload(page);
  const selector = page.locator('.language-picker select');
  for (const locale of ['es', 'fr', 'pl', 'it', 'en', 'de'] as const) {
    await selector.selectOption(locale);
    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    await expect(page.locator('h1')).toHaveText(
      catalogs[locale]['app.selectedTitle'],
    );
    await expect(
      page.getByRole('button', {
        name: catalogs[locale]['analysis.analyze'],
        exact: true,
      }),
    ).toBeVisible();
    await expect(page.locator('.match-analysis h3')).toHaveText(
      catalogs[locale]['matchAnalysis.title'],
    );
    await expect(
      page.getByRole('img', {
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
  await selector.selectOption('en');
  await page
    .getByRole('button', { name: catalogs.en['analysis.analyze'], exact: true })
    .click();
  await expect(
    page.getByText(catalogs.en['analysis.bestMove'], { exact: true }),
  ).toBeVisible({ timeout: 30_000 });
  await selector.selectOption('fr');
  await expect(
    page.getByText(catalogs.fr['analysis.bestMove'], { exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: 'test-results/french-ui.png', fullPage: true });
  await page.reload();
  await expect(selector).toHaveValue('fr');
  await expect(page.locator('html')).toHaveAttribute('lang', 'fr');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Tutor Demo' })
    .first()
    .click();
  await expect(
    page.getByText(catalogs.fr['analysis.bestMove'], { exact: true }),
  ).toBeVisible();
  expect(errors).toEqual([]);
});

test('a missing theme image shows a localized error and a working fallback board', async ({
  page,
}) => {
  test.skip(
    !themes.some((t) => t.id === 'bgblitz:bg-earthboard'),
    'optional local BG Earth Board artwork',
  );
  await page.route('**/themes/bg-earthboard/background.jpg', (route) =>
    route.abort(),
  );
  await upload(page);
  await page.getByLabel('Board-Theme').selectOption('bgblitz:bg-earthboard');
  await expect(page.locator('.theme-load-error')).toHaveText(
    catalogs.de['board.assetError'],
  );
  await expect(page.locator('.theme-forest .board')).toBeVisible();
  await page.locator('.language-picker select').selectOption('es');
  await expect(page.locator('.theme-load-error')).toHaveText(
    catalogs.es['board.assetError'],
  );
});
