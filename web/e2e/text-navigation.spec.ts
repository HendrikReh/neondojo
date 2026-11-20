import { openMatchAnalysisOptions } from './analysis-controls';
import { expect, test, type Locator, type Page } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });
async function fixture(page: Page, theme = 'graphite', mode = 'light') {
  await page.addInitScript(
    ({ theme, mode }) => {
      localStorage.setItem('neondojo.locale', 'en');
      localStorage.setItem(
        'neondojo.ui-appearance',
        JSON.stringify({ version: 1, theme, mode }),
      );
    },
    { theme, mode },
  );
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'text-navigation.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[Navigation ${crypto.randomUUID()}]PB[Mochy]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
        ),
      },
    },
  });
  expect(response.ok()).toBe(true);
  const imported = await response.json();
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'neondojo.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    imported.match.id,
  );
  await page.route('**/api/positions/*/analysis-result?*', (route) =>
    route.fulfill({ json: null }),
  );
  await page.route('**/api/positions/*/opening-book', (route) =>
    route.fulfill({
      json: {
        hit: true,
        provider: 'automatic',
        checker: { plies: 2, candidates: [] },
        cube: { plies: 2, verdict: 'No double', equities: {} },
        provenance: {
          provider: 'backgammondb',
          engine: 'Reference GNUbg',
          equity_kind: 'cubeful',
          evaluation_kind: 'evaluation',
          context: 'Navigation fixture',
        },
      },
    }),
  );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return imported.match.id;
}
async function accent(page: Page) {
  return page.evaluate(() => {
    const span = document.createElement('span');
    span.style.color = 'var(--ui-accent)';
    document.body.append(span);
    const color = getComputedStyle(span).color;
    span.remove();
    return color;
  });
}
async function activeText(item: Locator, color: string) {
  const siblings = await item
    .locator('..')
    .locator(':scope > button')
    .evaluateAll((nodes) =>
      nodes.map((node) => {
        const rect = node.getBoundingClientRect();
        return { x: rect.x, y: rect.y, right: rect.right };
      }),
    );
  for (let i = 1; i < siblings.length; i++)
    if (Math.abs(siblings[i].y - siblings[i - 1].y) < 2)
      expect(siblings[i].x - siblings[i - 1].right).toBeGreaterThanOrEqual(12);
  await expect(item).toBeVisible();
  await expect(item).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await expect(item).toHaveCSS('border-top-width', '0px');
  await expect(item).toHaveCSS('border-bottom-width', '2px');
  await expect(item).toHaveCSS('border-bottom-color', color);
  await expect(item).toHaveCSS('border-radius', '0px');
  await expect(item).toHaveCSS('color', color);
  await expect(item).toHaveCSS('font-weight', '600');
  await item.hover();
  await expect(item).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await expect(item).toHaveCSS('border-bottom-color', color);
}
for (const theme of ['graphite', 'club', 'paper'])
  for (const mode of ['light', 'dark'])
    test(`${theme} ${mode}: navigation uses text and active underlines throughout the workspaces`, async ({
      page,
    }) => {
      await fixture(page, theme, mode);
      const color = await accent(page);
      await activeText(page.locator('.app-tabs [aria-selected="true"]'), color);
      await activeText(
        page.locator('.analysis-detail-navigation [aria-selected="true"]'),
        color,
      );
      await activeText(
        page.locator('.board-view-buttons [aria-pressed="true"]'),
        color,
      );
      await activeText(
        page
          .getByRole('tablist', { name: 'Results', exact: true })
          .getByRole('tab', { selected: true }),
        color,
      );
      const selection = page.getByLabel('Displayed analysis', { exact: true });
      await expect(selection).toBeVisible();
      await expect(selection).toHaveValue('');
      await page.locator('.analysis-book-reference > summary').click();
      await expect(page.locator('.analysis-book-reference')).toHaveAttribute(
        'open',
        '',
      );
      await activeText(
        page.locator('.source-book-selector [aria-pressed="true"]'),
        color,
      );
      await activeText(
        page.locator('.source-decision-buttons [aria-pressed="true"]'),
        color,
      );
      await expect(
        page.getByRole('tab', { name: 'Imported analysis', exact: true }),
      ).toHaveCount(0);
      await page
        .locator('.source-decision-buttons')
        .getByRole('button', { name: 'Cube', exact: true })
        .click();
      await activeText(
        page.locator('.source-decision-buttons [aria-pressed="true"]'),
        color,
      );
      await openMatchAnalysisOptions(page);
      const action = page.getByRole('button', {
        name: 'Recalculate position …',
        exact: true,
      });
      await expect(action).not.toHaveCSS(
        'background-color',
        'rgba(0, 0, 0, 0)',
      );
      await page.getByRole('tab', { name: 'Notes', exact: true }).click();
      await activeText(
        page.locator('.notes-scope-tabs [aria-selected="true"]'),
        color,
      );
      await page.getByRole('button', { name: 'Settings', exact: true }).click();
      const settings = page.getByRole('dialog', {
        name: 'Settings',
        exact: true,
      });
      await activeText(settings.locator('.settings-categories .active'), color);
      await settings
        .getByRole('button', { name: 'Appearance', exact: true })
        .click();
      await activeText(settings.locator('.settings-categories .active'), color);
      await activeText(settings.locator('.ui-preview-tab'), color);
      await settings
        .getByRole('button', { name: 'Board', exact: true })
        .click();
      await expect(
        settings.locator('.settings-theme-choice').first(),
      ).toHaveCSS('border-top-width', '1px');
      await settings
        .getByRole('button', { name: 'Close', exact: true })
        .click();
      await page
        .getByRole('tab', { name: 'Match database', exact: true })
        .click();
      await activeText(
        page.locator('.database-collections [aria-pressed="true"]'),
        color,
      );
      const collections = page.locator('.database-collections');
      for (const name of ['Unsorted', 'Bot Matches', 'Paused Matches']) {
        const entry = collections.getByRole('button', {
          name: new RegExp(`\\b${name}\\b`),
        });
        const icon = entry.locator('[data-action-glyph="system-database"]');
        await expect(icon).toBeVisible();
        await expect(entry).toHaveAccessibleDescription(/System database/);
        await expect(icon).toHaveCSS('color', color);
        await expect(icon).not.toHaveCSS(
          'background-color',
          'rgba(0, 0, 0, 0)',
        );
      }
      await page.locator('.database-collections button').last().click();
      await activeText(
        page.locator('.database-collections [aria-pressed="true"]'),
        color,
      );
      for (const name of ['Positions', 'Study']) {
        await page.getByRole('tab', { name, exact: true }).click();
        await activeText(
          page.locator('.app-tabs [aria-selected="true"]'),
          color,
        );
      }
    });

test('header tabs follow the workspace order with mouse and keyboard', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'de'));
  await page.goto('/');
  const header = page.getByRole('tablist', { name: 'Arbeitsbereiche' });
  const tabs = header.getByRole('tab');
  const labels = [
    'Spielen',
    'Analyse',
    'Lernen',
    'Positionen',
    'Matchdatenbank',
    'Spielerdatenbank',
    'Extras',
  ];
  await expect(tabs).toHaveText(labels);
  await expect(header.getByRole('tab', { selected: true })).toHaveText(
    'Analyse',
  );
  await tabs.first().click();
  for (const label of labels.slice(1)) {
    await page.keyboard.press('ArrowRight');
    const tab = header.getByRole('tab', { name: label, exact: true });
    await expect(tab).toBeFocused();
    await expect(tab).toHaveAttribute('aria-selected', 'true');
  }
  await page.keyboard.press('ArrowLeft');
  await expect(tabs.nth(5)).toBeFocused();
  await page.keyboard.press('End');
  await expect(tabs.last()).toBeFocused();
  await page.keyboard.press('Home');
  await expect(tabs.first()).toBeFocused();
  await expect(tabs.first()).toHaveAttribute('aria-selected', 'true');
});

test('text tabs retain keyboard focus, selection and desktop containment', async ({
  page,
}) => {
  await fixture(page);
  const color = await accent(page);
  const overview = page.getByRole('tab', { name: 'Checker', exact: true });
  const positionDetails = overview.locator('..');
  await expect(
    positionDetails.getByRole('tab', { name: 'Evaluation', exact: true }),
  ).toHaveCount(0);
  const overviewPanel = page.getByRole('tabpanel', {
    name: 'Checker',
    exact: true,
  });
  for (const name of ['Candidate moves', 'Dice equity', 'Game rolls'])
    await expect(
      overviewPanel.getByRole('tab', { name, exact: true }),
    ).toBeVisible();
  await overview.focus();
  await page.keyboard.press('ArrowRight');
  const statistics = positionDetails.getByRole('tab', {
    name: 'Statistics',
    exact: true,
  });
  await expect(statistics).toBeFocused();
  await activeText(statistics, color);
  await expect(statistics).toHaveCSS('outline-style', 'solid');
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  const scopes = page.locator('.notes-scope-tabs [role="tab"]');
  await scopes.first().focus();
  await page.keyboard.press('ArrowRight');
  await expect(scopes.nth(1)).toBeFocused();
  await activeText(scopes.nth(1), color);
  await page.keyboard.press('End');
  await expect(scopes.last()).toBeFocused();
  await activeText(scopes.last(), color);
  await page.getByRole('tab', { name: 'Checker', exact: true }).click();
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`navigation-${viewport.width}.png`),
    });
  }
});
