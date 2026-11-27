import { openMatchAnalysisOptions } from '../helpers/analysis-controls';
import { fulfillPositionAnalysisResult } from '../helpers/api-fixtures';
import { importSgfMatch } from '../helpers/match-fixtures';
import {
  DESKTOP_VIEWPORTS,
  gotoAnalysisBoard,
  seedLocale,
  seedStorage,
} from '../helpers/navigation';
import { expect, test, type Locator, type Page } from '../helpers/test';
import { uiThemes } from '../../src/settings/appearance/ui-appearance';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });
async function fixture(
  page: Page,
  theme = 'graphite',
  mode: 'light' | 'dark' = 'light',
) {
  const { match } = await importSgfMatch(page, {
    filename: 'text-navigation.sgf',
    game: { white: `Navigation ${crypto.randomUUID()}`, black: 'Mochy' },
  });
  await seedStorage(page, {
    locale: 'en',
    uiAppearance: { theme, mode },
    workspace: { matchId: match.id, gameNo: 1 },
  });
  await page.route('**/api/positions/*/analysis-result?*', (route) =>
    fulfillPositionAnalysisResult(route, null),
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
  await gotoAnalysisBoard(page);
  return match.id;
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

async function expectHiddenContentOutOfLayout(page: Page) {
  await expect(page.locator('.app-shell [hidden]:visible')).toHaveCount(0);
}

for (const theme of uiThemes) {
  for (const mode of ['light', 'dark'] as const) {
    test(`Benchmark mode buttons use the shared navigation in ${theme} ${mode}`, async ({
      page,
    }) => {
      await fixture(page, theme, mode);
      const color = await accent(page);
      await page.getByRole('tab', { name: 'Extras', exact: true }).click();
      const switcher = page.getByRole('group', { name: 'Measurement type' });
      const analysis = switcher.getByRole('button', { name: 'Match analysis' });
      const play = switcher.getByRole('button', { name: 'Play-turn latency' });
      await expect(analysis).toHaveAttribute('aria-pressed', 'true');
      await activeText(analysis, color);

      await analysis.focus();
      await page.keyboard.press('Tab');
      await expect(play).toBeFocused();
      await expect(play).toHaveCSS('outline-color', color);
      await page.keyboard.press('Enter');
      await expect(play).toHaveAttribute('aria-pressed', 'true');
      await expect(analysis).toHaveAttribute('aria-pressed', 'false');
      await expect(page.getByLabel('Playing depth')).toBeVisible();
      await activeText(play, color);

      await page.keyboard.press('Shift+Tab');
      await expect(analysis).toBeFocused();
      await page.keyboard.press('Space');
      await expect(analysis).toHaveAttribute('aria-pressed', 'true');
      await expect(page.getByLabel('Playing depth')).toHaveCount(0);
    });
  }
}

for (const [theme, mode] of uiThemes.flatMap((theme) =>
  (['light', 'dark'] as const).map((mode) => [theme, mode] as const),
))
  test(`${theme} ${mode}: navigation uses text and active underlines throughout the workspaces`, async ({
    page,
  }) => {
    await fixture(page, theme, mode);
    await expectHiddenContentOutOfLayout(page);
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
    await expect(action).not.toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expectHiddenContentOutOfLayout(page);
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
    await settings.getByRole('button', { name: 'Board', exact: true }).click();
    await expect(settings.locator('.settings-theme-choice').first()).toHaveCSS(
      'border-top-width',
      '1px',
    );
    await settings.getByRole('button', { name: 'Close', exact: true }).click();
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expectHiddenContentOutOfLayout(page);
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
      await expect(icon).not.toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
    }
    await page.locator('.database-collections button').last().click();
    await activeText(
      page.locator('.database-collections [aria-pressed="true"]'),
      color,
    );
    for (const name of ['Positions', 'Study']) {
      await page.getByRole('tab', { name, exact: true }).click();
      await expectHiddenContentOutOfLayout(page);
      await activeText(page.locator('.app-tabs [aria-selected="true"]'), color);
    }
  });

test('header tabs follow the workspace order with mouse and keyboard', async ({
  page,
}) => {
  await seedLocale(page, 'de');
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
  await expect(
    page.getByRole('heading', { name: 'Startseite', exact: true }),
  ).toBeVisible();
  await expect(header.getByRole('tab', { selected: true })).toHaveCount(0);
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
  const help = page.getByRole('banner').getByText('Hilfe', { exact: true });
  const manuals = [
    ['NeonDojo-Handbuch', 'https://hendrikreh.github.io/neondojo/user-guide/'],
    ['NeonGammon-Handbuch', 'https://hendrikreh.github.io/neongammon/'],
  ] as const;
  for (const [name] of manuals) {
    await expect(page.getByRole('link', { name, exact: true })).toBeHidden();
  }
  await page.keyboard.press('Tab');
  await expect(help).toBeFocused();
  await expect(help).toHaveCSS('list-style-type', 'none');
  const extrasBox = (await tabs.last().boundingBox())!;
  const helpBox = (await help.boundingBox())!;
  expect(helpBox.x).toBe(extrasBox.x + extrasBox.width + 20);
  expect(helpBox.y).toBe(extrasBox.y);
  await page.keyboard.press('Enter');
  const menuBox = (await page.locator('.app-help-links').boundingBox())!;
  expect(menuBox.x).toBeGreaterThanOrEqual(helpBox.x - 1);
  expect(menuBox.x).toBeGreaterThan(extrasBox.x + extrasBox.width);
  for (const [name, href] of manuals) {
    const link = page.getByRole('link', { name, exact: true });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute('href', href);
    await expect(link).toHaveAttribute('target', '_blank');
    await expect(link).toHaveAccessibleDescription('Öffnet in einem neuen Tab');
    await expect(link.locator('.app-help-external')).toHaveText('↗');
    await expect(link.locator('.app-help-external')).toHaveAttribute(
      'aria-hidden',
      'true',
    );
    await page.keyboard.press('Tab');
    await expect(link).toBeFocused();
  }
  // Tabbing past the last manual link closes the Help disclosure.
  await page.keyboard.press('Tab');
  await expect(page.locator('.app-help')).not.toHaveAttribute('open', '');
  await expect(page.getByRole('link', { name: manuals[0][0] })).toBeHidden();

  // Escape closes and returns focus to the summary trigger.
  await help.click();
  await expect(page.getByRole('link', { name: manuals[0][0] })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(help).toBeFocused();
  await expect(page.getByRole('link', { name: manuals[0][0] })).toBeHidden();

  // Clicking a manual link closes the Help disclosure cleanly.
  await help.click();
  const firstManual = page.getByRole('link', {
    name: manuals[0][0],
    exact: true,
  });
  await expect(firstManual).toBeVisible();
  await page.evaluate(() => {
    document.querySelector('.app-help-links a')?.addEventListener(
      'click',
      (event) => event.preventDefault(),
      { once: true },
    );
  });
  await firstManual.click();
  await expect(page.locator('.app-help')).not.toHaveAttribute('open', '');
  await expect(firstManual).toBeHidden();

  await help.click();
  await expect(page.getByRole('link', { name: manuals[0][0] })).toBeVisible();
  await tabs.last().click();
  await expect(page.getByRole('link', { name: manuals[0][0] })).toBeHidden();
  await page.keyboard.press('Home');
  await expect(tabs.first()).toBeFocused();
  await expect(tabs.first()).toHaveAttribute('aria-selected', 'true');
});

test('Home and Analysis startup defer /api/databases until Match database opens', async ({
  page,
}) => {
  let databaseRequests = 0;
  page.on('request', (req) => {
    if (new URL(req.url()).pathname === '/api/databases') databaseRequests++;
  });
  await seedLocale(page, 'en');
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: 'Home', exact: true }),
  ).toBeVisible();
  await page.waitForLoadState('networkidle');
  expect(databaseRequests).toBe(0);

  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await page.waitForLoadState('networkidle');
  expect(databaseRequests).toBe(0);

  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect
    .poll(() => databaseRequests)
    .toBeGreaterThanOrEqual(1);
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
  // Every AnalysisPanel sub-tab points to a real role="tabpanel" with matching aria-labelledby.
  const resultsTabs = overviewPanel
    .getByRole('tablist', { name: 'Results', exact: true })
    .getByRole('tab');
  const resultsTabCount = await resultsTabs.count();
  expect(resultsTabCount).toBe(4);
  for (let i = 0; i < resultsTabCount; i++) {
    const tab = resultsTabs.nth(i);
    const tabId = await tab.getAttribute('id');
    const controls = await tab.getAttribute('aria-controls');
    expect(tabId).toBeTruthy();
    expect(controls).toBeTruthy();
    const panel = page.locator(`[id="${controls}"]`);
    await expect(panel).toHaveAttribute('role', 'tabpanel');
    await expect(panel).toHaveAttribute('aria-labelledby', tabId!);
  }
  await overview.focus();
  await page.keyboard.press('ArrowRight');
  const statistics = positionDetails.getByRole('tab', {
    name: 'Statistics',
    exact: true,
  });
  await expect(statistics).toBeFocused();
  await activeText(statistics, color);
  await expect(statistics).toHaveCSS('outline-style', 'solid');
  // Switching away from Checker keeps #inspector-panel-overview as a hidden tabpanel,
  // never leaving an unroled id on .analysis-grid.
  await expect(page.locator('.analysis-grid')).not.toHaveAttribute(
    'id',
    'inspector-panel-overview',
  );
  await expect(page.locator('#inspector-panel-overview')).toHaveAttribute(
    'role',
    'tabpanel',
  );
  await expect(page.locator('#inspector-panel-overview')).toBeHidden();
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
  for (const viewport of DESKTOP_VIEWPORTS) {
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
